//! Phase batch step 5: AI usage telemetry.
//!
//! Frontend reads:
//! - `get_cache_stats(window_hours)` — hit ratio for the status-bar
//!   badge (target ≥ 70%).
//! - `get_today_cost_cents()` — daily budget meter.
//! - `get_recent_audit_log(limit)` — last N rows for the debugging
//!   panel.

use serde::Serialize;
use tauri::State;

use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    /// 0.0..=1.0; `None` when no calls in the window so the UI shows
    /// "—" instead of "0%".
    pub hit_ratio: Option<f32>,
    pub window_hours: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostSnapshot {
    pub today_cost_cents: i64,
    pub default_cap_cents: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditLogRow {
    pub id: i64,
    pub timestamp: i64,
    pub actor: String,
    pub operation: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    pub cost_micro_cents: i64,
    pub duration_ms: i64,
    pub status: String,
}

#[tauri::command]
pub async fn get_cache_stats(
    state: State<'_, AppState>,
    window_hours: Option<i64>,
) -> CmdResult<CacheStats> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let hours = window_hours.unwrap_or(24).clamp(1, 24 * 30);
    let hit_ratio = vault
        .db
        .audit_cache_hit_ratio(hours)
        .await
        .map_err(AuraError::from)?;
    Ok(CacheStats {
        hit_ratio,
        window_hours: hours,
    })
}

#[tauri::command]
pub async fn get_today_cost_cents(state: State<'_, AppState>) -> CmdResult<CostSnapshot> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let today = vault
        .db
        .audit_today_cost_cents()
        .await
        .map_err(AuraError::from)?;
    Ok(CostSnapshot {
        today_cost_cents: today,
        default_cap_cents: crate::ai::providers::anthropic::DEFAULT_DAILY_CAP_CENTS,
    })
}

#[tauri::command]
pub async fn get_recent_audit_log(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> CmdResult<Vec<AuditLogRow>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let lim = limit.unwrap_or(50).clamp(1, 500);
    let rows = vault
        .db
        .audit_recent(lim)
        .await
        .map_err(AuraError::from)?;
    Ok(rows
        .into_iter()
        .map(|r| AuditLogRow {
            id: r.id,
            timestamp: r.timestamp,
            actor: r.actor,
            operation: r.operation,
            model: r.model,
            input_tokens: r.input_tokens,
            cache_creation_input_tokens: r.cache_creation_input_tokens,
            cache_read_input_tokens: r.cache_read_input_tokens,
            output_tokens: r.output_tokens,
            cost_micro_cents: r.cost_micro_cents,
            duration_ms: r.duration_ms,
            status: r.status,
        })
        .collect())
}
