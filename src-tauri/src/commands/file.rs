use std::path::PathBuf;

use chrono::Utc;
use ignore::WalkBuilder;
use serde::Serialize;
use tauri::State;

use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

/// List markdown files only, flat, sorted by `modified_at` desc.
#[tauri::command]
pub async fn list_files(state: State<'_, AppState>) -> CmdResult<Vec<FileEntry>> {
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
        if !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("md") | Some("markdown")
        ) {
            continue;
        }
        let metadata = match path.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let rel = match vault.relativize(path) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let modified_ms = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        out.push(FileEntry {
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            path: rel,
            is_dir: false,
            size: metadata.len(),
            modified_at: modified_ms,
        });
    }

    out.sort_by_key(|f| std::cmp::Reverse(f.modified_at));
    Ok(out)
}

/// Build a hierarchical folder tree of the vault (folders + .md files).
#[tauri::command]
pub async fn file_tree(state: State<'_, AppState>) -> CmdResult<TreeNode> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let root = vault.root.clone();

    fn build(node_path: &std::path::Path, vault_root: &std::path::Path) -> Option<TreeNode> {
        let name = node_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if matches!(name.as_str(), ".aura" | ".git" | "node_modules") {
            return None;
        }
        let metadata = std::fs::metadata(node_path).ok()?;
        let rel = node_path
            .strip_prefix(vault_root)
            .ok()
            .map(|p| {
                p.components()
                    .filter_map(|c| match c {
                        std::path::Component::Normal(s) => Some(s.to_string_lossy().to_string()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_default();

        if metadata.is_dir() {
            let mut children = Vec::new();
            if let Ok(entries) = std::fs::read_dir(node_path) {
                let mut sorted: Vec<_> = entries.flatten().collect();
                sorted.sort_by_key(|e| (!e.path().is_dir(), e.file_name()));
                for entry in sorted {
                    if let Some(child) = build(&entry.path(), vault_root) {
                        children.push(child);
                    }
                }
            }
            Some(TreeNode {
                path: rel,
                name,
                is_dir: true,
                children,
            })
        } else if matches!(
            node_path.extension().and_then(|s| s.to_str()),
            Some("md") | Some("markdown")
        ) {
            Some(TreeNode {
                path: rel,
                name,
                is_dir: false,
                children: vec![],
            })
        } else {
            None
        }
    }

    let tree = build(&root, &root).unwrap_or(TreeNode {
        path: String::new(),
        name: root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        is_dir: true,
        children: vec![],
    });
    Ok(tree)
}

#[tauri::command]
pub async fn read_file(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let abs = vault.resolve(&path)?;
    if !abs.exists() {
        return Err(AuraError::FileNotFound(path).into());
    }
    let content = std::fs::read_to_string(&abs)?;
    Ok(content)
}

#[tauri::command]
pub async fn write_file(
    state: State<'_, AppState>,
    path: String,
    content: String,
) -> CmdResult<()> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let abs = vault.resolve(&path)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&abs, content)?;
    // Reindex synchronously so list_files reflects new size/title immediately.
    if let Err(e) = vault.index_one(&abs).await {
        tracing::warn!("reindex after write failed for {}: {}", path, e);
    }
    Ok(())
}

#[tauri::command]
pub async fn create_file(
    state: State<'_, AppState>,
    path: String,
    initial_content: Option<String>,
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
    let body = initial_content.unwrap_or_else(|| {
        format!(
            "# {}\n\nCreated {}\n",
            abs.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".into()),
            Utc::now().to_rfc3339()
        )
    });
    std::fs::write(&abs, &body)?;
    if let Err(e) = vault.index_one(&abs).await {
        tracing::warn!("index after create failed for {}: {}", path, e);
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_file(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let abs = vault.resolve(&path)?;
    if abs.exists() {
        std::fs::remove_file(&abs)?;
    }
    vault
        .db
        .delete_file_by_path(&path)
        .await
        .map_err(AuraError::from)?;
    Ok(())
}

#[tauri::command]
pub async fn rename_file(
    state: State<'_, AppState>,
    from_path: String,
    to_path: String,
) -> CmdResult<()> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let from_abs: PathBuf = vault.resolve(&from_path)?;
    let to_abs: PathBuf = vault.resolve(&to_path)?;
    if !from_abs.exists() {
        return Err(AuraError::FileNotFound(from_path).into());
    }
    if to_abs.exists() {
        return Err(AuraError::InvalidPath(format!("already exists: {}", to_path)).into());
    }
    if let Some(parent) = to_abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&from_abs, &to_abs)?;
    vault
        .db
        .delete_file_by_path(&from_path)
        .await
        .map_err(AuraError::from)?;
    if let Err(e) = vault.index_one(&to_abs).await {
        tracing::warn!("reindex after rename failed for {}: {}", to_path, e);
    }
    Ok(())
}
