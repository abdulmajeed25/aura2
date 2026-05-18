//! Extractive per-community summariser.
//!
//! With no LLM available locally, we build a community summary by
//! concatenating each member's title and its opening paragraphs (the
//! "headline + first sentence" pattern). The result is suitable both for
//! direct display and for embedding into the search-time context payload
//! that the LLM would consume in a full deployment.

const TITLE_LEAD_BUDGET: usize = 220; // chars per member's contribution
const SUMMARY_HARD_CAP: usize = 1200; // chars per community summary

/// Build a community summary from `(title, leading_text)` pairs.
/// `leading_text` should be the first 1-2 paragraphs of the file body
/// (post-frontmatter). We deliberately truncate so a wide community
/// produces a *concise* summary the LLM/embedder can scan in one pass.
pub fn extractive_summary(members: &[(String, String)]) -> String {
    if members.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    for (title, lead) in members {
        let clean_lead = trim_ws(lead);
        let truncated = take_chars(&clean_lead, TITLE_LEAD_BUDGET);
        let entry = if truncated.is_empty() {
            format!("• {}\n", title)
        } else {
            format!("• {}: {}\n", title, truncated)
        };
        if out.len() + entry.len() > SUMMARY_HARD_CAP {
            break;
        }
        out.push_str(&entry);
    }
    out.trim_end().to_string()
}

/// Take the leading `n` chars on a char boundary, appending `…` if the
/// string is longer.
fn take_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n).collect();
    out.push('…');
    out
}

fn trim_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Pull the leading text (up to `max_chars`) out of a Markdown body. Skips
/// frontmatter and stops at the first blank line that follows real content.
pub fn leading_paragraphs(body: &str, max_chars: usize) -> String {
    let (_, body, _) = crate::core::markdown_parser::split_frontmatter(body);
    let mut acc = String::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            // Skip headings — we want narrative text only.
            continue;
        }
        if trimmed.is_empty() {
            if !acc.is_empty() {
                acc.push('\n');
            }
            continue;
        }
        if !acc.is_empty() && !acc.ends_with(' ') && !acc.ends_with('\n') {
            acc.push(' ');
        }
        acc.push_str(trimmed);
        if acc.chars().count() >= max_chars {
            break;
        }
    }
    take_chars(&acc, max_chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_members_yield_empty_summary() {
        assert_eq!(extractive_summary(&[]), "");
    }

    #[test]
    fn summary_respects_hard_cap() {
        let huge: Vec<_> = (0..50)
            .map(|i| (format!("Note {}", i), "lorem ipsum dolor sit amet ".repeat(20)))
            .collect();
        let s = extractive_summary(&huge);
        assert!(s.len() <= SUMMARY_HARD_CAP + 10, "got {} chars", s.len());
    }

    #[test]
    fn leading_paragraphs_skips_headings_and_frontmatter() {
        let body = "---\ntitle: x\n---\n\n# Heading\n\nFirst paragraph here.\n\n## Sub\n\nSecond paragraph.";
        let lead = leading_paragraphs(body, 200);
        assert!(lead.contains("First paragraph"));
        assert!(lead.contains("Second paragraph"));
        assert!(!lead.contains("Heading"));
    }
}
