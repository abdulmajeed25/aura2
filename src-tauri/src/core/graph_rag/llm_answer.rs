//! Phase batch step 3: Claude-Sonnet GraphRAG answer with inline
//! citation markers.
//!
//! Pipeline:
//! 1. Take the top-K community hits from `query_engine::run_query`.
//! 2. Format their summaries + member titles into a system prompt
//!    (1-hour cached — the vault's stable context).
//! 3. Send the user's question.
//! 4. Parse `[C<n>]` (community) and `[N:<path>]` (note) markers from
//!    the response.
//!
//! For simplicity this gate skips the mode classifier (local /
//! global / drift) from the spec — every query runs "global mode" with
//! community summaries. A future gate can add a Haiku-cached classifier
//! that picks the mode based on the query shape.

use serde::Serialize;

use crate::ai::providers::{AIProvider, AiError, CacheTtl, ChatRequest};
use crate::core::graph_rag::query_engine::CommunityHit;

const SONNET_MODEL: &str = "claude-sonnet-4-6";

pub const SYSTEM_PROMPT: &str = "You are answering a question about the \
    user's personal knowledge graph. You will receive a list of \
    candidate communities — each is a cluster of related notes — \
    along with a one-paragraph summary and the titles of the notes in \
    that community. Answer the user's question using ONLY information \
    grounded in those communities. \
    \
    When you make a claim grounded in a community, mark the claim with \
    `[C<community_id>]` (where the id is the integer you see in the \
    community header). When you cite a specific note, use \
    `[N:<note_path>]` (the exact path string we provided). Put markers \
    inline at the end of the relevant sentence. Do not fabricate IDs \
    or paths. If the candidate communities don't answer the question, \
    say so explicitly.";

#[derive(Debug, Clone, Serialize)]
pub struct LlmGraphRagAnswer {
    pub answer: String,
    pub cited_communities: Vec<i64>,
    pub cited_notes: Vec<String>,
    pub model: String,
    pub usage_tokens: u32,
}

/// Build the user prompt that lays out the candidate communities and
/// the user's question.
pub fn render_user_prompt(question: &str, communities: &[CommunityHit]) -> String {
    let mut s = String::new();
    s.push_str("# Candidate communities\n\n");
    for c in communities {
        s.push_str(&format!(
            "## Community [C{}]  (score {:.3}, level {}, {} notes)\n\n",
            c.community_id,
            c.score,
            c.level,
            c.member_paths.len()
        ));
        s.push_str(&c.summary_text);
        s.push_str("\n\nNotes:\n");
        for (path, title) in c.member_paths.iter().zip(c.member_titles.iter()) {
            s.push_str(&format!("- [N:{path}] — {title}\n"));
        }
        s.push('\n');
    }
    s.push_str("---\n\n# Question\n\n");
    s.push_str(question);
    s
}

/// One LLM call to compose the natural-language answer.
pub async fn compose_answer(
    provider: &dyn AIProvider,
    question: &str,
    communities: &[CommunityHit],
) -> Result<LlmGraphRagAnswer, AiError> {
    let user = render_user_prompt(question, communities);
    let req = ChatRequest::new(SONNET_MODEL, user)
        .with_system(SYSTEM_PROMPT, Some(CacheTtl::OneHour))
        .with_max_tokens(1024)
        .with_temperature(0.4)
        .with_metadata(serde_json::json!({
            "op": "graphrag_answer",
            "candidate_communities": communities.iter().map(|c| c.community_id).collect::<Vec<_>>(),
        }));
    let resp = provider.chat(req).await?;
    let (cited_communities, cited_notes) = parse_citations(&resp.content);
    Ok(LlmGraphRagAnswer {
        answer: resp.content,
        cited_communities,
        cited_notes,
        model: resp.model,
        usage_tokens: resp.usage.input_tokens + resp.usage.output_tokens,
    })
}

/// Extract every `[C<n>]` and `[N:<path>]` marker. Deduplicates while
/// preserving first-seen order so the UI can render citations in the
/// order Claude introduced them.
pub fn parse_citations(text: &str) -> (Vec<i64>, Vec<String>) {
    let mut communities: Vec<i64> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // Walk the text char-by-char looking for `[C<digits>]` and
    // `[N:<path>]`. A small hand-rolled scanner is simpler than a
    // regex dep and produces the same output.
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        // Look for `[C<n>]`.
        if i + 1 < bytes.len() && bytes[i + 1] == b'C' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 2 && j < bytes.len() && bytes[j] == b']' {
                let id: i64 = std::str::from_utf8(&bytes[i + 2..j])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(-1);
                if id >= 0 && !communities.contains(&id) {
                    communities.push(id);
                }
                i = j + 1;
                continue;
            }
        }
        // Look for `[N:<path>]`. Stop at the next `]`, but ALSO stop at
        // any `[` so a malformed unterminated `[N:` doesn't swallow a
        // following valid `[C…]` marker.
        if i + 2 < bytes.len() && bytes[i + 1] == b'N' && bytes[i + 2] == b':' {
            let mut j = i + 3;
            while j < bytes.len() && bytes[j] != b']' && bytes[j] != b'[' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b']' && j > i + 3 {
                if let Ok(path) = std::str::from_utf8(&bytes[i + 3..j]) {
                    let p = path.to_string();
                    if !notes.contains(&p) {
                        notes.push(p);
                    }
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    (communities, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::providers::mock::MockProvider;

    fn hit(id: i64, summary: &str, paths: &[&str]) -> CommunityHit {
        CommunityHit {
            community_id: id,
            level: 0,
            member_count: paths.len() as i64,
            member_paths: paths.iter().map(|s| s.to_string()).collect(),
            member_titles: paths.iter().map(|s| s.to_string()).collect(),
            summary_text: summary.into(),
            score: 0.5,
        }
    }

    #[tokio::test]
    async fn compose_calls_provider_once_and_parses_citations() {
        let response_text = "Your morning workflow centers on a routine \
            that frames deep work before lunch [C42]. The roadmap note \
            [N:projects/aura/roadmap.md] aligns with that. The second \
            community [C42] reinforces this pattern (duplicate marker \
            should be de-duped).";
        let provider = MockProvider::with_response(response_text);
        let hits = vec![hit(42, "Productivity habits and roadmap.", &["projects/aura/roadmap.md"])];
        let out = compose_answer(&provider, "How do my mornings tie to Aura?", &hits)
            .await
            .unwrap();
        assert_eq!(out.cited_communities, vec![42]); // de-duped
        assert_eq!(out.cited_notes, vec!["projects/aura/roadmap.md"]);
        assert_eq!(provider.call_count(), 1);
    }

    #[tokio::test]
    async fn user_prompt_lists_each_community_with_id_and_member_paths() {
        let provider = MockProvider::with_response("ok");
        let hits = vec![
            hit(1, "Cluster A summary.", &["a/one.md", "a/two.md"]),
            hit(2, "Cluster B summary.", &["b/one.md"]),
        ];
        compose_answer(&provider, "question?", &hits).await.unwrap();
        let req = &provider.received_requests()[0];
        let user_text = &req.messages[0].content[0].text;
        // Each community header appears with its [C<id>] marker.
        assert!(user_text.contains("Community [C1]"));
        assert!(user_text.contains("Community [C2]"));
        // Each member path appears via the [N:<path>] reference syntax.
        assert!(user_text.contains("[N:a/one.md]"));
        assert!(user_text.contains("[N:a/two.md]"));
        assert!(user_text.contains("[N:b/one.md]"));
    }

    #[tokio::test]
    async fn metadata_marks_op_and_candidate_ids() {
        let provider = MockProvider::with_response("ok");
        let hits = vec![hit(7, "x", &["a.md"])];
        compose_answer(&provider, "q", &hits).await.unwrap();
        let req = &provider.received_requests()[0];
        assert_eq!(req.metadata["op"], "graphrag_answer");
        assert_eq!(req.metadata["candidate_communities"][0], 7);
    }

    #[test]
    fn parse_citations_handles_mixed_markers() {
        let (c, n) = parse_citations("Foo [C12] bar [N:x.md] baz [C9] qux [N:y.md] [C12]");
        assert_eq!(c, vec![12, 9]); // de-dup, first-seen order
        assert_eq!(n, vec!["x.md", "y.md"]);
    }

    #[test]
    fn parse_citations_ignores_malformed_brackets() {
        // `[C` not followed by digits — ignored.
        // `[X42]` — wrong prefix.
        // `[N:` unterminated — ignored.
        let (c, n) = parse_citations("noise [C] [X42] [N:unterminated and [C5]");
        assert_eq!(c, vec![5]);
        assert!(n.is_empty());
    }

    #[test]
    fn parse_citations_on_empty_text_yields_empty() {
        let (c, n) = parse_citations("");
        assert!(c.is_empty() && n.is_empty());
    }

    #[tokio::test]
    async fn cached_system_prompt_carries_one_hour_breakpoint() {
        let provider = MockProvider::with_response("ok");
        compose_answer(&provider, "q", &[hit(1, "s", &["a.md"])])
            .await
            .unwrap();
        let req = &provider.received_requests()[0];
        assert_eq!(req.system.len(), 1);
        assert_eq!(req.system[0].cache, Some(CacheTtl::OneHour));
    }
}
