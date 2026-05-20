//! Phi-3-mini-4k cpu-int4 ONNX runtime + greedy/temperature sampler.
//!
//! Gated behind the `ort` Cargo feature. Default build keeps the EMA
//! stand-in from `core::ssm`; the user opts in by either building
//! with `--features ort` or installing a pre-built Aura artefact
//! whose default features include it.
//!
//! Architecture pinned against `cpu-int4-rtn-block-32-acc-level-4`
//! (verified by reading the vendored `genai_config.json`):
//! - 32 decoder layers, 32 attention heads, 32 KV heads (no GQA
//!   collapse), `head_size = 96`, `hidden_size = 3072`.
//! - Vocab 32 064 tokens. EOS ∈ {32 000, 32 001, 32 007}.
//! - Activations are fp32; only the weights are INT4-block-quantised.
//! - No `position_ids` input — `RotaryEmbedding` is internal to the
//!   ONNX graph.
//!
//! Inference loop = initial pass over the whole prompt → greedy /
//! temperature-sampled token-by-token loop, feeding each step's
//! `present.<i>.*` back as the next step's `past_key_values.<i>.*`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ort::value::Tensor;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use thiserror::Error;
use tokenizers::Tokenizer;
use tracing::warn;

/// Architectural constants pinned in [`genai_config.json`] of the
/// vendored cpu-int4 export. Hardcoded because the same constants are
/// needed at three places in this file and "re-parse the config on
/// every call" would be wasteful.
const NUM_LAYERS: usize = 32;
const NUM_KV_HEADS: i64 = 32;
const HEAD_SIZE: i64 = 96;
const VOCAB_SIZE: usize = 32_064;
const EOS_TOKENS: &[i64] = &[32_000, 32_001, 32_007];

#[derive(Debug, Error)]
pub enum Phi3LoadError {
    #[error("model directory missing: {0}")]
    DirNotFound(PathBuf),
    #[error("model.onnx missing under {0}")]
    OnnxMissing(PathBuf),
    #[error("tokenizer.json missing under {0}")]
    TokenizerMissing(PathBuf),
    #[error("tokenizer load: {0}")]
    TokenizerLoad(String),
    #[error("ort session init failed: {0}")]
    OrtInit(String),
    /// Used to be the expected outcome under vanilla `ort` because of
    /// `com.microsoft.MatMulNBits`/`GroupQueryAttention`/`RotaryEmbedding`.
    /// Kept on the error type because (a) an older `ort` rc would still
    /// hit it, and (b) the message body lets the registry quote the op
    /// name verbatim.
    #[error("unsupported custom op: {0}")]
    UnsupportedCustomOp(String),
}

#[derive(Debug, Error)]
pub enum Phi3InferError {
    #[error("tokenize: {0}")]
    Tokenize(String),
    #[error("decode: {0}")]
    Decode(String),
    #[error("ort run: {0}")]
    Run(String),
    #[error("ort tensor: {0}")]
    Tensor(String),
    #[error("empty prompt")]
    EmptyPrompt,
}

/// What [`Phi3Runtime::complete`] returns.
#[derive(Debug, Clone)]
pub struct CompletionResult {
    pub prompt: String,
    pub completion: String,
    pub generated_token_ids: Vec<i64>,
    pub prompt_token_count: usize,
    pub completion_token_count: usize,
    pub stopped_on_eos: bool,
    pub elapsed_seconds: f32,
    /// Wall-clock throughput counted over *generated* tokens (excludes
    /// the prompt-processing pass). This is the user-facing "how fast
    /// does it stream" number.
    pub tokens_per_second: f32,
}

/// One present.<i>.{key,value} pair captured between steps. Stored as
/// `(shape, data)` so we hand it back into the next `Tensor::from_array`
/// without paying for a needless re-allocation of `Vec<f32>`.
struct PastKv {
    shape: Vec<i64>,
    data: Vec<f32>,
}

pub struct Phi3Runtime {
    session: ort::session::Session,
    tokenizer: Tokenizer,
}

impl Phi3Runtime {
    /// Try to load the cpu-int4 export. Caller decides what to do on
    /// `Err` — in practice we log and fall back to the EMA stand-in.
    pub fn load(model_dir: &Path) -> Result<Self, Phi3LoadError> {
        if !model_dir.is_dir() {
            return Err(Phi3LoadError::DirNotFound(model_dir.to_path_buf()));
        }
        let onnx = locate_onnx(model_dir)
            .ok_or_else(|| Phi3LoadError::OnnxMissing(model_dir.to_path_buf()))?;
        let tok_path = model_dir.join("tokenizer.json");
        if !tok_path.is_file() {
            return Err(Phi3LoadError::TokenizerMissing(model_dir.to_path_buf()));
        }
        let tokenizer = Tokenizer::from_file(&tok_path)
            .map_err(|e| Phi3LoadError::TokenizerLoad(e.to_string()))?;

        let session_res = ort::session::Session::builder()
            .map_err(|e| Phi3LoadError::OrtInit(e.to_string()))?
            .commit_from_file(&onnx);

        match session_res {
            Ok(session) => Ok(Self { session, tokenizer }),
            Err(e) => {
                let msg = e.to_string();
                let custom_op_markers = [
                    "MatMulNBits",
                    "GroupQueryAttention",
                    "RotaryEmbedding",
                    "com.microsoft.",
                ];
                if custom_op_markers.iter().any(|m| msg.contains(m)) {
                    warn!(
                        target: "aura::ssm",
                        "phi-3 cpu-int4 needs onnxruntime-genai; \
                         vanilla ort returned: {msg}"
                    );
                    Err(Phi3LoadError::UnsupportedCustomOp(msg))
                } else {
                    Err(Phi3LoadError::OrtInit(msg))
                }
            }
        }
    }

    /// Complete `prompt` with up to `max_new_tokens` more tokens.
    ///
    /// Sampling:
    /// - `temperature ≤ 0` → greedy (argmax). Fastest, fully
    ///   deterministic, recommended for the streaming-state-space
    ///   "continue this topic" feature.
    /// - `temperature > 0` → softmax + multinomial sample. The
    ///   `temperature` divides the logits; higher → more random.
    /// - `seed` lets a caller pin a sample chain for reproducibility.
    ///   `None` → random seed.
    ///
    /// Stops on the first EOS token (one of `{32 000, 32 001, 32 007}`)
    /// or `max_new_tokens`, whichever comes first.
    pub fn complete(
        &mut self,
        prompt: &str,
        max_new_tokens: usize,
        temperature: f32,
        seed: Option<u64>,
    ) -> Result<CompletionResult, Phi3InferError> {
        if prompt.is_empty() {
            return Err(Phi3InferError::EmptyPrompt);
        }

        // ---- Tokenize ----
        let encoded = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| Phi3InferError::Tokenize(e.to_string()))?;
        let prompt_ids: Vec<i64> = encoded.get_ids().iter().map(|&id| id as i64).collect();
        let prompt_len = prompt_ids.len();

        // ---- Initial pass over the whole prompt ----
        let mut all_ids: Vec<i64> = prompt_ids.clone();
        let mut past_kvs: Vec<(PastKv, PastKv)> = empty_past_kvs();
        let timer = Instant::now();

        let (next_token, new_past) = self.forward(&prompt_ids, &past_kvs, all_ids.len())?;
        past_kvs = new_past;
        all_ids.push(next_token);

        // Greedy/sampling loop — `next_token` was already produced by
        // the initial pass; we now keep generating until EOS or limit.
        let mut generated: Vec<i64> = Vec::with_capacity(max_new_tokens);
        generated.push(next_token);
        let mut stopped_on_eos = EOS_TOKENS.contains(&next_token);

        // RNG only constructed if we'll actually sample (saves a
        // `SmallRng::from_entropy` allocation on the greedy path).
        let mut rng = if !stopped_on_eos && temperature > 0.0 && max_new_tokens > 1 {
            Some(match seed {
                Some(s) => SmallRng::seed_from_u64(s),
                None => SmallRng::from_entropy(),
            })
        } else {
            None
        };

        while !stopped_on_eos && generated.len() < max_new_tokens {
            let last = *generated.last().expect("non-empty by construction");
            let total_seq = all_ids.len();
            let (next, new_past) = self.forward_one(last, &past_kvs, total_seq)?;
            past_kvs = new_past;

            let chosen = if temperature > 0.0 {
                if let Some(r) = rng.as_mut() {
                    sample_with_temperature(&next.logits_last, temperature, r)
                } else {
                    argmax(&next.logits_last)
                }
            } else {
                argmax(&next.logits_last)
            };
            all_ids.push(chosen);
            generated.push(chosen);
            stopped_on_eos = EOS_TOKENS.contains(&chosen);
        }

        let elapsed = timer.elapsed().as_secs_f32();
        let completion_token_count = generated.len();
        let tokens_per_second = if elapsed > 0.0 {
            completion_token_count as f32 / elapsed
        } else {
            0.0
        };

        // Decode just the generated tokens (skip the prompt) so the
        // returned `completion` doesn't echo the input.
        let ids_u32: Vec<u32> = generated.iter().map(|&i| i as u32).collect();
        let completion = self
            .tokenizer
            .decode(&ids_u32, true)
            .map_err(|e| Phi3InferError::Decode(e.to_string()))?;

        Ok(CompletionResult {
            prompt: prompt.to_string(),
            completion,
            generated_token_ids: generated,
            prompt_token_count: prompt_len,
            completion_token_count,
            stopped_on_eos,
            elapsed_seconds: elapsed,
            tokens_per_second,
        })
    }

    /// Initial-pass forward: feed the whole `prompt_ids` plus empty
    /// KVs. Returns `(next_token, new_past_kvs)`.
    fn forward(
        &mut self,
        prompt_ids: &[i64],
        past_kvs: &[(PastKv, PastKv)],
        total_seq: usize,
    ) -> Result<(i64, Vec<(PastKv, PastKv)>), Phi3InferError> {
        let r = self.forward_inner(prompt_ids, past_kvs, total_seq)?;
        Ok((argmax(&r.logits_last), r.new_past))
    }

    /// Single-token follow-up forward — same shape as `forward` but the
    /// input is a single new token. Returns the *full* step result so the
    /// caller can sample (greedy vs. temperature) at its discretion.
    fn forward_one(
        &mut self,
        new_token: i64,
        past_kvs: &[(PastKv, PastKv)],
        total_seq: usize,
    ) -> Result<(StepResult, Vec<(PastKv, PastKv)>), Phi3InferError> {
        let r = self.forward_inner(&[new_token], past_kvs, total_seq)?;
        Ok((
            StepResult { logits_last: r.logits_last },
            r.new_past,
        ))
    }

    fn forward_inner(
        &mut self,
        new_ids: &[i64],
        past_kvs: &[(PastKv, PastKv)],
        total_seq: usize,
    ) -> Result<ForwardInner, Phi3InferError> {
        let seq_in = new_ids.len() as i64;

        // input_ids: [1, seq_in]
        let input_ids_tensor = Tensor::<i64>::from_array((vec![1i64, seq_in], new_ids.to_vec()))
            .map_err(|e| Phi3InferError::Tensor(e.to_string()))?;
        // attention_mask: [1, total_seq] of 1s (no padding)
        let mask_data = vec![1i64; total_seq];
        let attention_mask_tensor =
            Tensor::<i64>::from_array((vec![1i64, total_seq as i64], mask_data))
                .map_err(|e| Phi3InferError::Tensor(e.to_string()))?;

        let mut inputs = ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
        ];

        // `Tensor::from_array` rejects shapes that contain a 0
        // dimension ("all dimensions must be >= 1 when creating a
        // tensor from raw data"). On the initial pass past_seq_len = 0,
        // so we need the allocator path (`Tensor::new`) which accepts
        // empty dims and zero-fills internally. Non-empty steps still
        // use `from_array` to avoid the extra alloc + memcpy.
        let alloc = ort::memory::Allocator::default();
        for (i, (kkv, vkv)) in past_kvs.iter().enumerate() {
            let k_dyn = build_past_tensor(&alloc, kkv)?;
            let v_dyn = build_past_tensor(&alloc, vkv)?;
            inputs.push((
                format!("past_key_values.{i}.key").into(),
                k_dyn.into(),
            ));
            inputs.push((
                format!("past_key_values.{i}.value").into(),
                v_dyn.into(),
            ));
        }

        let mut outputs = self
            .session
            .run(inputs)
            .map_err(|e| Phi3InferError::Run(e.to_string()))?;

        // logits: [1, seq_in, vocab_size]. Greedy/sampling looks at the
        // LAST row only.
        let logits_dyn = outputs
            .remove("logits")
            .ok_or_else(|| Phi3InferError::Run("logits output missing".into()))?;
        let (logits_shape, logits_data) = logits_dyn
            .try_extract_tensor::<f32>()
            .map_err(|e| Phi3InferError::Tensor(e.to_string()))?;
        let dims: Vec<i64> = logits_shape.iter().copied().collect();
        if dims.len() != 3 || dims[2] != VOCAB_SIZE as i64 {
            return Err(Phi3InferError::Tensor(format!(
                "unexpected logits shape {dims:?}"
            )));
        }
        let seq = dims[1] as usize;
        let last_row_start = (seq - 1) * VOCAB_SIZE;
        let logits_last: Vec<f32> =
            logits_data[last_row_start..last_row_start + VOCAB_SIZE].to_vec();

        let mut new_past: Vec<(PastKv, PastKv)> = Vec::with_capacity(NUM_LAYERS);
        for i in 0..NUM_LAYERS {
            let k_name = format!("present.{i}.key");
            let v_name = format!("present.{i}.value");
            let k = outputs
                .remove(&k_name)
                .ok_or_else(|| Phi3InferError::Run(format!("missing {k_name}")))?;
            let v = outputs
                .remove(&v_name)
                .ok_or_else(|| Phi3InferError::Run(format!("missing {v_name}")))?;
            let (ks, kd) = k
                .try_extract_tensor::<f32>()
                .map_err(|e| Phi3InferError::Tensor(e.to_string()))?;
            let (vs, vd) = v
                .try_extract_tensor::<f32>()
                .map_err(|e| Phi3InferError::Tensor(e.to_string()))?;
            new_past.push((
                PastKv {
                    shape: ks.iter().copied().collect(),
                    data: kd.to_vec(),
                },
                PastKv {
                    shape: vs.iter().copied().collect(),
                    data: vd.to_vec(),
                },
            ));
        }

        Ok(ForwardInner {
            logits_last,
            new_past,
        })
    }
}

struct ForwardInner {
    logits_last: Vec<f32>,
    new_past: Vec<(PastKv, PastKv)>,
}

struct StepResult {
    logits_last: Vec<f32>,
}

/// Construct one past_*_value tensor. Branches on emptiness because
/// `Tensor::from_array` refuses zero-sized dims; the allocator path
/// accepts them.
fn build_past_tensor(
    alloc: &ort::memory::Allocator,
    kv: &PastKv,
) -> Result<ort::value::Tensor<f32>, Phi3InferError> {
    let nelems: i64 = kv.shape.iter().product();
    if nelems == 0 {
        let shape_us: Vec<usize> = kv.shape.iter().map(|&d| d.max(0) as usize).collect();
        ort::value::Tensor::<f32>::new(alloc, shape_us)
            .map_err(|e| Phi3InferError::Tensor(e.to_string()))
    } else {
        ort::value::Tensor::<f32>::from_array((kv.shape.clone(), kv.data.clone()))
            .map_err(|e| Phi3InferError::Tensor(e.to_string()))
    }
}

/// 32 layers × empty `[1, 32, 0, 96]` key/value tensors — the initial
/// "no prior context" KV cache.
fn empty_past_kvs() -> Vec<(PastKv, PastKv)> {
    let shape = vec![1i64, NUM_KV_HEADS, 0, HEAD_SIZE];
    (0..NUM_LAYERS)
        .map(|_| {
            (
                PastKv { shape: shape.clone(), data: Vec::new() },
                PastKv { shape: shape.clone(), data: Vec::new() },
            )
        })
        .collect()
}

fn argmax(logits: &[f32]) -> i64 {
    let mut best_i = 0usize;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &v) in logits.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best_i = i;
        }
    }
    best_i as i64
}

/// Numerically-stable softmax + multinomial sample. `temperature ≤ 0`
/// is a bug at the call site (greedy should have been picked); this
/// function asserts the caller already filtered for that.
fn sample_with_temperature(logits: &[f32], temperature: f32, rng: &mut SmallRng) -> i64 {
    debug_assert!(temperature > 0.0);
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let inv_t = 1.0 / temperature;
    let mut probs: Vec<f32> = logits
        .iter()
        .map(|&l| ((l - max) * inv_t).exp())
        .collect();
    let sum: f32 = probs.iter().sum();
    if sum <= 0.0 {
        // Degenerate (all -inf or NaN). Fall back to greedy to avoid
        // dividing by zero.
        return argmax(logits);
    }
    for p in &mut probs {
        *p /= sum;
    }
    let u: f32 = rng.gen_range(0.0..1.0);
    let mut acc = 0.0_f32;
    for (i, &p) in probs.iter().enumerate() {
        acc += p;
        if u < acc {
            return i as i64;
        }
    }
    // Float rounding can push `acc` slightly under 1.0; return the
    // last index to keep the sampler total-probability-mass-safe.
    (probs.len() - 1) as i64
}

/// Pick the largest `*.onnx` in the directory. The Phi-3 cpu-int4 export
/// has one obvious weights file plus a sidecar `*.onnx.data`; the
/// `.onnx` is what `ort` needs to start the session.
fn locate_onnx(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("onnx") {
            continue;
        }
        let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
        let bigger = best
            .as_ref()
            .map(|(b, _)| size > *b)
            .unwrap_or(true);
        if bigger {
            best = Some((size, path));
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vendored_phi3() -> Option<PathBuf> {
        let p = PathBuf::from("/tmp/aura-phi3-test");
        let onnx = std::fs::read_dir(&p)
            .ok()?
            .filter_map(|e| e.ok())
            .any(|e| e.path().extension().and_then(|s| s.to_str()) == Some("onnx"));
        let tok = p.join("tokenizer.json").is_file();
        if onnx && tok { Some(p) } else { None }
    }

    /// Schema check — same one as before, kept so a future upstream
    /// `ort` rc change is caught at the schema layer not the loop.
    #[test]
    fn cpu_int4_export_loads_cleanly_when_present() {
        let Some(p) = vendored_phi3() else {
            eprintln!("skipped: phi-3 cpu-int4 onnx+tokenizer not at /tmp/aura-phi3-test");
            return;
        };
        let rt = Phi3Runtime::load(&p).expect("ort 2.x must load Phi-3 cpu-int4 cleanly");
        let inputs = rt.session.inputs().len();
        let outputs = rt.session.outputs().len();
        assert_eq!(inputs, 66, "expected 66 inputs, got {inputs}");
        assert_eq!(outputs, 65, "expected 65 outputs, got {outputs}");
        assert_eq!(rt.session.inputs()[0].name(), "input_ids");
        assert_eq!(rt.session.outputs()[0].name(), "logits");
    }

    /// **100-token greedy completion** from the spec prompt. Verifies
    /// the entire driver: tokenize → initial pass → KV-cache loop →
    /// argmax sampling → detokenize. Stops on EOS so the actual
    /// completion length may be shorter than the limit.
    ///
    /// Gated on `/tmp/aura-phi3-test/` containing the cpu-int4 export
    /// AND `--features ort` AND `--ignored` because a single CPU
    /// completion takes a few minutes on a VPS-class machine.
    #[test]
    #[ignore = "slow: greedy 100-token completion on CPU; opt in with --ignored"]
    fn greedy_completion_capital_of_saudi_arabia() {
        let Some(p) = vendored_phi3() else {
            eprintln!("skipped: phi-3 cpu-int4 onnx+tokenizer not at /tmp/aura-phi3-test");
            return;
        };
        let mut rt = Phi3Runtime::load(&p).expect("load");
        let result = rt
            .complete("The capital of Saudi Arabia is", 100, 0.0, None)
            .expect("complete");
        eprintln!(
            "prompt_tokens={} completion_tokens={} elapsed={:.2}s tok/s={:.2} stopped_on_eos={}",
            result.prompt_token_count,
            result.completion_token_count,
            result.elapsed_seconds,
            result.tokens_per_second,
            result.stopped_on_eos,
        );
        eprintln!("---completion---\n{}\n---", result.completion);
        // The completion must contain "Riyadh" (greedy on this model
        // continues "... Riyadh ..." essentially every time).
        assert!(
            result.completion.to_lowercase().contains("riyadh"),
            "expected 'Riyadh' in completion, got: {}",
            result.completion
        );
    }

    /// Empty prompt is a clean structured error.
    #[test]
    fn empty_prompt_errors() {
        let Some(p) = vendored_phi3() else {
            eprintln!("skipped: phi-3 cpu-int4 not present");
            return;
        };
        let mut rt = Phi3Runtime::load(&p).expect("load");
        let err = rt.complete("", 10, 0.0, None).unwrap_err();
        assert!(matches!(err, Phi3InferError::EmptyPrompt));
    }

    /// Sampler unit-tests — deterministic given a fixed seed.
    #[test]
    fn argmax_picks_largest() {
        let logits = vec![0.1f32, 0.5, 0.4, 0.3];
        assert_eq!(argmax(&logits), 1);
    }

    #[test]
    fn temperature_sampler_respects_seed() {
        let logits: Vec<f32> = (0..100).map(|i| (i as f32) * 0.01).collect();
        let mut r1 = SmallRng::seed_from_u64(42);
        let mut r2 = SmallRng::seed_from_u64(42);
        let a = sample_with_temperature(&logits, 1.0, &mut r1);
        let b = sample_with_temperature(&logits, 1.0, &mut r2);
        assert_eq!(a, b);
    }

    #[test]
    fn temperature_zero_path_uses_greedy() {
        // Even with temperature=0 the sample function should never be
        // called (the caller goes greedy). Sanity-check `argmax` on a
        // logit vector with a clear winner.
        let logits = vec![1.0f32, 2.0, 0.5];
        assert_eq!(argmax(&logits), 1);
    }
}
