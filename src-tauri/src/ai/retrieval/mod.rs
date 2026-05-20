//! Phase batch step 5: retrieval helpers.
//!
//! Today this is the seam for stand-in #18 (LLMLingua-2 prompt
//! compression). The full v5.0 plan adds contextual retrieval,
//! hybrid RRF, and a reranker here once the corresponding
//! provider-side pieces land.

pub mod llmlingua;

pub use llmlingua::{CompressError, Compressor, NoopCompressor, SidecarCompressor};
