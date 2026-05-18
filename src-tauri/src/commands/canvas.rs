use ignore::WalkBuilder;
use serde::Serialize;
use tauri::State;

use crate::core::canvas::CanvasDoc;
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct CanvasFile {
    pub path: String,
    pub name: String,
    pub modified_at: i64,
}

/// Enumerate every `.canvas` file in the vault.
#[tauri::command]
pub async fn list_canvases(state: State<'_, AppState>) -> CmdResult<Vec<CanvasFile>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let root = vault.root.clone();

    let mut out = Vec::new();
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
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("canvas") {
            continue;
        }
        let rel = match vault.relativize(path) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let modified_at = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        out.push(CanvasFile {
            path: rel,
            name,
            modified_at,
        });
    }
    out.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(out)
}

#[tauri::command]
pub async fn read_canvas(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<CanvasDoc> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let abs = vault.resolve(&path)?;
    if !abs.exists() {
        return Ok(CanvasDoc::default());
    }
    let content = std::fs::read_to_string(&abs)?;
    let doc = CanvasDoc::parse(&content)
        .map_err(|e| AuraError::Other(format!("invalid canvas JSON: {}", e)))?;
    Ok(doc)
}

#[tauri::command]
pub async fn write_canvas(
    state: State<'_, AppState>,
    path: String,
    doc: CanvasDoc,
) -> CmdResult<()> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    doc.validate().map_err(AuraError::Other)?;
    let abs = vault.resolve(&path)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = doc
        .serialize_pretty()
        .map_err(|e| AuraError::Other(e.to_string()))?;
    std::fs::write(&abs, serialized)?;
    Ok(())
}

#[tauri::command]
pub async fn create_canvas(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<()> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let abs = vault.resolve(&path)?;
    if abs.exists() {
        return Err(AuraError::InvalidPath(format!("already exists: {}", path)).into());
    }
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let empty = CanvasDoc::default()
        .serialize_pretty()
        .map_err(|e| AuraError::Other(e.to_string()))?;
    std::fs::write(&abs, empty)?;
    Ok(())
}
