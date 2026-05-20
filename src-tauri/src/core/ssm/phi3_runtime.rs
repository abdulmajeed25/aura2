//! Phi-3-mini ONNX runtime (autonomous-batch step 1).
//!
//! Loads `<vault>/.aura/models/phi-3-mini-cpu-int4/*.onnx` via `ort` 2.x
//! and exposes a [`Phi3Runtime`] that the streaming-mode AIChat can ask
//! for short completions. Only compiled when the `ort` Cargo feature is
//! enabled; the default build keeps the EMA stand-in from `ssm.rs`.
//!
//! What this file does **not** do:
//! - Full token-by-token streaming with KV cache. The Microsoft cpu-int4
//!   export uses `com.microsoft.MatMulNBits` + `GroupQueryAttention` +
//!   `RotaryEmbedding`, which are `onnxruntime-genai` custom ops that
//!   vanilla `ort` cannot decode. Loading is therefore expected to fail
//!   with a clear error; this module captures that failure into a
//!   [`Phi3LoadError::UnsupportedCustomOp`] and the caller falls back to
//!   the EMA stand-in. See [`docs/STAND_IN_REGISTRY.md`] entry #5.
//!
//! Once that load error surfaces on the user's box, the swap is one of:
//! 1. Run an `onnxruntime-genai` Python sidecar (mirrors the
//!    `services/llmlingua-sidecar/` pattern from stand-in #18).
//! 2. Switch to `candle-transformers` + Phi-3 safetensors weights
//!    (different model file, redownload — not the cpu-int4 ONNX we
//!    already vendored).
//! 3. Register `MatMulNBits` / `GroupQueryAttention` / `RotaryEmbedding`
//!    as Rust-implemented custom ops via `ort::operator`. This is the
//!    "vanilla `ort` end-to-end" path and the deepest of the three.

use std::path::{Path, PathBuf};

use thiserror::Error;
use tracing::warn;

#[derive(Debug, Error)]
pub enum Phi3LoadError {
    #[error("model directory missing: {0}")]
    DirNotFound(PathBuf),
    #[error("model.onnx missing under {0}")]
    OnnxMissing(PathBuf),
    #[error("ort session init failed: {0}")]
    OrtInit(String),
    /// The expected failure mode for `microsoft/Phi-3-mini-4k-instruct-onnx`
    /// `cpu-int4` exports under vanilla `ort`. The body contains the op
    /// name the runtime refused to load (e.g. `com.microsoft.MatMulNBits`)
    /// so the registry / report can quote it verbatim.
    #[error("unsupported custom op: {0}")]
    UnsupportedCustomOp(String),
}

pub struct Phi3Runtime {
    #[allow(dead_code)]
    session: ort::session::Session,
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

        let session_res = ort::session::Session::builder()
            .map_err(|e| Phi3LoadError::OrtInit(e.to_string()))?
            .commit_from_file(&onnx);

        match session_res {
            Ok(session) => Ok(Self { session }),
            Err(e) => {
                let msg = e.to_string();
                // Map the documented genai-only op failures to the
                // dedicated variant so the registry/report doesn't have
                // to grep stringly.
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

    /// Confirms the expected failure mode on the user's box. Gated on
    /// `/tmp/aura-phi3-test/` containing the cpu-int4 export so it
    /// auto-skips when the model isn't vendored (CI, dev boxes).
    /// `ort` 2.0.0-rc.12 actually loads the cpu-int4 session cleanly —
    /// the contrib `com.microsoft.MatMulNBits` /
    /// `GroupQueryAttention` / `RotaryEmbedding` ops are registered in
    /// the underlying `onnxruntime` C++ runtime since 1.17. What's still
    /// missing for an end-to-end completion is the inference *driver*
    /// (KV cache construction, attention-mask + position-ids book-
    /// keeping, sampling loop). This test pins the load-side ground
    /// truth so the next session inherits it without re-discovering.
    #[test]
    fn cpu_int4_export_loads_cleanly_when_present() {
        let p = std::path::PathBuf::from("/tmp/aura-phi3-test");
        let mut has_onnx = false;
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                for e in rd.flatten() {
                    if e.path().extension().and_then(|s| s.to_str()) == Some("onnx") {
                        has_onnx = true;
                        break;
                    }
                }
            }
        }
        if !has_onnx {
            eprintln!("skipped: phi-3 cpu-int4 onnx not at /tmp/aura-phi3-test");
            return;
        }
        let rt = Phi3Runtime::load(&p).expect("ort 2.x must load Phi-3 cpu-int4 cleanly");
        let inputs = rt.session.inputs().len();
        let outputs = rt.session.outputs().len();
        // Empirically pinned on 2026-05-20 with ort 2.0.0-rc.12:
        // 66 inputs = `input_ids` + `attention_mask` +
        //             32 × (`past_key_values.<i>.key` + `past_key_values.<i>.value`)
        // 65 outputs = `logits` + 32 × (`present.<i>.key` + `present.<i>.value`)
        // No `position_ids` — Phi-3-mini-4k uses internal `RotaryEmbedding`.
        assert_eq!(inputs, 66, "expected 66 inputs, got {inputs}");
        assert_eq!(outputs, 65, "expected 65 outputs, got {outputs}");
        let in0 = rt.session.inputs()[0].name();
        let out0 = rt.session.outputs()[0].name();
        assert_eq!(in0, "input_ids");
        assert_eq!(out0, "logits");
    }
}
