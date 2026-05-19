//! Tauri commands for skills + workflow loading. Phase 16(a).

use tauri::State;

use crate::orchestration::skills::{load_skills, LoadReport};
use crate::orchestration::workflows::{load_workflows, WorkflowLoadReport};
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

/// List all Anthropic-Skills under `<vault>/.aura/skills/`. Returns
/// both the loaded skills and the ones that were skipped (with reason)
/// so the UI can show a "broken skill" hint.
#[tauri::command]
pub async fn list_skills(state: State<'_, AppState>) -> CmdResult<LoadReport> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let dir = vault.root.join(".aura").join("skills");
    Ok(load_skills(&dir))
}

/// List all workflows under `<vault>/.aura/workflows/`.
#[tauri::command]
pub async fn list_workflows(
    state: State<'_, AppState>,
) -> CmdResult<WorkflowLoadReport> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let dir = vault.root.join(".aura").join("workflows");
    Ok(load_workflows(&dir))
}
