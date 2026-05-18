use chrono::Utc;
use ignore::WalkBuilder;
use serde::Serialize;
use tauri::State;

use crate::core::embeddings::{embedding_to_bytes, EMBED_DIM};
use crate::core::multimedia::tools::ToolsStatus;
use crate::core::multimedia::{describe, detect_kind, encode_media};
use crate::db::schemas::MediaRow;
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

/// Status of the optional external binaries Aura uses for URL ingestion.
#[tauri::command]
pub async fn media_tools_status() -> CmdResult<ToolsStatus> {
    Ok(ToolsStatus::probe())
}

/// Ingest a single media file by its vault-relative path. The file must
/// already exist inside the vault (Local-First Absolute).
#[tauri::command]
pub async fn ingest_media(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<MediaRow> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let abs = vault.resolve(&path)?;
    if !abs.exists() {
        return Err(AuraError::FileNotFound(path).into());
    }
    let kind = match detect_kind(&abs) {
        Some(k) => k,
        None => return Err(AuraError::InvalidPath(format!("not a media file: {}", path)).into()),
    };

    let metadata = std::fs::metadata(&abs)?;
    let description = describe(&path, kind, metadata.len());
    let bytes = std::fs::read(&abs)?;
    let emb = encode_media(&description, &bytes);
    let emb_bytes = embedding_to_bytes(&emb);

    let now = Utc::now().timestamp_millis();
    let existing = vault.db.get_media_by_path(&path).await.map_err(AuraError::from)?;
    let id = existing
        .as_ref()
        .map(|r| r.id.clone())
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    let row = MediaRow {
        id,
        path: path.clone(),
        kind: kind.as_str().to_string(),
        size_bytes: metadata.len() as i64,
        duration_ms: None,
        description,
        indexed_at: now,
    };
    vault
        .db
        .upsert_media(&row, &emb_bytes, EMBED_DIM as i64)
        .await
        .map_err(AuraError::from)?;
    Ok(row)
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub ingested: u32,
    pub skipped: u32,
}

/// Walk the vault and ingest every media file we find.
#[tauri::command]
pub async fn scan_media(state: State<'_, AppState>) -> CmdResult<ScanReport> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let root = vault.root.clone();
    drop(guard);

    let mut ingested = 0u32;
    let mut skipped = 0u32;
    let walker = WalkBuilder::new(&root)
        .hidden(false)
        .git_ignore(true)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(name == ".aura" || name == ".git" || name == "node_modules")
        })
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() || detect_kind(path).is_none() {
            continue;
        }
        let guard = state.vault.lock().await;
        let vault = match guard.as_ref() {
            Some(v) => v,
            None => return Err(AuraError::NoVault.into()),
        };
        let rel = match vault.relativize(path) {
            Ok(r) => r,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        drop(guard);

        match ingest_media(state.clone(), rel).await {
            Ok(_) => ingested += 1,
            Err(_) => skipped += 1,
        }
    }
    Ok(ScanReport { ingested, skipped })
}

#[tauri::command]
pub async fn list_media(state: State<'_, AppState>) -> CmdResult<Vec<MediaRow>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let out = vault.db.list_media().await.map_err(AuraError::from)?;
    Ok(out)
}

#[tauri::command]
pub async fn delete_media(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    vault
        .db
        .delete_media_by_path(&path)
        .await
        .map_err(AuraError::from)?;
    Ok(())
}
