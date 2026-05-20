use serde::Serialize;
use tauri::State;

use crate::core::markdown_parser::{extract_block_by_user_ref, extract_section_by_heading};
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbedKind {
    File,
    Heading,
    Block,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedResult {
    pub kind: EmbedKind,
    pub source_path: String,
    pub source_title: String,
    pub content: String,
}

/// Resolve a `![[target#heading-or-^anchor]]` embed.
///
/// - If neither `heading` nor `block_ref` is provided, the entire file body is
///   returned (after stripping any YAML frontmatter).
/// - If `block_ref` is provided, the block whose `^anchor` matches is returned.
/// - If `heading` is provided, the slice of the document starting at that
///   heading and continuing until the next heading at the same or higher level
///   is returned.
#[tauri::command]
pub async fn resolve_embed(
    state: State<'_, AppState>,
    target: String,
    heading: Option<String>,
    block_ref: Option<String>,
) -> CmdResult<Option<EmbedResult>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;

    let Some(target_file) = vault
        .db
        .resolve_link_target(&target)
        .await
        .map_err(AuraError::from)?
    else {
        return Ok(None);
    };

    let abs = vault.resolve(&target_file.path)?;
    if !abs.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&abs)?;

    if let Some(anchor) = block_ref.as_deref().filter(|s| !s.is_empty()) {
        let Some(body) = extract_block_by_user_ref(&content, anchor) else {
            return Ok(None);
        };
        return Ok(Some(EmbedResult {
            kind: EmbedKind::Block,
            source_path: target_file.path,
            source_title: target_file.title,
            content: body,
        }));
    }

    if let Some(h) = heading.as_deref().filter(|s| !s.is_empty()) {
        let Some(body) = extract_section_by_heading(&content, h) else {
            return Ok(None);
        };
        return Ok(Some(EmbedResult {
            kind: EmbedKind::Heading,
            source_path: target_file.path,
            source_title: target_file.title,
            content: body,
        }));
    }

    let (_, body, _) =
        crate::core::markdown_parser::split_frontmatter(&content);
    Ok(Some(EmbedResult {
        kind: EmbedKind::File,
        source_path: target_file.path,
        source_title: target_file.title,
        content: body.trim().to_string(),
    }))
}
