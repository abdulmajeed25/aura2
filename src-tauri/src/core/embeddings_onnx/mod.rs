//! ONNX-backed `all-MiniLM-L6-v2` embedder.
//!
//! v5.0 Phase 5. Replaces (or sits alongside) the v3 `HashEmbedder`. Uses:
//! - `tract-onnx` — pure-Rust ONNX inference. No native dependencies, builds
//!   on every Tauri target without extra setup.
//! - `tokenizers` — HuggingFace tokenizer in pure Rust, loaded from the
//!   model's `tokenizer.json`.
//!
//! Model files (`model.onnx` ~90 MB, `tokenizer.json` ~712 KB) are downloaded
//! on first use, **never** committed to git. The download source is the
//! `hunterreid/pool-party-embed-weights` GitHub mirror of the upstream
//! HuggingFace `sentence-transformers/all-MiniLM-L6-v2` (Apache-2.0).
//! Vetted SHA-256 checksums in [`MANIFEST`] make the download tamper-evident.
//!
//! Honest disclosure: this is a real semantic encoder, not a lexical hash.
//! Per the spec's Hard Rule #10, when this path is active the UI must say
//! "Semantic search via all-MiniLM-L6-v2 ONNX" — not "real embeddings" if
//! `HashEmbedder` is still the fallback.

pub mod download;
pub mod embedder;
pub mod embedder_e5;
pub mod tokenizer;

pub use download::{
    download_model, find_manifest, ModelArch, ModelManifest, E5_MULTILINGUAL_MANIFEST,
    MANIFEST, MINILM_MANIFEST, MODELS,
};
pub use embedder::OnnxMiniLm;
pub use embedder_e5::OnnxMultilingualE5;
pub use tokenizer::MiniLmTokenizer;
