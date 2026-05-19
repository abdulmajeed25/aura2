//! Phase 13/14: neuro-symbolic reasoning. v5.0 NEW.
//!
//! Two pieces shipped so far:
//! - [`vsa_inference`] — algebraic queries over a bipolar-HDC knowledge
//!   base. "What value is bound to role R?" "What role binds X to Y?"
//!   "Is (K, V) plausibly in memory M?". Built on the existing
//!   [`crate::core::hdc`] kernel.
//! - [`z3_bridge`] — Python sidecar wrapping `z3-solver`. Rust spawns the
//!   sidecar, pipes SMT-LIB constraint text in, reads a `{result, model}`
//!   JSON back. The sidecar lives at `sidecars/z3_sidecar.py` so a user
//!   can update Python deps without a Rust rebuild.
//!
//! ILP (inductive logic programming) and the DSPy-style prompt
//! self-modifier from the spec are still pending — they need either an
//! LLM or a more involved rule-induction engine.

pub mod vsa_inference;
pub mod z3_bridge;
