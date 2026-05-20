//! Link-suggestion engine. For every pair of notes that *aren't* already
//! linked, compute their HDC combined-HV similarity and propose new links
//! when the score exceeds a user-controllable threshold.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use rayon::prelude::*;
use serde::Serialize;

use crate::core::hdc::encoder::encode_text;
use crate::core::hdc::graph_encoder::encode_note_combined;
use crate::core::hdc::{Hypervector, HV_DIM};
use crate::db::sqlite::VaultDb;

#[derive(Debug, Clone, Serialize)]
pub struct LinkSuggestion {
    pub source_file_id: String,
    pub source_path: String,
    pub source_title: String,
    pub target_file_id: String,
    pub target_path: String,
    pub target_title: String,
    pub score: f32,
    pub shared_neighbours: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct SuggestParams {
    pub min_score: f32,
    pub limit_per_source: usize,
    pub total_limit: usize,
}

impl Default for SuggestParams {
    fn default() -> Self {
        Self {
            min_score: 0.18,
            limit_per_source: 5,
            total_limit: 100,
        }
    }
}

/// Compute suggestions across the whole vault. O(N²) HDC similarities,
/// fine for ≤ ~2k notes; for larger vaults the seam is the parallel
/// `flat_map_iter` over the precomputed combined HVs.
pub async fn compute_suggestions(
    db: &VaultDb,
    params: SuggestParams,
) -> Result<Vec<LinkSuggestion>> {
    let rows = db.all_note_text_hvs().await?;
    if rows.len() < 2 {
        return Ok(Vec::new());
    }
    let neighbours = db.neighbour_map().await?;

    let combined: Vec<(String, String, String, Hypervector)> = rows
        .into_par_iter()
        .map(|(file_id, path, title, _dim, packed)| {
            let text_hv = Hypervector::from_packed_bytes(&packed, HV_DIM);
            let neigh = neighbours.get(&file_id).cloned().unwrap_or_default();
            let combined = encode_note_combined(&text_hv, &neigh.0, &neigh.1);
            (file_id, path, title, combined)
        })
        .collect();

    let mut outgoing: HashMap<String, HashSet<String>> = HashMap::new();
    for (src, (out, _in)) in &neighbours {
        outgoing.insert(src.clone(), out.iter().cloned().collect());
    }

    let mut suggestions: Vec<LinkSuggestion> = (0..combined.len())
        .into_par_iter()
        .flat_map_iter(|i| {
            let (src_id, src_path, src_title, src_hv) = &combined[i];
            let already_linked = outgoing.get(src_id).cloned().unwrap_or_default();
            let (src_out, src_in) = neighbours.get(src_id).cloned().unwrap_or_default();
            let src_set: HashSet<&String> = src_out.iter().chain(src_in.iter()).collect();

            let mut per_source: Vec<LinkSuggestion> = Vec::new();
            for (j, (tgt_id, tgt_path, tgt_title, tgt_hv)) in combined.iter().enumerate() {
                if i == j || already_linked.contains(tgt_id) {
                    continue;
                }
                let score = src_hv.similarity(tgt_hv);
                if score < params.min_score {
                    continue;
                }
                let (tgt_out, tgt_in) = neighbours.get(tgt_id).cloned().unwrap_or_default();
                let shared = tgt_out
                    .iter()
                    .chain(tgt_in.iter())
                    .filter(|n| src_set.contains(*n))
                    .count() as u32;

                per_source.push(LinkSuggestion {
                    source_file_id: src_id.clone(),
                    source_path: src_path.clone(),
                    source_title: src_title.clone(),
                    target_file_id: tgt_id.clone(),
                    target_path: tgt_path.clone(),
                    target_title: tgt_title.clone(),
                    score,
                    shared_neighbours: shared,
                });
            }
            per_source.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            per_source.truncate(params.limit_per_source);
            per_source.into_iter()
        })
        .collect();

    suggestions.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    suggestions.truncate(params.total_limit);
    Ok(suggestions)
}

/// Convenience: encode a chunk of text for callers that want to score it
/// against a stored note's HV without the DB roundtrip.
pub fn text_to_hv(text: &str) -> Hypervector {
    encode_text(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_have_sensible_bounds() {
        let p = SuggestParams::default();
        assert!(p.min_score > 0.0 && p.min_score < 1.0);
        assert!(p.limit_per_source > 0);
        assert!(p.total_limit >= p.limit_per_source);
    }

    #[test]
    fn text_to_hv_passthrough() {
        let a = text_to_hv("productivity habits");
        let b = encode_text("productivity habits");
        assert_eq!(a, b);
    }
}
