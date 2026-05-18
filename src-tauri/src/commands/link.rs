use tauri::State;

use crate::core::markdown_parser::{parse_outline, HeadingEntry};
use crate::db::schemas::{BacklinkEntry, LinkCandidate, OutgoingLinkEntry};
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

/// Backlinks for the file at the given vault-relative path.
#[tauri::command]
pub async fn get_backlinks(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<Vec<BacklinkEntry>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let out = vault.db.get_backlinks(&path).await.map_err(AuraError::from)?;
    Ok(out)
}

#[tauri::command]
pub async fn get_outgoing_links(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<Vec<OutgoingLinkEntry>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let out = vault
        .db
        .get_outgoing_links(&path)
        .await
        .map_err(AuraError::from)?;
    Ok(out)
}

/// Compute the heading outline for the given file, in document order.
#[tauri::command]
pub async fn get_outline(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<Vec<HeadingEntry>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let abs = vault.resolve(&path)?;
    if !abs.exists() {
        return Err(AuraError::FileNotFound(path).into());
    }
    let content = std::fs::read_to_string(&abs)?;
    Ok(parse_outline(&content))
}

/// Suggestions used by the `[[` autocomplete.
#[tauri::command]
pub async fn list_link_candidates(
    state: State<'_, AppState>,
) -> CmdResult<Vec<LinkCandidate>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let out = vault
        .db
        .list_link_candidates()
        .await
        .map_err(AuraError::from)?;
    Ok(out)
}
