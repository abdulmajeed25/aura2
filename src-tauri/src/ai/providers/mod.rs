//! Provider-agnostic chat interface.
//!
//! The trait is intentionally small: [`AIProvider::chat`] does one
//! request/response. Streaming is a future extension once we wire a
//! frontend that can consume it; for the initial GraphRAG + workflow
//! gates the non-streamed path is enough.
//!
//! Honest disclosure: this trait carries Anthropic's prompt-caching
//! shape (4 cache breakpoints, two TTL options). Other providers
//! (OpenAI, Ollama) that don't have native cache_control will ignore
//! the cache hints — the cost-per-token just doesn't drop. The trait
//! contract is "your cache hints will be honoured if the provider
//! supports them, otherwise they're free metadata."

pub mod anthropic;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One request to a chat-style provider.
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    /// Anthropic-style structured system prompt. Multiple blocks let us
    /// place a `cache_control` breakpoint between the stable preamble
    /// (1h cache) and any per-call additions (5min cache).
    pub system: Vec<TextBlock>,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    pub stop_sequences: Vec<String>,
    /// Free-form metadata stored in `audit_log.metadata_json`. Useful
    /// for the prompt self-modifier to correlate scores with prompts.
    pub metadata: serde_json::Value,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, user_text: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            system: Vec::new(),
            messages: vec![Message::user(user_text)],
            max_tokens: 1024,
            temperature: None,
            stop_sequences: Vec::new(),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_system(mut self, text: impl Into<String>, cache: Option<CacheTtl>) -> Self {
        self.system.push(TextBlock {
            text: text.into(),
            cache,
        });
        self
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }

    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = Some(t);
        self
    }

    pub fn with_metadata(mut self, m: serde_json::Value) -> Self {
        self.metadata = m;
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TextBlock {
    pub text: String,
    /// `None` = no caching hint. `Some(CacheTtl::FiveMinutes)` = short
    /// per-session cache. `Some(CacheTtl::OneHour)` = stable preamble.
    pub cache: Option<CacheTtl>,
}

impl TextBlock {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            cache: None,
        }
    }
    pub fn cached(text: impl Into<String>, ttl: CacheTtl) -> Self {
        Self {
            text: text.into(),
            cache: Some(ttl),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheTtl {
    /// Anthropic's default ephemeral cache: 5-minute TTL, refreshed on
    /// every read. Use for the per-session context (recent
    /// conversation, current vault state).
    FiveMinutes,
    /// 1-hour TTL. Use for stable preambles — the global system prompt,
    /// the tool catalogue, a community's full member content.
    OneHour,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<TextBlock>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![TextBlock::plain(text)],
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![TextBlock::plain(text)],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
    pub output_tokens: u32,
    /// Total cost in **USD cents** (not micro-cents — micro-cents is
    /// the storage representation in `audit_log`). f64 is OK here
    /// because cost-per-call rarely exceeds a few cents.
    pub cost_usd_cents: f64,
}

#[derive(Debug, Error)]
pub enum AiError {
    #[error("provider returned http {status}: {body}")]
    Http { status: u16, body: String },
    #[error("provider rate-limited after {attempts} attempts")]
    RateLimited { attempts: u32 },
    #[error("budget reached: today's spend ${spent_cents} ≥ cap ${cap_cents}")]
    BudgetExceeded { spent_cents: i64, cap_cents: i64 },
    #[error("network: {0}")]
    Network(String),
    #[error("response shape: {0}")]
    BadResponse(String),
    #[error("audit log: {0}")]
    Audit(String),
}

#[async_trait]
pub trait AIProvider: Send + Sync {
    /// Stable name used in `audit_log.actor`. Lowercase, ASCII.
    fn name(&self) -> &str;

    /// One round-trip. Implementations write one row to `audit_log` per
    /// call (regardless of outcome — even rate-limited or errored
    /// requests show up so the user can see what happened).
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AiError>;
}
