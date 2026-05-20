use std::collections::HashMap;

use serde::Serialize;
use tauri::State;

use crate::ai::audit::DbAuditLogger;
use crate::ai::providers::AIProvider;
use crate::ai::providers::anthropic::AnthropicProvider;
use crate::ai::secrets::load_anthropic_key;
use crate::core::embeddings::{embedding_to_bytes, TextEncoder, EMBED_DIM};
use crate::core::graph_rag::leiden::{leiden, Partition};
use crate::core::graph_rag::llm_summarizer::{summarize_community, CommunityEntries};
use crate::core::graph_rag::query_engine::{run_query, GraphRagAnswer};
use crate::core::graph_rag::summarizer::{extractive_summary, leading_paragraphs};
use crate::db::sqlite::ReplaceCommunity;
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct RebuildReport {
    /// Total rows written across all hierarchy levels.
    pub communities: u32,
    pub members_total: u32,
    pub avg_members: f32,
    /// Number of Leiden hierarchy levels persisted.
    pub levels: u32,
    /// Per-level community counts, coarsest-first.
    pub per_level: Vec<u32>,
    /// How many community summaries came from Claude (Phase batch step 2).
    pub llm_summaries: u32,
    /// How many summaries fell back to the extractive path (no key, or
    /// the LLM call failed and we retried locally).
    pub extractive_summaries: u32,
}

/// Detect communities (real Leiden, all hierarchy levels), summarise each,
/// and persist them with parent linkage. Idempotent — running it again
/// wipes the old partition and writes a fresh one.
#[tauri::command]
pub async fn rebuild_graph_rag(state: State<'_, AppState>) -> CmdResult<RebuildReport> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;

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

    let leiden_result = leiden(
        &node_ids,
        &edges_raw,
        1.0,             // resolution γ
        4,               // max_levels (the spec's "4-level hierarchy")
        30,              // max_iterations per level
        0xA1A0_2026_u64, // seed — same as the original LPA call for continuity
    );

    // Phase batch step 2: if an Anthropic key is configured, build a
    // provider once and use it for every community's summary. Falls
    // back to the extractive summariser when no key is present so the
    // command still works offline.
    let llm_provider: Option<std::sync::Arc<dyn AIProvider>> =
        match load_anthropic_key(&vault.root) {
            Ok(key) => {
                let audit = std::sync::Arc::new(DbAuditLogger::new(vault.db.clone()));
                Some(std::sync::Arc::new(AnthropicProvider::new(key, audit)))
            }
            Err(_) => None,
        };
    let mut llm_summaries: u32 = 0;
    let mut extractive_summaries: u32 = 0;

    // Build the per-level community payloads with parent linkage.
    let mut payloads: Vec<OwnedReplaceCommunity> = Vec::new();
    let mut per_level_counts: Vec<u32> = Vec::with_capacity(leiden_result.levels.len());
    for (level_idx, partition) in leiden_result.levels.iter().enumerate() {
        let parent_partition = if level_idx + 1 < leiden_result.levels.len() {
            Some(&leiden_result.levels[level_idx + 1])
        } else {
            None
        };
        let level_payloads = build_level_payloads(
            level_idx as i64,
            partition,
            parent_partition,
            &id_to_path,
            vault.encoder.as_ref(),
            vault,
            llm_provider.as_deref(),
            &mut llm_summaries,
            &mut extractive_summaries,
        )
        .await?;
        per_level_counts.push(level_payloads.len() as u32);
        payloads.extend(level_payloads);
    }
    // `per_level_counts` is currently fine-to-coarse; reverse for the
    // report so the coarsest level (the "top" of the hierarchy) comes
    // first — matches how a UI would render the breadcrumb.
    per_level_counts.reverse();

    // Project to the borrowed shape the DB expects.
    let borrowed: Vec<ReplaceCommunity<'_>> = payloads
        .iter()
        .map(|p| ReplaceCommunity {
            level: p.level,
            partition_cid: p.partition_cid,
            parent_partition_cid: p.parent_partition_cid,
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
        levels: leiden_result.levels.len() as u32,
        per_level: per_level_counts,
        llm_summaries,
        extractive_summaries,
    })
}

/// Build the per-community payloads for one hierarchy level. If
/// `parent_partition` is `Some`, each community's `parent_partition_cid`
/// is resolved by looking up any of its members in the coarser partition.
#[allow(clippy::too_many_arguments)]
async fn build_level_payloads(
    level: i64,
    partition: &Partition,
    parent_partition: Option<&Partition>,
    id_to_path: &HashMap<String, (String, String)>,
    encoder: &dyn TextEncoder,
    vault: &crate::core::vault::VaultState,
    llm: Option<&dyn AIProvider>,
    llm_summaries: &mut u32,
    extractive_summaries: &mut u32,
) -> Result<Vec<OwnedReplaceCommunity>, AuraError> {
    let mut by_community: HashMap<u32, Vec<String>> = HashMap::new();
    for (node_id, cid) in partition {
        by_community.entry(*cid).or_default().push(node_id.clone());
    }

    let mut out: Vec<OwnedReplaceCommunity> = Vec::with_capacity(by_community.len());
    for (cid, members) in by_community {
        let mut entries: Vec<(String, String)> = Vec::with_capacity(members.len());
        for file_id in &members {
            let Some((path, title)) = id_to_path.get(file_id) else {
                continue;
            };
            let abs = vault
                .resolve(path)
                .map_err(|e| AuraError::from(anyhow::anyhow!(e.to_string())))?;
            let content = std::fs::read_to_string(&abs).unwrap_or_default();
            let lead = leading_paragraphs(&content, 240);
            entries.push((title.clone(), lead));
        }
        let summary = if let Some(provider) = llm {
            let c = CommunityEntries {
                level,
                partition_cid: cid,
                member_entries: &entries,
            };
            match summarize_community(provider, &c).await {
                Ok(s) if !s.trim().is_empty() => {
                    *llm_summaries += 1;
                    s
                }
                Ok(_) | Err(_) => {
                    // LLM returned nothing OR errored (rate-limited,
                    // network, budget) — fall back so a single broken
                    // call doesn't poison the whole rebuild.
                    *extractive_summaries += 1;
                    extractive_summary(&entries)
                }
            }
        } else {
            *extractive_summaries += 1;
            extractive_summary(&entries)
        };
        let emb = encoder.encode(&summary);
        let bytes = embedding_to_bytes(&emb);

        // Parent: any member's community id in the coarser partition.
        let parent_cid: Option<u32> = parent_partition.and_then(|pp| {
            members.first().and_then(|m| pp.get(m).copied())
        });

        out.push(OwnedReplaceCommunity {
            level,
            partition_cid: cid,
            parent_partition_cid: parent_cid,
            member_file_ids: members,
            summary_text: summary,
            embedding: bytes,
            dim: EMBED_DIM as i64,
        });
    }
    Ok(out)
}

/// Run a GraphRAG query. If the community index is empty, returns an empty
/// answer so the UI can show a "run rebuild first" hint.
///
/// Phase batch step 3: when an Anthropic key is configured, the top-K
/// communities feed into a Claude Sonnet call that produces a
/// natural-language answer with inline `[C<n>]` / `[N:<path>]`
/// citation markers. The extracted citation lists land in
/// `cited_communities` + `cited_notes`. Without a key, the answer
/// fields stay `None` and the UI renders `context_payload` as before.
#[tauri::command]
pub async fn graph_rag_query(
    state: State<'_, AppState>,
    question: String,
    limit: Option<u32>,
) -> CmdResult<GraphRagAnswer> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let limit = limit.unwrap_or(3) as usize;
    let mut answer = run_query(&vault.db, vault.encoder.as_ref(), &question, limit)
        .await
        .map_err(AuraError::from)?;

    if answer.communities.is_empty() {
        return Ok(answer);
    }

    if let Ok(key) = load_anthropic_key(&vault.root) {
        let audit = std::sync::Arc::new(DbAuditLogger::new(vault.db.clone()));
        let provider = AnthropicProvider::new(key, audit);
        match crate::core::graph_rag::llm_answer::compose_answer(
            &provider,
            &question,
            &answer.communities,
        )
        .await
        {
            Ok(out) => {
                answer.llm_answer = Some(out.answer);
                answer.cited_communities = out.cited_communities;
                answer.cited_notes = out.cited_notes;
                answer.answer_model = Some(out.model);
            }
            Err(e) => {
                tracing::warn!(
                    target: "aura::graphrag",
                    "LLM answer failed ({e}); returning context-payload only"
                );
            }
        }
    }

    Ok(answer)
}

/// Phase 7a-ii: list every community at a specific hierarchy level. The
/// UI uses this for the "zoom out / zoom in" slider — level 0 is the
/// finest (most communities), `max_community_level()` is the coarsest.
#[derive(Debug, Clone, Serialize)]
pub struct CommunityListItem {
    pub id: i64,
    pub level: i64,
    pub member_count: i64,
    pub member_paths: Vec<String>,
    pub member_titles: Vec<String>,
    pub summary_text: String,
}

#[tauri::command]
pub async fn list_communities_at_level(
    state: State<'_, AppState>,
    level: i64,
) -> CmdResult<Vec<CommunityListItem>> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let rows = vault
        .db
        .list_communities_at_level(level)
        .await
        .map_err(AuraError::from)?;
    Ok(rows
        .into_iter()
        .map(|r| CommunityListItem {
            id: r.id,
            level: r.level,
            member_count: r.member_count,
            member_paths: r.member_paths,
            member_titles: r.member_titles,
            summary_text: r.summary_text,
        })
        .collect())
}

#[tauri::command]
pub async fn max_community_level(state: State<'_, AppState>) -> CmdResult<i64> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;
    let n = vault
        .db
        .max_community_level()
        .await
        .map_err(AuraError::from)?;
    Ok(n)
}

struct OwnedReplaceCommunity {
    level: i64,
    partition_cid: u32,
    parent_partition_cid: Option<u32>,
    member_file_ids: Vec<String>,
    summary_text: String,
    embedding: Vec<u8>,
    dim: i64,
}
