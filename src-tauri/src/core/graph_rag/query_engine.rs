//! GraphRAG query engine.
//!
//! Given a natural-language question, pick the top-K community summaries by
//! semantic similarity and return them assembled into a compact "context
//! payload". A real deployment pipes this payload into an LLM as the
//! retrieval-augmented context; the engine itself is LLM-free so it stays
//! deterministic and testable.

use anyhow::Result;
use rayon::prelude::*;
use serde::Serialize;

use crate::core::embeddings::{bytes_to_embedding, cosine_similarity, TextEncoder};
use crate::db::sqlite::VaultDb;

#[derive(Debug, Clone, Serialize)]
pub struct CommunityHit {
    pub community_id: i64,
    pub level: i64,
    pub member_count: i64,
    pub member_paths: Vec<String>,
    pub member_titles: Vec<String>,
    pub summary_text: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphRagAnswer {
    /// The question, echoed back so the UI can render it next to results.
    pub question: String,
    /// Top-K community matches in descending score order.
    pub communities: Vec<CommunityHit>,
    /// Pre-assembled compact context the UI (or an LLM) can consume directly.
    pub context_payload: String,
    /// Approximate token count of the payload (chars/4 heuristic) — useful
    /// for proving the spec's "≤ 2% of original tokens" headline.
    pub estimated_tokens: u32,
    /// How many notes the answer effectively summarises.
    pub covered_notes: u32,
    /// Phase batch step 3: Claude-Sonnet-generated natural-language
    /// answer with inline citation markers. `None` when no Anthropic
    /// key is configured or the LLM call failed (the UI falls back to
    /// rendering `context_payload`).
    #[serde(default)]
    pub llm_answer: Option<String>,
    /// Community IDs cited inline as `[C<n>]` in `llm_answer`. Empty
    /// when no LLM answer.
    #[serde(default)]
    pub cited_communities: Vec<i64>,
    /// Note paths cited inline as `[N:<path>]` in `llm_answer`.
    #[serde(default)]
    pub cited_notes: Vec<String>,
    /// Model id that produced `llm_answer`, if any.
    #[serde(default)]
    pub answer_model: Option<String>,
}

/// Run a GraphRAG query against the community index.
pub async fn run_query(
    db: &VaultDb,
    encoder: &dyn TextEncoder,
    question: &str,
    limit: usize,
) -> Result<GraphRagAnswer> {
    let q = question.trim();
    if q.is_empty() {
        return Ok(empty_answer(q));
    }

    let q_vec = encoder.encode(q);
    if q_vec.iter().all(|x| *x == 0.0) {
        return Ok(empty_answer(q));
    }

    let communities = db.all_communities_with_members().await?;
    if communities.is_empty() {
        return Ok(empty_answer(q));
    }

    // Score each community in parallel.
    let mut scored: Vec<CommunityHit> = communities
        .par_iter()
        .map(|c| {
            let emb = bytes_to_embedding(&c.embedding);
            let score = cosine_similarity(&q_vec, &emb);
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

    let context_payload = assemble_context(&scored);
    let estimated_tokens = (context_payload.len() as f32 / 4.0).ceil() as u32;
    let covered_notes: u32 = scored.iter().map(|c| c.member_count as u32).sum();

    Ok(GraphRagAnswer {
        question: q.to_string(),
        communities: scored,
        context_payload,
        estimated_tokens,
        covered_notes,
        llm_answer: None,
        cited_communities: Vec::new(),
        cited_notes: Vec::new(),
        answer_model: None,
    })
}

fn empty_answer(question: &str) -> GraphRagAnswer {
    GraphRagAnswer {
        question: question.to_string(),
        communities: Vec::new(),
        context_payload: String::new(),
        estimated_tokens: 0,
        covered_notes: 0,
        llm_answer: None,
        cited_communities: Vec::new(),
        cited_notes: Vec::new(),
        answer_model: None,
    }
}

fn assemble_context(hits: &[CommunityHit]) -> String {
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        out.push_str(&format!(
            "## Theme {} (notes: {}, score: {:.3})\n{}\n\n",
            i + 1,
            h.member_count,
            h.score,
            h.summary_text
        ));
    }
    out.trim_end().to_string()
}

/// Row payload produced by `VaultDb::all_communities_with_members`.
pub struct CommunityRow {
    pub id: i64,
    pub level: i64,
    pub member_count: i64,
    pub member_paths: Vec<String>,
    pub member_titles: Vec<String>,
    pub summary_text: String,
    pub embedding: Vec<u8>,
}
