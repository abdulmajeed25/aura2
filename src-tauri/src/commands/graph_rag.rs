use std::collections::HashMap;

use serde::Serialize;
use tauri::State;

use crate::core::embeddings::{embedding_to_bytes, HashEmbedder, TextEncoder, EMBED_DIM};
use crate::core::graph_rag::community_detector::detect_communities;
use crate::core::graph_rag::query_engine::{run_query, GraphRagAnswer};
use crate::core::graph_rag::summarizer::{extractive_summary, leading_paragraphs};
use crate::db::sqlite::ReplaceCommunity;
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct RebuildReport {
    pub communities: u32,
    pub members_total: u32,
    pub avg_members: f32,
}

/// Detect communities, summarise each, and persist them. Idempotent —
/// running it again wipes the old partition and writes a fresh one.
#[tauri::command]
pub async fn rebuild_graph_rag(state: State<'_, AppState>) -> CmdResult<RebuildReport> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;

    // Pull nodes + resolved edges.
    let (nodes_raw, edges_raw) = vault
        .db
        .fetch_graph_nodes_and_edges()
        .await
        .map_err(AuraError::from)?;
    let node_ids: Vec<String> = nodes_raw.iter().map(|(id, _, _)| id.clone()).collect();
    let id_to_path: HashMap<String, (String, String)> = nodes_raw
        .iter()
        .map(|(id, p, t)| (id.clone(), (p.clone(), t.clone())))
        .collect();

    let partition = detect_communities(&node_ids, &edges_raw, 30, 0xA1A0_2026_u64);

    // Group node ids by community.
    let mut by_community: HashMap<u32, Vec<String>> = HashMap::new();
    for (node_id, cid) in &partition {
        by_community.entry(*cid).or_default().push(node_id.clone());
    }

    // For each community, build an extractive summary and its embedding.
    let encoder = HashEmbedder::new();
    let mut payloads: Vec<OwnedReplaceCommunity> = Vec::with_capacity(by_community.len());
    for members in by_community.values() {
        let mut entries: Vec<(String, String)> = Vec::with_capacity(members.len());
        for file_id in members {
            let (path, title) = match id_to_path.get(file_id) {
                Some(v) => v,
                None => continue,
            };
            // Read the file body to extract a leading paragraph.
            let abs = vault
                .resolve(path)
                .map_err(|e| AuraError::from(anyhow::anyhow!(e.to_string())))?;
            let content = std::fs::read_to_string(&abs).unwrap_or_default();
            let lead = leading_paragraphs(&content, 240);
            entries.push((title.clone(), lead));
        }
        let summary = extractive_summary(&entries);
        let emb = encoder.encode(&summary);
        let bytes = embedding_to_bytes(&emb);
        payloads.push(OwnedReplaceCommunity {
            level: 0,
            member_file_ids: members.clone(),
            summary_text: summary,
            embedding: bytes,
            dim: EMBED_DIM as i64,
        });
    }

    // Project to the borrowed shape the DB expects.
    let borrowed: Vec<ReplaceCommunity<'_>> = payloads
        .iter()
        .map(|p| ReplaceCommunity {
            level: p.level,
            member_file_ids: p.member_file_ids.iter().map(String::as_str).collect(),
            summary_text: &p.summary_text,
            embedding: &p.embedding,
            dim: p.dim,
        })
        .collect();
    vault
        .db
        .replace_communities(&borrowed)
        .await
        .map_err(AuraError::from)?;

    let communities = payloads.len() as u32;
    let members_total: u32 = payloads
        .iter()
        .map(|p| p.member_file_ids.len() as u32)
        .sum();
    let avg_members = if communities == 0 {
        0.0
    } else {
        members_total as f32 / communities as f32
    };

    Ok(RebuildReport {
        communities,
        members_total,
        avg_members,
    })
}

/// Run a GraphRAG query. If the community index is empty, returns an empty
/// answer so the UI can show a "run rebuild first" hint.
#[tauri::command]
pub async fn graph_rag_query(
    state: State<'_, AppState>,
    question: String,
    limit: Option<u32>,
) -> CmdResult<GraphRagAnswer> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let limit = limit.unwrap_or(3) as usize;
    let answer = run_query(&vault.db, &question, limit)
        .await
        .map_err(AuraError::from)?;
    Ok(answer)
}

struct OwnedReplaceCommunity {
    level: i64,
    member_file_ids: Vec<String>,
    summary_text: String,
    embedding: Vec<u8>,
    dim: i64,
}
