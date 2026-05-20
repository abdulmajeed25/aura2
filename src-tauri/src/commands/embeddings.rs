//! Embeddings model management Tauri commands (Phase 5).
//!
//! Lets the frontend list available models, trigger the one-time download of
//! any of them into `<vault>/.aura/models/<model-name>/`, and check their
//! installed status. The active encoder picked at `VaultState::open` time
//! prefers `multilingual-e5-small` if installed, then `all-MiniLM-L6-v2`,
//! then `HashEmbedder` — see `core::vault::pick_encoder`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::core::embeddings_onnx::download::{
    download_model, find_manifest, is_present, vault_model_dir, ModelArch, ModelManifest,
    MODELS,
};
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    /// Key used in API calls (e.g. `"multilingual-e5-small"`).
    pub name: &'static str,
    /// Friendly UI label.
    pub display_name: &'static str,
    /// Output embedding dimension.
    pub embed_dim: usize,
    /// Languages declared by the manifest.
    pub languages: &'static [&'static str],
    /// Total expected payload size in bytes (sum of all files).
    pub bytes_total: u64,
    /// `true` iff every file in the manifest is on disk with the right size.
    pub installed: bool,
    /// Resolved model directory, if a vault is open. `None` otherwise.
    pub model_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingsStatus {
    /// One entry per known model in priority order.
    pub models: Vec<ModelStatus>,
    /// Name of the encoder that would activate on next `VaultState::open`.
    /// `"HashEmbedder"` when no real model is installed.
    pub active_when_reopened: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub model: String,
    pub file: String,
    pub bytes_so_far: u64,
    pub bytes_total: u64,
}

fn arch_priority_name(arch: ModelArch) -> &'static str {
    match arch {
        ModelArch::E5Multilingual => "multilingual-e5-small",
        ModelArch::MiniLm => "all-MiniLM-L6-v2",
    }
}

/// List every known embedding model and whether each is installed in the
/// currently-open vault. Also reports which encoder would activate on the
/// next `open_vault` call.
#[tauri::command]
pub async fn embeddings_model_status(
    state: State<'_, AppState>,
) -> CmdResult<EmbeddingsStatus> {
    let vault = state.vault.lock().await;
    let mut models = Vec::with_capacity(MODELS.len());
    let mut active = "HashEmbedder";

    for m in MODELS {
        let dir = vault.as_ref().map(|v| vault_model_dir(&v.root, m));
        let installed = dir.as_deref().map(|d| is_present(d, m)).unwrap_or(false);
        let bytes_total: u64 = m.files.iter().map(|f| f.bytes).sum();
        // First installed model in registry-priority order wins.
        if installed && active == "HashEmbedder" {
            active = arch_priority_name(m.arch);
        }
        models.push(ModelStatus {
            name: m.name,
            display_name: m.display_name,
            embed_dim: m.embed_dim,
            languages: m.languages,
            bytes_total,
            installed,
            model_dir: dir.map(|p| p.display().to_string()),
        });
    }

    Ok(EmbeddingsStatus {
        models,
        active_when_reopened: active,
    })
}

/// Download (or re-verify) a specific embeddings model bundle. Emits
/// `embeddings://download-progress` events with `DownloadProgress` payloads
/// while the download runs.
///
/// `model_name` defaults to `"multilingual-e5-small"` (the broader-coverage
/// option) when omitted.
#[tauri::command]
pub async fn download_embeddings_model(
    state: State<'_, AppState>,
    app: AppHandle,
    model_name: Option<String>,
) -> CmdResult<ModelStatus> {
    let name = model_name.unwrap_or_else(|| "multilingual-e5-small".to_string());
    let manifest: &'static ModelManifest = find_manifest(&name)
        .ok_or_else(|| AuraError::Other(format!("unknown model: {name}")))?;

    let model_dir = {
        let vault = state.vault.lock().await;
        let v = vault
            .as_ref()
            .ok_or_else(|| AuraError::Other("no vault open".into()))?;
        vault_model_dir(&v.root, manifest)
    };

    let app_clone = app.clone();
    let name_clone = manifest.name.to_string();
    let progress = move |file: &str, bytes_so_far: u64, bytes_total: u64| {
        let _ = app_clone.emit(
            "embeddings://download-progress",
            DownloadProgress {
                model: name_clone.clone(),
                file: file.to_string(),
                bytes_so_far,
                bytes_total,
            },
        );
    };

    download_model(&model_dir, manifest, progress)
        .await
        .map_err(|e| AuraError::Other(format!("download: {e}")))?;

    let bytes_total: u64 = manifest.files.iter().map(|f| f.bytes).sum();
    Ok(ModelStatus {
        name: manifest.name,
        display_name: manifest.display_name,
        embed_dim: manifest.embed_dim,
        languages: manifest.languages,
        bytes_total,
        installed: is_present(&model_dir, manifest),
        model_dir: Some(model_dir.display().to_string()),
    })
}
