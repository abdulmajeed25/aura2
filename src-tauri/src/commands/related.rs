use rayon::prelude::*;
use serde::Serialize;
use tauri::State;

use crate::core::hdc::graph_encoder::encode_note_combined;
use crate::core::hdc::{Hypervector, HV_DIM};
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct RelatedNote {
    pub file_id: String,
    pub path: String,
    pub title: String,
    pub score: f32,
    pub shared_neighbours: u32,
}

/// Find notes structurally + textually similar to `path` using HDC.
///
/// Computes:
/// - The query note's combined HV (text bundle + permuted neighbour identities)
/// - Each candidate note's combined HV the same way
/// - Cosine similarity, descending, top-K
///
/// This is the HDC counterpart to Phase 5 semantic search: it picks up
/// notes that share *neighbourhood* even when their text doesn't overlap.
#[tauri::command]
pub async fn find_related(
    state: State<'_, AppState>,
    path: String,
    limit: Option<u32>,
) -> CmdResult<Vec<RelatedNote>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;

    let limit = limit.unwrap_or(10).clamp(1, 50) as usize;

    // Resolve query note to its file_id and find its row in the HV table.
    let Some(query_file) = vault
        .db
        .get_file_by_path(&path)
        .await
        .map_err(AuraError::from)?
    else {
        return Ok(Vec::new());
    };

    let all_hvs = vault
        .db
        .all_note_text_hvs()
        .await
        .map_err(AuraError::from)?;
    if all_hvs.is_empty() {
        return Ok(Vec::new());
    }
    let neighbours = vault.db.neighbour_map().await.map_err(AuraError::from)?;

    // Find query HV and pre-compute its combined HV.
    let Some(query_row) = all_hvs.iter().find(|r| r.0 == query_file.id) else {
        return Ok(Vec::new());
    };
    let query_text_hv = Hypervector::from_packed_bytes(&query_row.4, HV_DIM);
    let query_neighbours = neighbours.get(&query_file.id).cloned().unwrap_or_default();
    let query_combined = encode_note_combined(
        &query_text_hv,
        &query_neighbours.0,
        &query_neighbours.1,
    );

    let query_outgoing: std::collections::HashSet<String> =
        query_neighbours.0.iter().cloned().collect();
    let query_incoming: std::collections::HashSet<String> =
        query_neighbours.1.iter().cloned().collect();

    // Score each candidate in parallel.
    let mut scored: Vec<RelatedNote> = all_hvs
        .par_iter()
        .filter(|(file_id, _, _, _, _)| file_id != &query_file.id)
        .map(|(file_id, path, title, _dim, packed)| {
            let text_hv = Hypervector::from_packed_bytes(packed, HV_DIM);
            let cand_neighbours = neighbours.get(file_id).cloned().unwrap_or_default();
            let combined =
                encode_note_combined(&text_hv, &cand_neighbours.0, &cand_neighbours.1);
            let score = query_combined.similarity(&combined);

            // Count how many neighbours the two notes share. Used to colour
            // the UI and to break ties when scores are equal.
            let shared = cand_neighbours
                .0
                .iter()
                .chain(cand_neighbours.1.iter())
                .filter(|n| query_outgoing.contains(*n) || query_incoming.contains(*n))
                .count() as u32;

            RelatedNote {
                file_id: file_id.clone(),
                path: path.clone(),
                title: title.clone(),
                score,
                shared_neighbours: shared,
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);
    Ok(scored)
}
