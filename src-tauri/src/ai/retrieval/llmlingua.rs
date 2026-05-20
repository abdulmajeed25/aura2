//! HTTP client for the LLMLingua-2 sidecar
//! (`services/llmlingua-sidecar/`). Closes the Rust half of
//! stand-in #18.
//!
//! The sidecar is optional: if it isn't reachable, callers should
//! fall back to the [`NoopCompressor`] which just returns the input
//! unchanged. The Anthropic provider picks the impl at construction
//! time so wiring is one trait swap.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompressError {
    #[error("http: {0}")]
    Http(String),
    #[error("sidecar status {0}: {1}")]
    Sidecar(u16, String),
    #[error("decode: {0}")]
    Decode(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressResponse {
    pub compressed: String,
    pub original_chars: usize,
    pub compressed_chars: usize,
    pub ratio: f32,
}

/// Trait so the Anthropic provider doesn't depend on a specific
/// implementation. `Send + Sync` because the provider stores its
/// compressor in an `Arc`.
#[async_trait]
pub trait Compressor: Send + Sync {
    async fn compress(
        &self,
        text: &str,
        target_ratio: f32,
    ) -> Result<CompressResponse, CompressError>;
}

/// Pass-through. Used when the sidecar isn't running and we don't
/// want to silently drop calls.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopCompressor;

#[async_trait]
impl Compressor for NoopCompressor {
    async fn compress(
        &self,
        text: &str,
        _target_ratio: f32,
    ) -> Result<CompressResponse, CompressError> {
        Ok(CompressResponse {
            compressed: text.to_string(),
            original_chars: text.len(),
            compressed_chars: text.len(),
            ratio: 1.0,
        })
    }
}

/// HTTP client for the FastAPI sidecar. Defaults to
/// `http://127.0.0.1:8765`; override with the env var
/// `AURA_LLMLINGUA_URL` at construction time.
#[derive(Debug, Clone)]
pub struct SidecarCompressor {
    base_url: String,
    client: reqwest::Client,
}

impl SidecarCompressor {
    /// Build with the configured URL. The 30-s timeout is generous on
    /// purpose — first compression after sidecar startup pulls the
    /// model into RAM.
    pub fn new(base_url: impl Into<String>) -> Result<Self, CompressError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| CompressError::Http(e.to_string()))?;
        Ok(Self {
            base_url: base_url.into(),
            client,
        })
    }

    pub fn from_env_or_default() -> Result<Self, CompressError> {
        let url = std::env::var("AURA_LLMLINGUA_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8765".to_string());
        Self::new(url)
    }
}

#[derive(Serialize)]
struct CompressRequest<'a> {
    text: &'a str,
    target_ratio: f32,
}

#[async_trait]
impl Compressor for SidecarCompressor {
    async fn compress(
        &self,
        text: &str,
        target_ratio: f32,
    ) -> Result<CompressResponse, CompressError> {
        let url = format!("{}/compress", self.base_url.trim_end_matches('/'));
        let req = CompressRequest { text, target_ratio };
        let resp = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| CompressError::Http(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CompressError::Sidecar(status.as_u16(), body));
        }
        resp.json::<CompressResponse>()
            .await
            .map_err(|e| CompressError::Decode(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_returns_input_unchanged() {
        let n = NoopCompressor;
        let r = n.compress("hello world", 0.5).await.unwrap();
        assert_eq!(r.compressed, "hello world");
        assert_eq!(r.ratio, 1.0);
    }

    #[tokio::test]
    async fn sidecar_url_falls_back_to_default() {
        // Without env var set, the constructor should not panic and
        // should still produce a working client (it just won't reach
        // anything until the sidecar runs).
        std::env::remove_var("AURA_LLMLINGUA_URL");
        let s = SidecarCompressor::from_env_or_default().unwrap();
        assert_eq!(s.base_url, "http://127.0.0.1:8765");
    }
}
