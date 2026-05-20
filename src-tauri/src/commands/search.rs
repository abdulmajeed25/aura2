use tauri::State;

use crate::core::search::{search_blocks, SearchHit, SearchMode};
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

/// Run a search across all blocks in the vault.
///
/// `mode` is one of `"semantic"`, `"fts"`, or `"hybrid"` (default).
/// `limit` is clamped to [1, 200].
#[tauri::command]
pub async fn search_vault(
    state: State<'_, AppState>,
    query: String,
    mode: Option<String>,
    limit: Option<u32>,
) -> CmdResult<Vec<SearchHit>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;

    let mode = match mode.as_deref() {
        Some("semantic") => SearchMode::Semantic,
        Some("fts") => SearchMode::Fts,
        _ => SearchMode::Hybrid,
    };
    let limit = limit.unwrap_or(20) as usize;

    let hits = search_blocks(&vault.db, vault.encoder.as_ref(), &query, mode, limit)
        .await
        .map_err(AuraError::from)?;
    Ok(hits)
}
