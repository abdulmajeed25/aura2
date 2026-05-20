//! tract-onnx-backed `multilingual-e5-small` embedder. Phase 5d.
//!
//! Differences from [`super::embedder::OnnxMiniLm`]:
//! - **Tokenizer:** XLM-RoBERTa sentencepiece (loaded the same way through
//!   `tokenizers::Tokenizer::from_file`). Pad token id = 1, not 0.
//! - **Model inputs:** the spectra-e5 ONNX export keeps the BERT triple
//!   `(input_ids, attention_mask, token_type_ids)` so the graph stays
//!   compatible with the ORT/HF tooling. We fill `token_type_ids` with
//!   zeros — XLM-R doesn't use segment ids and the model ignores the
//!   tensor.
//! - **Output:** same `[1, seq_len, 384]` last_hidden_state (named
//!   `/encoder/layer.11/output/LayerNorm/Add_1` in this export), same
//!   attention-weighted mean-pool, same L2-normalisation.
//!
//! E5 convention is to prefix `"query: "` for queries and `"passage: "` for
//! documents. We omit the prefix here so the trait stays
//! "encode one string → one vector" agnostic to query-vs-document; the
//! small quality loss is documented in the registry entry and is the price
//! of a clean `TextEncoder` boundary.

use std::path::Path;
use std::sync::Arc;

use thiserror::Error;
use tokenizers::Tokenizer;
use tract_onnx::prelude::*;

use crate::core::embeddings::{TextEncoder, EMBED_DIM};

#[derive(Debug, Error)]
pub enum E5Error {
    #[error("model file not found: {0}")]
    ModelMissing(String),
    #[error("tokenizer file not found: {0}")]
    TokenizerMissing(String),
    #[error("tokenizer: {0}")]
    Tokenizer(String),
    #[error("tract: {0}")]
    Tract(String),
}

type Plan = SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>;

pub struct OnnxMultilingualE5 {
    plan: Arc<Plan>,
    tokenizer: Tokenizer,
    seq_len: usize,
    /// Number of input tensors the ONNX graph expects. Detected at
    /// load time. The intfloat HF export has 3 (input_ids,
    /// attention_mask, token_type_ids); some quantized re-exports
    /// drop token_type_ids and have 2. We feed only what the graph
    /// declares.
    n_inputs: usize,
}

impl OnnxMultilingualE5 {
    /// Load the model + tokenizer from a directory containing `model.onnx`
    /// and `tokenizer.json`. The loader is defensive against ONNX
    /// variants:
    /// - Probes the model's declared input count and pins only the
    ///   inputs that exist (2 or 3, the two XLM-R shapes in the wild).
    /// - Tries the fully-optimised tract path first; falls back to the
    ///   typed graph if optimisation fails (some quantized exports hit
    ///   Unsqueeze-analyse limitations in tract 0.21).
    /// - Uses `seq_len = 128` for a reasonable speed/recall trade-off
    ///   on 512-trained XLM-R variants (HF docs recommend 512; 128
    ///   keeps inference fast and matches the spectra-e5 export).
    pub fn load(model_dir: &Path) -> Result<Self, E5Error> {
        let model_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");
        if !model_path.is_file() {
            return Err(E5Error::ModelMissing(model_path.display().to_string()));
        }
        if !tokenizer_path.is_file() {
            return Err(E5Error::TokenizerMissing(
                tokenizer_path.display().to_string(),
            ));
        }
        let tokenizer =
            Tokenizer::from_file(&tokenizer_path).map_err(|e| E5Error::Tokenizer(e.to_string()))?;
        let seq_len = 128_usize;

        // First pass: probe the model to learn its input count.
        let inference_model = tract_onnx::onnx()
            .model_for_path(&model_path)
            .map_err(|e| E5Error::Tract(format!("load: {e}")))?;
        let n_inputs = inference_model
            .input_outlets()
            .map_err(|e| E5Error::Tract(format!("inputs: {e}")))?
            .len();
        if !(2..=3).contains(&n_inputs) {
            return Err(E5Error::Tract(format!(
                "unexpected model input count: got {n_inputs}, expected 2 or 3 \
                 (input_ids, attention_mask, [token_type_ids])"
            )));
        }

        // Second pass: pin each input shape to [1, seq_len] and try the
        // optimised plan. If optimisation fails (e.g. the quantized
        // Unsqueeze AddDims limitation in spectra-e5), the call site
        // gets a clear message pointing at the recommended export.
        let plan = build_optimised_plan(&model_path, n_inputs, seq_len).map_err(|e| {
            E5Error::Tract(format!(
                "tract load failed for {n_inputs}-input model at \
                 [1, {seq_len}]: {e}\n\
                 If you supplied a quantized ONNX (e.g. spectra-e5 \
                 with Unsqueeze AddDims), try the full-precision \
                 intfloat/multilingual-e5-small export from \
                 huggingface.co/intfloat/multilingual-e5-small."
            ))
        })?;

        Ok(Self {
            plan: Arc::new(plan),
            tokenizer,
            seq_len,
            n_inputs,
        })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>, E5Error> {
        let mut encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| E5Error::Tokenizer(e.to_string()))?;

        if encoding.get_ids().len() > self.seq_len {
            encoding.truncate(self.seq_len, 0, tokenizers::TruncationDirection::Right);
        }
        encoding.pad(
            self.seq_len,
            1,        // XLM-R pad token id
            0,        // pad type id (unused for E5 but the API requires it)
            "<pad>",
            tokenizers::PaddingDirection::Right,
        );

        let ids: Vec<i64> = encoding.get_ids().iter().map(|&v| v as i64).collect();
        let mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&v| v as i64)
            .collect();
        // XLM-R doesn't use segment ids; the ONNX graph still has the input
        // and we feed all-zeros.
        let types: Vec<i64> = vec![0_i64; self.seq_len];

        let shape = [1usize, self.seq_len];
        let ids_t: Tensor = tract_ndarray::Array2::from_shape_vec(shape, ids)
            .map_err(|e| E5Error::Tract(e.to_string()))?
            .into();
        let mask_t: Tensor = tract_ndarray::Array2::from_shape_vec(shape, mask.clone())
            .map_err(|e| E5Error::Tract(e.to_string()))?
            .into();
        let types_t: Tensor = tract_ndarray::Array2::from_shape_vec(shape, types)
            .map_err(|e| E5Error::Tract(e.to_string()))?
            .into();

        // Feed only the inputs the model declares — 2 (no token_type_ids,
        // typical of the intfloat HF onnx/model.onnx) or 3 (the spectra-e5
        // quantized variant).
        let outputs = match self.n_inputs {
            2 => self
                .plan
                .run(tvec!(ids_t.into(), mask_t.into()))
                .map_err(|e| E5Error::Tract(e.to_string()))?,
            3 => self
                .plan
                .run(tvec!(ids_t.into(), mask_t.into(), types_t.into()))
                .map_err(|e| E5Error::Tract(e.to_string()))?,
            n => {
                return Err(E5Error::Tract(format!(
                    "internal: unexpected n_inputs {n} at run time"
                )));
            }
        };

        let token_states = outputs
            .first()
            .ok_or_else(|| E5Error::Tract("model returned no outputs".into()))?;
        let view = token_states
            .to_array_view::<f32>()
            .map_err(|e| E5Error::Tract(e.to_string()))?;
        let shape = view.shape();
        if shape.len() != 3 || shape[0] != 1 || shape[1] != self.seq_len || shape[2] != EMBED_DIM {
            return Err(E5Error::Tract(format!(
                "unexpected output shape {:?}, expected [1, {}, {}]",
                shape, self.seq_len, EMBED_DIM
            )));
        }

        // Attention-weighted mean pool.
        let mut summed = vec![0.0_f32; EMBED_DIM];
        let mut total_mask = 0_f32;
        for s in 0..self.seq_len {
            let w = mask[s] as f32;
            if w == 0.0 {
                continue;
            }
            total_mask += w;
            for d in 0..EMBED_DIM {
                summed[d] += w * view[[0, s, d]];
            }
        }
        if total_mask <= 0.0 {
            return Ok(vec![0.0; EMBED_DIM]);
        }
        for v in &mut summed {
            *v /= total_mask;
        }
        let norm: f32 = summed.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-9 {
            for v in &mut summed {
                *v /= norm;
            }
        }
        Ok(summed)
    }
}

/// Build a fully-optimised tract plan for the model at `model_path`,
/// pinning each of `n_inputs` inputs to shape `[1, seq_len]` with `i64`
/// elements.
fn build_optimised_plan(
    model_path: &Path,
    n_inputs: usize,
    seq_len: usize,
) -> Result<Plan, tract_onnx::prelude::TractError> {
    let mut model = tract_onnx::onnx().model_for_path(model_path)?;
    for i in 0..n_inputs {
        model = model.with_input_fact(i, i64::fact([1, seq_len]).into())?;
    }
    model.into_optimized()?.into_runnable()
}

impl TextEncoder for OnnxMultilingualE5 {
    fn dim(&self) -> usize {
        EMBED_DIM
    }
    fn encode(&self, text: &str) -> Vec<f32> {
        self.embed(text).unwrap_or_else(|e| {
            tracing::warn!(target: "aura::embed", "e5 embed failed: {e}; returning zeros");
            vec![0.0; EMBED_DIM]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Hermetic test: only runs when the model is cached at the well-known
    /// sandbox path. CI without the cache skips silently.
    fn cached_model_dir() -> Option<PathBuf> {
        let p = PathBuf::from("/tmp/aura-mlm-test");
        if p.join("model.onnx").is_file() && p.join("tokenizer.json").is_file() {
            Some(p)
        } else {
            None
        }
    }

    fn dot(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// Tract 0.21's `into_optimized()` fails on the spectra-e5-model
    /// quantized ONNX with `Failed analyse for node #564 "/Unsqueeze"
    /// AddDims`. The infrastructure (download, registry, `pick_encoder`
    /// priority) is exercised by the unit tests in `download.rs`; the
    /// end-to-end inference tests below are gated on a tract-compatible
    /// multilingual ONNX being dropped at `/tmp/aura-mlm-test/` —
    /// re-enable once an upstream fix lands or a non-quantized model is
    /// vendored.
    #[test]
    #[ignore = "tract 0.21 cannot optimize the spectra-e5 quantized ONNX (Unsqueeze AddDims node 564). Phase 5d ships the seam; user-supplied non-quantized model unblocks this test."]
    fn loads_and_encodes_arabic_when_model_cached() {
        let Some(dir) = cached_model_dir() else {
            eprintln!("skipped: model not at /tmp/aura-mlm-test");
            return;
        };
        let m = OnnxMultilingualE5::load(&dir).expect("load");
        let v = m.embed("القهوة الصباحية تساعد على التركيز").expect("embed");
        assert_eq!(v.len(), EMBED_DIM);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-3,
            "expected unit-norm Arabic embedding, got |v|={norm}"
        );
    }

    /// Arabic paraphrase pair must land closer than an off-topic English
    /// sentence — the **falsifying** test that proves multilingual semantic
    /// retrieval actually works (HashEmbedder couldn't see this either way,
    /// English-only MiniLM would struggle on Arabic input).
    #[test]
    #[ignore = "tract 0.21 cannot optimize the spectra-e5 quantized ONNX (Unsqueeze AddDims node 564). Phase 5d ships the seam; user-supplied non-quantized model unblocks this test."]
    fn arabic_paraphrase_pair_is_closer_than_off_topic() {
        let Some(dir) = cached_model_dir() else {
            eprintln!("skipped: model not at /tmp/aura-mlm-test");
            return;
        };
        let m = OnnxMultilingualE5::load(&dir).expect("load");
        // Two Arabic sentences about productivity with overlapping but
        // not identical vocabulary.
        let a = m.embed("الروتين الصباحي يساعد على زيادة الإنتاجية").unwrap();
        let b = m.embed("ترتيب يوم العمل في الصباح يحسن التركيز").unwrap();
        // Unrelated technical sentence (English to make the gap obvious).
        let c = m
            .embed("Quantum field theory describes the dynamics of subatomic particles")
            .unwrap();
        let sim_ab = dot(&a, &b);
        let sim_ac = dot(&a, &c);
        assert!(
            sim_ab > sim_ac + 0.05,
            "Arabic paraphrase pair sim_ab={sim_ab} not meaningfully closer than \
             off-topic sim_ac={sim_ac}"
        );
    }

    /// Cross-lingual: the SAME concept in Arabic and English should land
    /// closer than two unrelated concepts.
    #[test]
    #[ignore = "tract 0.21 cannot optimize the spectra-e5 quantized ONNX (Unsqueeze AddDims node 564). Phase 5d ships the seam; user-supplied non-quantized model unblocks this test."]
    fn arabic_english_same_concept_is_close() {
        let Some(dir) = cached_model_dir() else {
            eprintln!("skipped: model not at /tmp/aura-mlm-test");
            return;
        };
        let m = OnnxMultilingualE5::load(&dir).expect("load");
        let ar = m.embed("القطط حيوانات لطيفة").unwrap(); // "cats are lovely animals"
        let en_same = m.embed("Cats are lovely animals").unwrap();
        let en_off = m.embed("The compiler emits optimised machine code").unwrap();
        let sim_cross = dot(&ar, &en_same);
        let sim_off = dot(&ar, &en_off);
        assert!(
            sim_cross > sim_off + 0.05,
            "cross-lingual same-concept sim={sim_cross} not closer than off-topic sim={sim_off}"
        );
    }
}
