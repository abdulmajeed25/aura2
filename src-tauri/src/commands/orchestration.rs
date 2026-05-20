//! Tauri commands for skills + workflow loading + execution.
//! Phase 16(a) loaded + listed; Phase 16(b) adds `run_workflow`.

use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;

use crate::ai::audit::DbAuditLogger;
use crate::ai::providers::anthropic::AnthropicProvider;
use crate::ai::providers::AIProvider;
use crate::ai::secrets::load_anthropic_key;
use crate::orchestration::executor::{execute_workflow, ExecutionResult};
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

/// Phase 16(b): execute a workflow by name.
///
/// `workflow_name` is matched against the loaded workflows' `name`
/// field. `params` are templated into each step's args via `{{name}}`
/// substitution. `dry_run = true` (the default) runs every step EXCEPT
/// file writes, which are collected as `PendingWrite` records so the
/// UI can render a "Review changes" panel before the user clicks Apply.
#[tauri::command]
pub async fn run_workflow(
    state: State<'_, AppState>,
    workflow_name: String,
    params: Option<HashMap<String, String>>,
    dry_run: Option<bool>,
) -> CmdResult<ExecutionResult> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let root = vault.root.clone();

    let workflows = load_workflows(&root.join(".aura").join("workflows"));
    let workflow = workflows
        .loaded
        .into_iter()
        .find(|w| w.name == workflow_name)
        .ok_or_else(|| AuraError::Other(format!("workflow {workflow_name:?} not found")))?;
    let skills = load_skills(&root.join(".aura").join("skills"));

    let provider: Option<Arc<dyn AIProvider>> = match load_anthropic_key(&root) {
        Ok(key) => {
            let audit = Arc::new(DbAuditLogger::new(vault.db.clone()));
            Some(Arc::new(AnthropicProvider::new(key, audit)))
        }
        Err(_) => None,
    };

    Ok(execute_workflow(
        &workflow,
        &skills.loaded,
        provider,
        vault,
        params.unwrap_or_default(),
        dry_run.unwrap_or(true),
    )
    .await)
}
