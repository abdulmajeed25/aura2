use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::core::file_watcher::spawn_watcher;
use crate::core::vault::{ReindexReport, VaultState};
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct VaultInfo {
    pub root: String,
    pub file_count: i64,
}

#[tauri::command]
pub async fn open_vault(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<VaultInfo> {
    let root = PathBuf::from(&path);
    let vault = VaultState::open(root)
        .await
        .map_err(AuraError::from)?;

    let report = vault.reindex().await.map_err(AuraError::from)?;
    let file_count = vault.db.count_files().await.map_err(AuraError::from)?;
    let root_str = vault.root.display().to_string();

    let vault_arc = Arc::new(vault);
    spawn_watcher(app.clone(), vault_arc.clone()).map_err(AuraError::from)?;

    // We store an owned VaultState in the app state so that command handlers
    // can borrow it cheaply. The watcher holds its own Arc.
    let detached = Arc::try_unwrap(vault_arc).unwrap_or_else(|arc| VaultState {
        root: arc.root.clone(),
        db: arc.db.clone(),
    });

    let mut guard = state.vault.lock().await;
    *guard = Some(detached);
    drop(guard);

    tracing::info!(
        target: "aura::vault",
        "opened vault {} (indexed {}, skipped {})",
        root_str,
        report.indexed,
        report.skipped
    );

    Ok(VaultInfo {
        root: root_str,
        file_count,
    })
}

#[tauri::command]
pub async fn close_vault(state: State<'_, AppState>) -> CmdResult<()> {
    let mut guard = state.vault.lock().await;
    *guard = None;
    Ok(())
}

#[tauri::command]
pub async fn current_vault(state: State<'_, AppState>) -> CmdResult<Option<VaultInfo>> {
    let guard = state.vault.lock().await;
    if let Some(v) = guard.as_ref() {
        let count = v.db.count_files().await.map_err(AuraError::from)?;
        Ok(Some(VaultInfo {
            root: v.root.display().to_string(),
            file_count: count,
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn reindex_vault(state: State<'_, AppState>) -> CmdResult<ReindexReport> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let report = vault.reindex().await.map_err(AuraError::from)?;
    Ok(report)
}
