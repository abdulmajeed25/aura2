//! Fact store backed by the libsql `facts` + `fact_history` tables
//! (migration 009). All times are milliseconds since the unix epoch.
//!
//! The store is intentionally not generic over the embedder: callers
//! pass the f32 vector in directly. That keeps this module testable in
//! isolation against a tiny vector even when the rest of the binary is
//! using a 384-d encoder.

use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use uuid::Uuid;

use crate::core::embeddings::{bytes_to_embedding, embedding_to_bytes};
use crate::db::sqlite::{FactRowDb, VaultDb};

/// One stored fact. `embedding` is the canonical f32 vector; the
/// `created_at` / `updated_at` fields are wall-clock millis.
#[derive(Debug, Clone, Serialize)]
pub struct Fact {
    pub id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub category: Option<String>,
    pub confidence: f32,
    pub source_session: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Audit entry. Surface in the UI as a "what changed and why" log.
#[derive(Debug, Clone, Serialize)]
pub struct FactHistoryEntry {
    pub fact_id: String,
    pub op: String,
    pub prev_text: Option<String>,
    pub new_text: Option<String>,
    pub reason: Option<String>,
    pub timestamp: i64,
}

#[derive(Clone)]
pub struct FactStore {
    db: Arc<VaultDb>,
}

impl FactStore {
    pub fn new(db: Arc<VaultDb>) -> Self {
        Self { db }
    }

    /// Insert a new fact. Caller supplies a fresh UUIDv7. We do not
    /// dedupe here — that's the decision policy's job.
    pub async fn add(
        &self,
        text: &str,
        embedding: &[f32],
        category: Option<&str>,
        confidence: f32,
        source_session: Option<&str>,
    ) -> Result<Fact> {
        let id = Uuid::now_v7().simple().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let bytes = embedding_to_bytes(embedding);
        self.db
            .fact_insert(
                &id,
                text,
                &bytes,
                category,
                confidence as f64,
                source_session,
                now,
            )
            .await?;
        self.db
            .fact_history_insert(&id, "ADD", None, Some(text), None, now)
            .await?;
        Ok(Fact {
            id,
            text: text.to_string(),
            embedding: embedding.to_vec(),
            category: category.map(|s| s.to_string()),
            confidence,
            source_session: source_session.map(|s| s.to_string()),
            created_at: now,
            updated_at: now,
        })
    }

    /// Replace an existing fact's text + embedding. `prev_text` is
    /// preserved on the history row so the UI can render a diff.
    pub async fn update(
        &self,
        id: &str,
        new_text: &str,
        new_embedding: &[f32],
        reason: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        let prev = self.db.fact_text(id).await?;
        let bytes = embedding_to_bytes(new_embedding);
        self.db.fact_update(id, new_text, &bytes, now).await?;
        self.db
            .fact_history_insert(id, "UPDATE", prev.as_deref(), Some(new_text), reason, now)
            .await?;
        Ok(())
    }

    /// Soft-delete: sets `deleted_at` on the fact and logs the reason.
    /// We never hard-delete because the history row is meaningless
    /// without a way to look up the old text on demand.
    pub async fn soft_delete(&self, id: &str, reason: Option<&str>) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        let prev = self.db.fact_text(id).await?;
        self.db.fact_soft_delete(id, now).await?;
        self.db
            .fact_history_insert(id, "DELETE", prev.as_deref(), None, reason, now)
            .await?;
        Ok(())
    }

    /// Record a NOOP for the audit trail. Useful so the user can see
    /// "the engine considered this fact and decided it was already
    /// known" without us silently dropping the candidate.
    pub async fn record_noop(
        &self,
        matched_id: &str,
        candidate_text: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        self.db
            .fact_history_insert(
                matched_id,
                "NOOP",
                None,
                Some(candidate_text),
                reason,
                now,
            )
            .await?;
        Ok(())
    }

    /// Active (non-deleted) facts. Newest first.
    pub async fn all_active(&self) -> Result<Vec<Fact>> {
        let rows: Vec<FactRowDb> = self.db.fact_list_active().await?;
        Ok(rows
            .into_iter()
            .map(|r| Fact {
                id: r.id,
                text: r.text,
                embedding: bytes_to_embedding(&r.embedding),
                category: r.category,
                confidence: r.confidence as f32,
                source_session: r.source_session,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect())
    }

    pub async fn history(&self, fact_id: &str) -> Result<Vec<FactHistoryEntry>> {
        self.db.fact_history(fact_id).await
    }
}
