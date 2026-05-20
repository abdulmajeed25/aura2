//! v5.0 AI provider plumbing. Phase batch 1.
//!
//! Layout:
//! - [`secrets`] — non-logging key loader (`<vault>/.aura/secrets/<name>.key`
//!   with an env-var fallback). API keys live behind an `ApiKey` newtype
//!   whose `Debug` impl never prints the value.
//! - [`providers`] — `AIProvider` trait + `AnthropicProvider` impl with
//!   prompt caching, 429 backoff, and per-call audit logging.
//! - [`audit`] — `DbAuditLogger` writes one row per AI call into the
//!   `audit_log` table (migration `007_audit_log`). Holds no secrets.
//!
//! Hard rule: nothing in this module logs the key in any form. Hard rule:
//! the key never enters error messages, debug traces, telemetry, or
//! tauri::ipc payloads.

pub mod audit;
pub mod providers;
pub mod secrets;

pub use audit::{AuditEvent, DbAuditLogger};
pub use providers::anthropic::AnthropicProvider;
pub use providers::{
    AIProvider, AiError, CacheTtl, ChatRequest, ChatResponse, Message, Role, TextBlock,
    Usage,
};
pub use secrets::{load_anthropic_key, ApiKey, KeyError};
