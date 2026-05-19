//! Embeddings model management Tauri commands (Phase 5).
//!
//! Lets the frontend trigger the one-time download of the
//! all-MiniLM-L6-v2 ONNX model + tokenizer into
//! `<vault>/.aura/models/all-MiniLM-L6-v2/`, check the status, and (in a
//! future gate) toggle the active encoder between `HashEmbedder` and
//! `OnnxMiniLm`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::core::embeddings_onnx::download::{
    download_model, is_present, vault_model_dir, MANIFEST,
};
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingsModelStatus {
    /// Name of the model bundle (e.g. `"all-MiniLM-L6-v2"`).
    pub name: &'static str,
    /// Output embedding dimension.
    pub embed_dim: usize,
    /// `true` iff every file in the manifest is on disk with the right size.
    pub installed: bool,
    /// Resolved model directory, if a vault is open. `None` otherwise.
    pub model_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub file: String,
    pub bytes_so_far: u64,
    pub bytes_total: u64,
}

/// Current installed-or-not status for the embeddings model.
#[tauri::command]
pub async fn embeddings_model_status(
    state: State<'_, AppState>,
) -> CmdResult<EmbeddingsModelStatus> {
    let vault = state.vault.lock().await;
    let model_dir = vault.as_ref().map(|v| vault_model_dir(&v.root));
    let installed = model_dir.as_deref().map(is_present).unwrap_or(false);
    Ok(EmbeddingsModelStatus {
        name: MANIFEST.name,
        embed_dim: MANIFEST.embed_dim,
        installed,
        model_dir: model_dir.map(|p| p.display().to_string()),
    })
}

/// Download (or re-verify) the embeddings model bundle. Emits
/// `embeddings://download-progress` events with [`DownloadProgress`]
/// payloads while the download runs.
#[tauri::command]
pub async fn download_embeddings_model(
    state: State<'_, AppState>,
    app: AppHandle,
) -> CmdResult<EmbeddingsModelStatus> {
    let model_dir = {
        let vault = state.vault.lock().await;
        let v = vault
            .as_ref()
            .ok_or_else(|| AuraError::Other("no vault open".into()))?;
        vault_model_dir(&v.root)
    };

    let app_clone = app.clone();
    let progress = move |file: &str, bytes_so_far: u64, bytes_total: u64| {
        let _ = app_clone.emit(
            "embeddings://download-progress",
            DownloadProgress {
                file: file.to_string(),
                bytes_so_far,
                bytes_total,
            },
        );
    };

    download_model(&model_dir, progress)
        .await
        .map_err(|e| AuraError::Other(format!("download: {e}")))?;

    Ok(EmbeddingsModelStatus {
        name: MANIFEST.name,
        embed_dim: MANIFEST.embed_dim,
        installed: is_present(&model_dir),
        model_dir: Some(model_dir.display().to_string()),
    })
}
