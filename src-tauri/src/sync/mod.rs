//! Phase 17 polish (autonomous-batch step 6): encrypted-sync primitives.
//!
//! [`age_envelope`] wraps the `age` crate so the rest of the codebase
//! never imports `age::*` directly — the trait surface here is the seam
//! a future cloud-sync provider plugs into. Nothing in this module
//! talks to a remote yet; that lands when the sync transport is chosen.

pub mod age_envelope;

pub use age_envelope::{decrypt, encrypt, EnvelopeError};
