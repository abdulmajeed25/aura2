//! AI-call audit log writer.
//!
//! Every call to an `AIProvider` produces one row in the `audit_log`
//! table. The row carries token counts, cost (in micro-cents to keep
//! everything in `i64`), duration, and a free-form `metadata_json`
//! field. The API key is **never** part of any row.
//!
//! This module is the only place that converts raw token counts into a
//! cost — the price table lives here as a `const` so it's clear what
//! the calculation is doing and easy to audit when Anthropic adjusts
//! pricing.

use std::sync::Arc;

use serde::Serialize;

use crate::ai::providers::Usage;
use crate::db::sqlite::VaultDb;

/// One row written to `audit_log` per AI call.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub timestamp: i64,
    /// Provider name, e.g. `"anthropic"`.
    pub actor: String,
    /// Logical operation, e.g. `"chat"`, `"summarize"`, `"embed"`.
    pub operation: String,
    pub model: Option<String>,
    pub usage: Usage,
    pub duration_ms: i64,
    /// `"ok"`, `"rate_limited"`, `"error"`.
    pub status: String,
    /// Free-form per-call metadata as JSON — used by the prompt
    /// self-modifier and by the budget UI. **Never** contains the API
    /// key or the prompt content verbatim. Prompt-content storage lives
    /// elsewhere (in the prompt-versioning module of the self-modifier).
    pub metadata_json: serde_json::Value,
}

/// Writes [`AuditEvent`]s into the `audit_log` table. `Arc<DbAuditLogger>`
/// can be cloned and shared across provider instances.
pub struct DbAuditLogger {
    db: Arc<VaultDb>,
}

impl DbAuditLogger {
    pub fn new(db: Arc<VaultDb>) -> Self {
        Self { db }
    }

    pub async fn log(&self, ev: AuditEvent) -> Result<(), anyhow::Error> {
        let metadata = serde_json::to_string(&ev.metadata_json).unwrap_or_default();
        let cost_micro_cents = (ev.usage.cost_usd_cents * 10_000.0).round() as i64;
        self.db
            .audit_insert(
                ev.timestamp,
                &ev.actor,
                &ev.operation,
                ev.model.as_deref(),
                ev.usage.input_tokens as i64,
                ev.usage.cache_creation_input_tokens as i64,
                ev.usage.cache_read_input_tokens as i64,
                ev.usage.output_tokens as i64,
                cost_micro_cents,
                ev.duration_ms,
                &ev.status,
                &metadata,
            )
            .await
    }

    /// Today's cumulative AI cost in **whole cents**. Used by the budget guard.
    pub async fn today_cost_cents(&self) -> Result<i64, anyhow::Error> {
        self.db.audit_today_cost_cents().await
    }

    /// Cache-hit ratio over the last `window_hours` of anthropic chat calls.
    pub async fn cache_hit_ratio(&self, window_hours: i64) -> Result<Option<f32>, anyhow::Error> {
        self.db.audit_cache_hit_ratio(window_hours).await
    }
}
