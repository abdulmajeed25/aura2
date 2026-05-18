use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebouncedEvent};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::core::vault::VaultState;

/// Frontend-facing payload emitted on the `vault://changed` event channel.
#[derive(Debug, Clone, Serialize)]
pub struct FileChangeEvent {
    pub kind: String, // "created" | "modified" | "removed" | "renamed"
    pub path: String, // vault-relative path
}

/// Spawn a debounced filesystem watcher on the vault root and forward events
/// into the database + the frontend.
pub fn spawn_watcher(app: AppHandle, vault: Arc<VaultState>) -> Result<()> {
    let root = vault.root.clone();
    let vault_for_thread = vault.clone();

    std::thread::Builder::new()
        .name("aura-file-watcher".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("failed to start watcher runtime: {}", e);
                    return;
                }
            };

            let (tx, rx) = std::sync::mpsc::channel();
            let mut debouncer = match new_debouncer(Duration::from_millis(300), None, tx) {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("failed to create debouncer: {}", e);
                    return;
                }
            };

            if let Err(e) = debouncer
                .watcher()
                .watch(&root, RecursiveMode::Recursive)
            {
                tracing::error!("failed to watch {}: {}", root.display(), e);
                return;
            }

            tracing::info!(target: "aura::watch", "watching {}", root.display());

            for result in rx {
                let events = match result {
                    Ok(events) => events,
                    Err(errs) => {
                        for e in errs {
                            tracing::warn!("watch error: {:?}", e);
                        }
                        continue;
                    }
                };
                rt.block_on(handle_events(&app, &vault_for_thread, events));
            }
        })?;
    Ok(())
}

async fn handle_events(app: &AppHandle, vault: &VaultState, events: Vec<DebouncedEvent>) {
    for ev in events {
        for path in &ev.event.paths {
            if !is_markdown(path) {
                continue;
            }
            if path
                .components()
                .any(|c| matches!(c.as_os_str().to_str(), Some(".aura") | Some(".git")))
            {
                continue;
            }

            let kind = classify(&ev.event.kind);
            let rel = match vault.relativize(path) {
                Ok(r) => r,
                Err(_) => continue,
            };

            match kind {
                "created" | "modified" | "renamed" => {
                    if let Err(e) = vault.index_one(path).await {
                        tracing::warn!("reindex failed for {}: {}", path.display(), e);
                    }
                }
                "removed" => {
                    if let Err(e) = vault.db.delete_file_by_path(&rel).await {
                        tracing::warn!("delete row failed for {}: {}", rel, e);
                    }
                }
                _ => {}
            }

            let _ = app.emit(
                "vault://changed",
                FileChangeEvent {
                    kind: kind.to_string(),
                    path: rel,
                },
            );
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("md") | Some("markdown")
    )
}

fn classify(kind: &notify::EventKind) -> &'static str {
    use notify::EventKind::*;
    match kind {
        Create(_) => "created",
        Modify(notify::event::ModifyKind::Name(_)) => "renamed",
        Modify(_) => "modified",
        Remove(_) => "removed",
        _ => "other",
    }
}
