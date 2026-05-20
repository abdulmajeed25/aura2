//! Phase batch step 2: replace extractive community summaries with
//! Claude-Haiku-generated paraphrases.
//!
//! When a community has > 20 members the prompt would balloon past
//! Haiku's optimal context size; we chunk into groups of 12, get a
//! sub-summary per chunk, then meta-summarise. The 1-hour cache
//! breakpoint sits on the system prompt so we pay full price only on
//! the first community of a rebuild; the rest read from cache.
//!
//! Honest fallback: this module is **only** wired in when a real
//! `AIProvider` is available. The caller (`rebuild_graph_rag`) drops
//! back to the existing extractive summariser when no Anthropic key
//! is configured — the registry entry for #3 says exactly that.

use crate::ai::providers::{AIProvider, AiError, CacheTtl, ChatRequest};

/// Hand-crafted system prompt for community summarisation. Stable
/// preamble + per-community body; we mark this block as 1-hour cached
/// so the second community onwards reads from cache.
pub const SYSTEM_PROMPT: &str = "You are a knowledge-graph community \
    summariser. Given a list of related Markdown notes from a personal \
    vault, write ONE concise paragraph (3–5 sentences, ≤120 words) that \
    captures what theme or thread connects them. Be specific: name the \
    project, concept, or pattern. Do NOT list the note titles verbatim. \
    Do NOT add bullet points, headings, or quotation marks. Output the \
    paragraph and nothing else.";

const HAIKU_MODEL: &str = "claude-haiku-4-5";
const CHUNK_SIZE: usize = 12;

/// One community's notes, ready to feed into the LLM.
pub struct CommunityEntries<'a> {
    pub level: i64,
    pub partition_cid: u32,
    pub member_entries: &'a [(String, String)], // (title, leading_paragraph)
}

/// Generate a paragraph-length summary for one community. For
/// communities with ≤ `CHUNK_SIZE` members, this is one LLM call. For
/// larger communities, it chunks → sub-summaries → meta-summary.
pub async fn summarize_community(
    provider: &dyn AIProvider,
    community: &CommunityEntries<'_>,
) -> Result<String, AiError> {
    let entries = community.member_entries;
    if entries.is_empty() {
        return Ok(String::new());
    }

    if entries.len() <= CHUNK_SIZE {
        return one_shot(provider, entries, community).await;
    }

    // Chunk-then-meta-summarise for large communities.
    let mut chunk_summaries = Vec::with_capacity(entries.len().div_ceil(CHUNK_SIZE));
    for chunk in entries.chunks(CHUNK_SIZE) {
        chunk_summaries.push(one_shot(provider, chunk, community).await?);
    }
    let meta_entries: Vec<(String, String)> = chunk_summaries
        .into_iter()
        .enumerate()
        .map(|(i, s)| (format!("Chunk {}", i + 1), s))
        .collect();
    one_shot(provider, &meta_entries, community).await
}

async fn one_shot(
    provider: &dyn AIProvider,
    entries: &[(String, String)],
    community: &CommunityEntries<'_>,
) -> Result<String, AiError> {
    let user = render_user_prompt(entries);
    let metadata = serde_json::json!({
        "op": "community_summary",
        "level": community.level,
        "partition_cid": community.partition_cid,
        "n_members": entries.len(),
    });
    let req = ChatRequest::new(HAIKU_MODEL, user)
        .with_system(SYSTEM_PROMPT, Some(CacheTtl::OneHour))
        .with_max_tokens(400)
        .with_temperature(0.3)
        .with_metadata(metadata);
    let resp = provider.chat(req).await?;
    Ok(resp.content.trim().to_string())
}

fn render_user_prompt(entries: &[(String, String)]) -> String {
    let mut s = String::with_capacity(entries.len() * 200);
    s.push_str("Summarise the theme connecting these notes:\n\n");
    for (i, (title, lead)) in entries.iter().enumerate() {
        s.push_str(&format!("{}. **{}**\n   {}\n\n", i + 1, title, lead));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::providers::mock::MockProvider;
    use crate::ai::providers::CacheTtl as Ttl;

    #[tokio::test]
    async fn small_community_makes_exactly_one_llm_call() {
        let provider = MockProvider::with_response("Notes about morning routine + deep work.");
        let entries = vec![
            ("Morning Routine".to_string(), "Wake 0530, walk, then deep work.".to_string()),
            ("Deep Work".to_string(), "90-minute focused blocks, no notifications.".to_string()),
        ];
        let c = CommunityEntries {
            level: 0,
            partition_cid: 0,
            member_entries: &entries,
        };
        let out = summarize_community(&provider, &c).await.unwrap();
        assert!(!out.is_empty());
        assert_eq!(provider.call_count(), 1);
    }

    #[tokio::test]
    async fn large_community_chunks_then_meta_summarises() {
        // 25 members → ceil(25/12) = 3 chunks → 1 meta = 4 total calls.
        let entries: Vec<(String, String)> = (0..25)
            .map(|i| (format!("Note {i}"), format!("Body of note {i}.")))
            .collect();
        let provider = MockProvider::with_response("Generated summary.");
        let c = CommunityEntries {
            level: 0,
            partition_cid: 0,
            member_entries: &entries,
        };
        let out = summarize_community(&provider, &c).await.unwrap();
        assert!(!out.is_empty());
        assert_eq!(
            provider.call_count(),
            4,
            "expected 3 chunks + 1 meta = 4 calls"
        );
    }

    #[tokio::test]
    async fn cache_control_breakpoint_present_on_system_prompt() {
        let provider = MockProvider::with_response("summary");
        let entries = vec![("X".to_string(), "Y".to_string())];
        let c = CommunityEntries {
            level: 0,
            partition_cid: 0,
            member_entries: &entries,
        };
        let _ = summarize_community(&provider, &c).await.unwrap();
        let reqs = provider.received_requests();
        assert_eq!(reqs.len(), 1);
        // System block carries the cache hint:
        let sys = &reqs[0].system;
        assert_eq!(sys.len(), 1);
        assert_eq!(sys[0].cache, Some(Ttl::OneHour));
    }

    #[tokio::test]
    async fn empty_community_returns_empty_summary_without_calling_provider() {
        let provider = MockProvider::with_response("ignored");
        let c = CommunityEntries {
            level: 0,
            partition_cid: 0,
            member_entries: &[],
        };
        let out = summarize_community(&provider, &c).await.unwrap();
        assert!(out.is_empty());
        assert_eq!(provider.call_count(), 0);
    }

    #[tokio::test]
    async fn metadata_carries_operation_marker() {
        let provider = MockProvider::with_response("summary");
        let entries = vec![("X".to_string(), "Y".to_string())];
        let c = CommunityEntries {
            level: 1,
            partition_cid: 42,
            member_entries: &entries,
        };
        let _ = summarize_community(&provider, &c).await.unwrap();
        let reqs = provider.received_requests();
        assert_eq!(reqs[0].metadata["op"], "community_summary");
        assert_eq!(reqs[0].metadata["level"], 1);
        assert_eq!(reqs[0].metadata["partition_cid"], 42);
    }
}
