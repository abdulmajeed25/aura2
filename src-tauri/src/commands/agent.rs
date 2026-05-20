use serde::Serialize;
use tauri::State;

use crate::core::agent::optimization::{find_orphans, OrphanNote};
use crate::core::agent::suggestions::{compute_suggestions, LinkSuggestion, SuggestParams};
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[tauri::command]
pub async fn suggest_links(
    state: State<'_, AppState>,
    min_score: Option<f32>,
    limit_per_source: Option<u32>,
    total_limit: Option<u32>,
) -> CmdResult<Vec<LinkSuggestion>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let mut params = SuggestParams::default();
    if let Some(s) = min_score {
        params.min_score = s.clamp(0.0, 1.0);
    }
    if let Some(l) = limit_per_source {
        params.limit_per_source = (l as usize).clamp(1, 50);
    }
    if let Some(t) = total_limit {
        params.total_limit = (t as usize).clamp(1, 500);
    }
    let out = compute_suggestions(&vault.db, params)
        .await
        .map_err(AuraError::from)?;
    Ok(out)
}

#[tauri::command]
pub async fn find_orphan_notes(state: State<'_, AppState>) -> CmdResult<Vec<OrphanNote>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let out = find_orphans(&vault.db).await.map_err(AuraError::from)?;
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyReport {
    pub source_path: String,
    pub target_path: String,
    pub appended: String,
}

/// Append a wiki link to `source_path` referencing `target_path`. The link
/// is added under a "## Related" heading at the end of the file (creating
/// it if missing) so the user can review or remove it as a normal text
/// edit. The file is reindexed immediately so backlinks reflect the change.
#[tauri::command]
pub async fn apply_link_suggestion(
    state: State<'_, AppState>,
    source_path: String,
    target_path: String,
    alias: Option<String>,
) -> CmdResult<ApplyReport> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let src_abs = vault.resolve(&source_path)?;
    if !src_abs.exists() {
        return Err(AuraError::FileNotFound(source_path).into());
    }

    let target_stem = target_path.trim_end_matches(".markdown").trim_end_matches(".md");
    let link_inner = match &alias {
        Some(a) if !a.is_empty() => format!("[[{}|{}]]", target_stem, a),
        _ => format!("[[{}]]", target_stem),
    };
    let appended = format!("- {}", link_inner);

    let mut content = std::fs::read_to_string(&src_abs)?;
    let related_marker = "\n## Related\n";
    if let Some(idx) = content.find(related_marker) {
        // Insert immediately after the header.
        let insert_at = idx + related_marker.len();
        content.insert_str(insert_at, &format!("{}\n", appended));
    } else {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("\n## Related\n\n{}\n", appended));
    }
    std::fs::write(&src_abs, &content)?;
    vault
        .index_one(&src_abs)
        .await
        .map_err(AuraError::from)?;

    Ok(ApplyReport {
        source_path,
        target_path,
        appended,
    })
}
