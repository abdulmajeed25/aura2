//! Cognitive-loop telemetry (autonomous-batch step 4).
//!
//! Lands a per-step record into `audit_log` with `actor = 'cortex'` so
//! the spec's "Hamiltonian / FHRR 30-day measurement window" has data
//! to chew on at the day-30 review. The choice to reuse `audit_log`
//! over a dedicated `cognitive_telemetry` table is deliberate — the
//! day-30 query joins against Anthropic-call rows from the same table
//! to compare cortex activity against LLM spend.
//!
//! Wire-in: callers do `telemetry::record_hamiltonian_step(...)` /
//! `telemetry::record_fhrr_compare(...)`. Both are best-effort —
//! failures log a `tracing::warn!` but never propagate into the
//! perpetual loop's tick path.

use std::sync::Arc;

use serde_json::json;

use crate::db::sqlite::VaultDb;

/// One leapfrog step's worth of data. Captured *after* the step so
/// `energy_after - energy_before` is the symplectic drift contribution.
pub struct HamiltonianStepRecord {
    pub energy_before: f32,
    pub energy_after: f32,
    pub state_dim: usize,
    pub dt: f32,
    /// L2 norm of the gradient at the start of the step. Big values
    /// → strong pattern pull; tiny values → free-particle regime.
    pub grad_norm: f32,
    /// L2 norm of the momentum after the step. Bounded growth here is
    /// the canonical symplectic-stability signal.
    pub momentum_norm_after: f32,
}

/// One FHRR vs. bipolar HV similarity measurement on the same input
/// pair. Lets the day-30 review answer: "does FHRR retrieve more / less
/// / same as bipolar on the corpus?"
pub struct FhrrCompareRecord {
    pub bipolar_similarity: f32,
    pub fhrr_similarity: f32,
    pub corpus_size: usize,
}

/// Write one Hamiltonian-step row. Best-effort; logs on failure.
pub async fn record_hamiltonian_step(
    db: &Arc<VaultDb>,
    r: HamiltonianStepRecord,
) {
    let meta = json!({
        "energy_before": r.energy_before,
        "energy_after": r.energy_after,
        "energy_delta": r.energy_after - r.energy_before,
        "state_dim": r.state_dim,
        "dt": r.dt,
        "grad_norm": r.grad_norm,
        "momentum_norm_after": r.momentum_norm_after,
    });
    let ts = chrono::Utc::now().timestamp_millis();
    if let Err(e) = db
        .audit_insert(
            ts,
            "cortex",
            "hamiltonian_step",
            None,
            0,
            0,
            0,
            0,
            0,
            0,
            "ok",
            &meta.to_string(),
        )
        .await
    {
        tracing::warn!(
            target: "aura::cognition::telemetry",
            "hamiltonian_step audit insert failed: {e}"
        );
    }
}

/// Write one FHRR-vs-bipolar comparison row.
pub async fn record_fhrr_compare(db: &Arc<VaultDb>, r: FhrrCompareRecord) {
    let meta = json!({
        "bipolar_similarity": r.bipolar_similarity,
        "fhrr_similarity": r.fhrr_similarity,
        "delta": r.fhrr_similarity - r.bipolar_similarity,
        "corpus_size": r.corpus_size,
    });
    let ts = chrono::Utc::now().timestamp_millis();
    if let Err(e) = db
        .audit_insert(
            ts,
            "cortex",
            "fhrr_compare",
            None,
            0,
            0,
            0,
            0,
            0,
            0,
            "ok",
            &meta.to_string(),
        )
        .await
    {
        tracing::warn!(
            target: "aura::cognition::telemetry",
            "fhrr_compare audit insert failed: {e}"
        );
    }
}
