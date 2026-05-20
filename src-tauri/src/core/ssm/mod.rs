//! Streaming state-space module (Phase 8).
//!
//! Real production deployment will swap [`StreamingState`] for an ONNX-backed
//! Mamba-130M; this stand-in keeps the same shape and the same user-visible
//! properties (constant memory, streaming, recurrent), but with a simple
//! transition rule we can run with no external model file.
//!
//! State update: `h' = normalise(α · h + (1-α) · x)` where `x` is the new
//! input embedding. The exponential-moving-average over a fixed-size hidden
//! vector means a million-step conversation occupies the same RAM as a
//! single-step one — the key Mamba property.
//!
//! STANDIN: Mamba-130M ONNX OR Phi-3-mini-4k-instruct ONNX.
//!
//! Phi-3 status (autonomous-batch step 1, 2026-05-20):
//! - Model on disk at `<vault>/.aura/models/phi-3-mini-cpu-int4/` (2.6 GB).
//! - `ort` 2.x bindings now build cleanly behind the `ort` Cargo feature;
//!   see [`phi3_runtime`].
//! - Loading the cpu-int4 ONNX with vanilla `ort` is **expected to fail**
//!   on `com.microsoft.MatMulNBits` / `GroupQueryAttention` /
//!   `RotaryEmbedding` — those are `onnxruntime-genai` custom ops.
//!   [`phi3_runtime::Phi3LoadError::UnsupportedCustomOp`] is the
//!   structured signal; on user boxes it cleanly drives the EMA
//!   fallback below. Closing #5 → 🟢 needs one of: an
//!   `onnxruntime-genai` Python sidecar, `candle-transformers` +
//!   Phi-3 safetensors (different model file from the cpu-int4
//!   ONNX we vendored), or Rust-implemented custom ops registered
//!   via `ort::operator`.

#[cfg(feature = "ort")]
pub mod phi3_runtime;

use serde::Serialize;

use crate::core::embeddings::cosine_similarity;

/// Default smoothing factor. Higher values keep more of the old state;
/// lower values track the latest input more aggressively.
pub const DEFAULT_ALPHA: f32 = 0.82;

#[derive(Debug, Clone, Serialize)]
pub struct StreamingState {
    pub dim: usize,
    pub alpha: f32,
    pub hidden: Vec<f32>,
    pub step_count: u32,
    /// Cosine of the latest input against the post-step state, in `[-1, 1]`.
    /// Useful as a "state heat" indicator in the UI.
    pub last_input_alignment: f32,
}

impl StreamingState {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            alpha: DEFAULT_ALPHA,
            hidden: vec![0.0; dim],
            step_count: 0,
            last_input_alignment: 0.0,
        }
    }

    /// Reset the hidden state to zeros. Counter and alignment are cleared too.
    pub fn reset(&mut self) {
        for v in &mut self.hidden {
            *v = 0.0;
        }
        self.step_count = 0;
        self.last_input_alignment = 0.0;
    }

    /// Recurrent step. Returns the cosine alignment between `input` and the
    /// new state (a proxy for "how surprising the input was").
    pub fn step(&mut self, input: &[f32]) -> f32 {
        debug_assert_eq!(input.len(), self.dim);
        let alpha = self.alpha;
        for (h, &x) in self.hidden.iter_mut().zip(input.iter()) {
            *h = alpha * *h + (1.0 - alpha) * x;
        }
        normalise_in_place(&mut self.hidden);
        self.step_count = self.step_count.saturating_add(1);
        let alignment = cosine_similarity(input, &self.hidden);
        self.last_input_alignment = alignment;
        alignment
    }

    /// Produce a "fused query" embedding that mixes the current state with
    /// the freshly-encoded `input`. Used to widen retrieval so the
    /// conversation's accumulated context biases ranking, not just the
    /// latest message.
    pub fn compose_query(&self, input: &[f32], blend: f32) -> Vec<f32> {
        debug_assert_eq!(input.len(), self.dim);
        let blend = blend.clamp(0.0, 1.0);
        let mut fused: Vec<f32> = self
            .hidden
            .iter()
            .zip(input.iter())
            .map(|(h, x)| blend * h + (1.0 - blend) * x)
            .collect();
        normalise_in_place(&mut fused);
        fused
    }

    /// Saturation in `[0, 1]`: 0 = freshly-reset, 1 = state vector has L2
    /// norm 1 (fully accumulated). The hidden vector is always
    /// post-normalised so this number tracks `step_count` smoothly.
    pub fn saturation(&self) -> f32 {
        if self.step_count == 0 {
            return 0.0;
        }
        let n = self.hidden.iter().map(|v| v * v).sum::<f32>().sqrt();
        n.clamp(0.0, 1.0)
    }
}

fn normalise_in_place(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-9 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::core::embeddings::{HashEmbedder, TextEncoder};

    #[test]
    fn first_step_aligns_state_to_input() {
        let enc = HashEmbedder::new();
        let mut s = StreamingState::new(enc.dim());
        let x = enc.encode("apple banana cherry");
        s.step(&x);
        // After a single step the state should be heavily aligned with x.
        let sim = cosine_similarity(&s.hidden, &x);
        assert!(sim > 0.9, "expected near-unit alignment, got {}", sim);
    }

    #[test]
    fn streaming_carries_information_across_steps() {
        let enc = HashEmbedder::new();
        let mut s = StreamingState::new(enc.dim());
        let a = enc.encode("productivity habits morning focus");
        let b = enc.encode("morning routine focused work flow");
        let c = enc.encode("rust async runtime tokio futures");

        s.step(&a);
        s.step(&b);

        // After two on-topic steps the state should be more similar to a
        // fresh on-topic input than to an off-topic one.
        let d_related = enc.encode("morning focus productivity");
        let d_other = enc.encode("compiler async future task");
        let sim_related = cosine_similarity(&s.hidden, &d_related);
        let sim_other = cosine_similarity(&s.hidden, &d_other);
        assert!(
            sim_related > sim_other,
            "state should be biased toward accumulated topic: related={} other={}",
            sim_related,
            sim_other
        );

        // Step in the off-topic direction. With α=0.82 a single step won't
        // overtake the entrenched topic, but it MUST move alignment with c
        // upward versus before the step.
        let sim_c_before = cosine_similarity(&s.hidden, &c);
        s.step(&c);
        let sim_c_after = cosine_similarity(&s.hidden, &c);
        assert!(
            sim_c_after > sim_c_before,
            "stepping with c must increase the state's alignment with c"
        );

        // After enough off-topic steps the state should eventually flip and
        // out-align c against the original topic — that's the recency
        // property of an EMA SSM.
        for _ in 0..40 {
            s.step(&c);
        }
        let sim_c_final = cosine_similarity(&s.hidden, &c);
        let sim_a_final = cosine_similarity(&s.hidden, &a);
        assert!(
            sim_c_final > sim_a_final,
            "after many off-topic steps, recent input dominates"
        );
    }

    #[test]
    fn reset_clears_all_state() {
        let enc = HashEmbedder::new();
        let mut s = StreamingState::new(enc.dim());
        s.step(&enc.encode("anything"));
        assert!(s.step_count > 0);
        s.reset();
        assert_eq!(s.step_count, 0);
        assert_eq!(s.last_input_alignment, 0.0);
        assert!(s.hidden.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn memory_is_fixed_size_regardless_of_step_count() {
        let enc = HashEmbedder::new();
        let mut s = StreamingState::new(enc.dim());
        let x = enc.encode("just one token here");
        for _ in 0..10_000 {
            s.step(&x);
        }
        assert_eq!(s.hidden.len(), enc.dim());
        assert!(s.saturation() <= 1.0 + 1e-6);
    }

    #[test]
    fn compose_query_interpolates_between_state_and_input() {
        let enc = HashEmbedder::new();
        let mut s = StreamingState::new(enc.dim());
        s.step(&enc.encode("alpha bravo charlie"));
        let x = enc.encode("xray yankee zulu");

        let pure_input = s.compose_query(&x, 0.0);
        let pure_state = s.compose_query(&x, 1.0);

        let sim_pure_to_input = cosine_similarity(&pure_input, &x);
        let sim_pure_to_state = cosine_similarity(&pure_state, &s.hidden);

        assert!(sim_pure_to_input > 0.99, "blend=0 should equal input");
        assert!(sim_pure_to_state > 0.99, "blend=1 should equal state");
    }
}
