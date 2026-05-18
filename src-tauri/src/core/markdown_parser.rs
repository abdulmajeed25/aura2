use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::Serialize;

/// Lightweight parsed metadata extracted from a Markdown document.
/// Phase 1 only needs title + word count; richer block extraction comes in Phase 2.
#[derive(Debug, Clone, Serialize)]
pub struct ParsedDoc {
    pub title: String,
    pub word_count: i64,
    pub frontmatter: Option<String>,
}

/// Strip a leading YAML frontmatter block (`---\n…\n---`) if present.
/// Returns `(frontmatter_yaml, remaining_body)`.
fn split_frontmatter(input: &str) -> (Option<String>, &str) {
    let trimmed_start = input.trim_start_matches('\u{feff}');
    if let Some(rest) = trimmed_start.strip_prefix("---\n") {
        if let Some(end_idx) = rest.find("\n---\n") {
            let fm = &rest[..end_idx];
            let body_start = end_idx + "\n---\n".len();
            return (Some(fm.to_string()), &rest[body_start..]);
        }
        if let Some(end_idx) = rest.find("\n---") {
            let fm = &rest[..end_idx];
            let body_start = end_idx + "\n---".len();
            return (Some(fm.to_string()), &rest[body_start..]);
        }
    }
    (None, trimmed_start)
}

/// Parse a Markdown document, deriving the title from either YAML frontmatter
/// (`title: …`), the first H1, or the filename (passed in by the caller).
pub fn parse_document(content: &str, fallback_title: &str) -> ParsedDoc {
    let (frontmatter, body) = split_frontmatter(content);

    let mut title: Option<String> = None;
    if let Some(fm) = &frontmatter {
        for line in fm.lines() {
            if let Some(rest) = line.strip_prefix("title:") {
                let v = rest.trim().trim_matches(|c: char| c == '"' || c == '\'');
                if !v.is_empty() {
                    title = Some(v.to_string());
                    break;
                }
            }
        }
    }

    let mut word_count: i64 = 0;
    let mut in_h1 = false;
    let mut h1_buf = String::new();

    for event in Parser::new(body) {
        match event {
            Event::Start(Tag::Heading {
                level: pulldown_cmark::HeadingLevel::H1,
                ..
            }) => {
                in_h1 = true;
            }
            Event::End(TagEnd::Heading(pulldown_cmark::HeadingLevel::H1)) => {
                in_h1 = false;
                if title.is_none() {
                    let t = h1_buf.trim().to_string();
                    if !t.is_empty() {
                        title = Some(t);
                    }
                }
                h1_buf.clear();
            }
            Event::Text(t) => {
                word_count += t.split_whitespace().count() as i64;
                if in_h1 {
                    h1_buf.push_str(&t);
                }
            }
            Event::Code(t) => {
                word_count += t.split_whitespace().count() as i64;
                if in_h1 {
                    h1_buf.push_str(&t);
                }
            }
            _ => {}
        }
    }

    ParsedDoc {
        title: title.unwrap_or_else(|| fallback_title.to_string()),
        word_count,
        frontmatter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_title_from_h1() {
        let doc = parse_document("# Hello World\n\nSome body text.", "fallback");
        assert_eq!(doc.title, "Hello World");
        assert_eq!(doc.word_count, 5);
        assert!(doc.frontmatter.is_none());
    }

    #[test]
    fn falls_back_to_filename_when_no_h1() {
        let doc = parse_document("just a paragraph here", "my-note");
        assert_eq!(doc.title, "my-note");
        assert_eq!(doc.word_count, 4);
    }

    #[test]
    fn extracts_frontmatter_title() {
        let src = "---\ntitle: My Note\ntags: [a, b]\n---\n\n# Wrong\n\nbody";
        let doc = parse_document(src, "fallback");
        assert_eq!(doc.title, "My Note");
        assert!(doc.frontmatter.as_deref().unwrap().contains("tags:"));
    }
}
