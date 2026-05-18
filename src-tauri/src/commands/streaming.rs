use serde::Serialize;
use tauri::State;

use crate::core::embeddings::{HashEmbedder, TextEncoder, EMBED_DIM};
use crate::core::graph_rag::query_engine::{run_query, GraphRagAnswer};
use crate::core::ssm::StreamingState;
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

/// Lightweight wire format describing the current state of the SSM session.
#[derive(Debug, Clone, Serialize)]
pub struct SsmStatus {
    pub dim: usize,
    pub step_count: u32,
    pub saturation: f32,
    pub last_input_alignment: f32,
    pub active: bool,
}

/// Read the SSM status. Returns `active=false` if no session has been
/// initialised yet.
#[tauri::command]
pub async fn ssm_status(state: State<'_, AppState>) -> CmdResult<SsmStatus> {
    let guard = state.ssm.lock().await;
    match guard.as_ref() {
        Some(s) => Ok(SsmStatus {
            dim: s.dim,
            step_count: s.step_count,
            saturation: s.saturation(),
            last_input_alignment: s.last_input_alignment,
            active: true,
        }),
        None => Ok(SsmStatus {
            dim: EMBED_DIM,
            step_count: 0,
            saturation: 0.0,
            last_input_alignment: 0.0,
            active: false,
        }),
    }
}

/// Reset (or initialise) the streaming session to a fresh zero state.
#[tauri::command]
pub async fn ssm_reset(state: State<'_, AppState>) -> CmdResult<SsmStatus> {
    let mut guard = state.ssm.lock().await;
    let s = guard.get_or_insert_with(|| StreamingState::new(EMBED_DIM));
    s.reset();
    Ok(SsmStatus {
        dim: s.dim,
        step_count: 0,
        saturation: 0.0,
        last_input_alignment: 0.0,
        active: true,
    })
}

/// Encode the supplied text and run a single SSM step, updating the hidden
/// state. Returns the post-step status.
#[tauri::command]
pub async fn ssm_step_text(
    state: State<'_, AppState>,
    text: String,
) -> CmdResult<SsmStatus> {
    let encoder = HashEmbedder::new();
    let emb = encoder.encode(&text);

    let mut guard = state.ssm.lock().await;
    let s = guard.get_or_insert_with(|| StreamingState::new(EMBED_DIM));
    let alignment = s.step(&emb);

    Ok(SsmStatus {
        dim: s.dim,
        step_count: s.step_count,
        saturation: s.saturation(),
        last_input_alignment: alignment,
        active: true,
    })
}

/// Continuous-mode chat: step the SSM with `question`, then run a GraphRAG
/// query whose retrieval embedding is the *fused* state-+-question vector.
/// This is the spec's "Mamba Stream → process the HV via Mamba for context
/// state → Final Prompt Assembly" reduced to its essential shape.
#[derive(Debug, Clone, Serialize)]
pub struct StreamingChatTurn {
    pub answer: GraphRagAnswer,
    pub status: SsmStatus,
}

#[tauri::command]
pub async fn streaming_chat(
    state: State<'_, AppState>,
    question: String,
    blend: Option<f32>,
    limit: Option<u32>,
) -> CmdResult<StreamingChatTurn> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard.as_ref().ok_or(AuraError::NoVault)?;

    let encoder = HashEmbedder::new();
    let q_vec = encoder.encode(&question);

    let mut ssm_guard = state.ssm.lock().await;
    let s = ssm_guard.get_or_insert_with(|| StreamingState::new(EMBED_DIM));

    // Step BEFORE producing the fused query so the state already integrates
    // the latest question; then blend with the just-encoded question for
    // retrieval. Saturation `blend` defaults to 0.5 → equal weight.
    let alignment = s.step(&q_vec);
    let blend = blend.unwrap_or(0.5);
    let fused = s.compose_query(&q_vec, blend);

    let status = SsmStatus {
        dim: s.dim,
        step_count: s.step_count,
        saturation: s.saturation(),
        last_input_alignment: alignment,
        active: true,
    };
    drop(ssm_guard);

    let answer = run_query_with_fused(&vault.db, &fused, &question, limit.unwrap_or(3) as usize)
        .await
        .map_err(AuraError::from)?;

    Ok(StreamingChatTurn { answer, status })
}

/// Variant of `run_query` that uses a pre-computed query embedding (the
/// fused state-+-question vector) instead of re-encoding `question`.
async fn run_query_with_fused(
    db: &crate::db::sqlite::VaultDb,
    fused: &[f32],
    question: &str,
    limit: usize,
) -> anyhow::Result<GraphRagAnswer> {
    use crate::core::embeddings::{bytes_to_embedding, cosine_similarity};
    use crate::core::graph_rag::query_engine::CommunityHit;
    use rayon::prelude::*;

    if fused.iter().all(|x| *x == 0.0) {
        // Fall back to the plain encoder path so we never look up an empty vec.
        return run_query(db, question, limit).await;
    }

    let communities = db.all_communities_with_members().await?;
    if communities.is_empty() {
        return run_query(db, question, limit).await;
    }

    let mut scored: Vec<CommunityHit> = communities
        .par_iter()
        .map(|c| {
            let emb = bytes_to_embedding(&c.embedding);
            let score = cosine_similarity(fused, &emb);
            CommunityHit {
                community_id: c.id,
                level: c.level,
                member_count: c.member_count,
                member_paths: c.member_paths.clone(),
                member_titles: c.member_titles.clone(),
                summary_text: c.summary_text.clone(),
                score,
            }
        })
        .collect();
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit.clamp(1, 20));

    let mut context = String::new();
    for (i, h) in scored.iter().enumerate() {
        context.push_str(&format!(
            "## Theme {} (notes: {}, score: {:.3})\n{}\n\n",
            i + 1,
            h.member_count,
            h.score,
            h.summary_text
        ));
    }
    let estimated_tokens = (context.len() as f32 / 4.0).ceil() as u32;
    let covered_notes: u32 = scored.iter().map(|c| c.member_count as u32).sum();

    Ok(GraphRagAnswer {
        question: question.to_string(),
        communities: scored,
        context_payload: context.trim_end().to_string(),
        estimated_tokens,
        covered_notes,
    })
}
