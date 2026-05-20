use anyhow::Result;
use rayon::prelude::*;
use serde::Serialize;

use crate::core::embeddings::{cosine_similarity, TextEncoder};
use crate::db::sqlite::VaultDb;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Semantic,
    Fts,
    Hybrid,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub block_id: String,
    pub file_id: String,
    pub file_path: String,
    pub file_title: String,
    pub block_type: String,
    pub line_number: i64,
    pub score: f32,
    pub snippet: String,
    pub matched_via: &'static str, // "semantic" | "fts" | "both"
}

const HYBRID_RRF_K: f32 = 60.0;
const SNIPPET_LEN: usize = 180;

/// Run a search across all blocks in the vault. `encoder` is the
/// active text encoder picked at `VaultState::open` time; pass
/// `vault.encoder.as_ref()` to use it.
pub async fn search_blocks(
    db: &VaultDb,
    encoder: &dyn TextEncoder,
    query: &str,
    mode: SearchMode,
    limit: usize,
) -> Result<Vec<SearchHit>> {
    let limit = limit.clamp(1, 200);
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    match mode {
        SearchMode::Semantic => semantic_only(db, encoder, q, limit).await,
        SearchMode::Fts => fts_only(db, q, limit).await,
        SearchMode::Hybrid => hybrid(db, encoder, q, limit).await,
    }
}

async fn semantic_only(
    db: &VaultDb,
    encoder: &dyn TextEncoder,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>> {
    let q_vec = encoder.encode(query);
    if q_vec.iter().all(|x| *x == 0.0) {
        return Ok(Vec::new());
    }

    let rows = db.all_block_embeddings_with_meta().await?;
    let mut scored_blocks: Vec<(f32, BlockMeta)> = rows
        .into_par_iter()
        .map(|(meta, emb)| {
            let sim = cosine_similarity(&q_vec, &emb);
            (sim, meta)
        })
        .filter(|(s, _)| *s > 0.01)
        .collect();

    // Phase 9: media files live in the same 384-dim space, so we score them
    // alongside text blocks and merge — one cosine ranking across modalities.
    let media = db.all_media_with_embeddings().await.unwrap_or_default();
    let media_hits: Vec<SearchHit> = media
        .into_par_iter()
        .filter_map(|(row, emb)| {
            let sim = cosine_similarity(&q_vec, &emb);
            if sim <= 0.01 {
                return None;
            }
            Some(SearchHit {
                block_id: row.id,
                file_id: row.path.clone(),
                file_path: row.path,
                file_title: row.description.clone(),
                block_type: format!("media:{}", row.kind),
                line_number: 0,
                score: sim,
                snippet: row.description,
                matched_via: "semantic",
            })
        })
        .collect();

    scored_blocks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<SearchHit> = scored_blocks
        .into_iter()
        .map(|(sim, m)| SearchHit {
            block_id: m.block_id,
            file_id: m.file_id,
            file_path: m.file_path,
            file_title: m.file_title,
            block_type: m.block_type,
            line_number: m.line_number,
            score: sim,
            snippet: snippet_around(&m.content, query),
            matched_via: "semantic",
        })
        .collect();
    out.extend(media_hits);
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit);
    Ok(out)
}

async fn fts_only(db: &VaultDb, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let rows = db.fts_search(query, limit as i64 * 2).await?;
    let mut out: Vec<SearchHit> = rows
        .into_iter()
        .map(|(m, score)| SearchHit {
            block_id: m.block_id,
            file_id: m.file_id,
            file_path: m.file_path,
            file_title: m.file_title,
            block_type: m.block_type,
            line_number: m.line_number,
            // FTS5 bm25 returns lower-is-better — flip to higher-is-better.
            score: -score,
            snippet: snippet_around(&m.content, query),
            matched_via: "fts",
        })
        .collect();
    out.truncate(limit);
    Ok(out)
}

async fn hybrid(
    db: &VaultDb,
    encoder: &dyn TextEncoder,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>> {
    let fts_hits = fts_only(db, query, limit * 3).await.unwrap_or_default();
    let sem_hits = semantic_only(db, encoder, query, limit * 3)
        .await
        .unwrap_or_default();

    let mut rank_map: std::collections::HashMap<String, (f32, Option<SearchHit>, &'static str)> =
        std::collections::HashMap::new();

    for (idx, hit) in fts_hits.iter().enumerate() {
        let contribution = 1.0 / (HYBRID_RRF_K + idx as f32 + 1.0);
        let entry = rank_map
            .entry(hit.block_id.clone())
            .or_insert((0.0, None, "fts"));
        entry.0 += contribution;
        if entry.1.is_none() {
            entry.1 = Some(hit.clone());
        }
    }
    for (idx, hit) in sem_hits.iter().enumerate() {
        let contribution = 1.0 / (HYBRID_RRF_K + idx as f32 + 1.0);
        let entry = rank_map
            .entry(hit.block_id.clone())
            .or_insert((0.0, None, "semantic"));
        entry.0 += contribution;
        if entry.1.is_none() {
            entry.1 = Some(hit.clone());
        } else {
            entry.2 = "both";
        }
    }

    let mut scored: Vec<SearchHit> = rank_map
        .into_iter()
        .filter_map(|(_, (score, hit, via))| {
            hit.map(|mut h| {
                h.score = score;
                h.matched_via = via;
                h
            })
        })
        .collect();
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}

fn snippet_around(content: &str, query: &str) -> String {
    let q_lower = query.to_lowercase();
    let lower = content.to_lowercase();
    let idx = lower.find(&q_lower);
    let len = content.len();

    let (start, end) = match idx {
        Some(i) => {
            let s = i.saturating_sub(40);
            let e = (i + q_lower.len() + 140).min(len);
            (s, e)
        }
        None => (0, len.min(SNIPPET_LEN)),
    };
    let mut s = start;
    while s > 0 && !content.is_char_boundary(s) {
        s -= 1;
    }
    let mut e = end;
    while e < len && !content.is_char_boundary(e) {
        e += 1;
    }
    let mut snip = content[s..e].replace('\n', " ");
    if s > 0 {
        snip.insert(0, '…');
    }
    if e < len {
        snip.push('…');
    }
    snip
}

/// Metadata payload returned from `VaultDb::all_block_embeddings_with_meta`.
pub struct BlockMeta {
    pub block_id: String,
    pub file_id: String,
    pub file_path: String,
    pub file_title: String,
    pub block_type: String,
    pub line_number: i64,
    pub content: String,
}
