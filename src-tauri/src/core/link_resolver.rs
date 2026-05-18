use serde::Serialize;

/// A wiki-link as it appears in the source, with its components parsed out.
#[derive(Debug, Clone, Serialize)]
pub struct RawWikiLink {
    /// Full `[[...]]` text including the brackets.
    pub raw: String,
    /// Filename (without extension) or vault-relative path that the link points at.
    pub target: String,
    /// `#Heading` part, if any.
    pub heading: Option<String>,
    /// `#^block-ref` part, if any.
    pub block_ref: Option<String>,
    /// Pipe alias (`|display`), if any.
    pub display: Option<String>,
    /// Zero-based line where the link starts.
    pub line: u32,
    /// Zero-based column where the `[[` starts on that line.
    pub column: u32,
    /// Byte offset of `[[` in the source.
    pub byte_offset: usize,
}

/// Scan a document for `[[...]]` wiki links. Code blocks/spans are NOT excluded —
/// the parser walks the raw source. This is fine in practice because the user
/// rarely embeds `[[ ]]` literally inside fenced code, and Phase 2 doesn't need
/// the extra complexity.
pub fn scan_wiki_links(content: &str) -> Vec<RawWikiLink> {
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    let mut i: usize = 0;

    while i + 1 < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                line_start = i + 1;
                i += 1;
            }
            b'[' if bytes[i + 1] == b'[' => {
                let start = i;
                let col = (start - line_start) as u32;
                let start_line = line;
                let mut j = i + 2;
                let mut closed = false;
                let mut nested_open: Option<usize> = None;

                while j + 1 < bytes.len() {
                    if bytes[j] == b'\n' {
                        break;
                    }
                    if bytes[j] == b'[' && bytes[j + 1] == b'[' {
                        // Outer is unclosed; restart from the nested `[[`.
                        nested_open = Some(j);
                        break;
                    }
                    if bytes[j] == b']' && bytes[j + 1] == b']' {
                        let inner = &content[start + 2..j];
                        if let Some(parsed) = parse_link_inner(inner) {
                            out.push(RawWikiLink {
                                raw: content[start..j + 2].to_string(),
                                target: parsed.target,
                                heading: parsed.heading,
                                block_ref: parsed.block_ref,
                                display: parsed.display,
                                line: start_line,
                                column: col,
                                byte_offset: start,
                            });
                        }
                        i = j + 2;
                        closed = true;
                        break;
                    }
                    j += 1;
                }

                if !closed {
                    // Move past the outer `[[`, or to the nested `[[` if we saw one.
                    i = nested_open.unwrap_or(start + 1);
                }
            }
            _ => i += 1,
        }
    }
    out
}

struct ParsedInner {
    target: String,
    heading: Option<String>,
    block_ref: Option<String>,
    display: Option<String>,
}

fn parse_link_inner(inner: &str) -> Option<ParsedInner> {
    let (lhs, display) = match inner.find('|') {
        Some(idx) => (&inner[..idx], Some(inner[idx + 1..].trim().to_string())),
        None => (inner, None),
    };

    let (file_part, after_hash) = match lhs.find('#') {
        Some(idx) => (&lhs[..idx], Some(&lhs[idx + 1..])),
        None => (lhs, None),
    };

    let target = file_part.trim().to_string();
    if target.is_empty() && after_hash.is_none() {
        return None;
    }

    let (heading, block_ref) = match after_hash {
        Some(s) if s.starts_with('^') => (None, Some(s[1..].trim().to_string())),
        Some(s) if !s.trim().is_empty() => (Some(s.trim().to_string()), None),
        _ => (None, None),
    };

    Some(ParsedInner {
        target,
        heading,
        block_ref,
        display,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_wiki_link() {
        let links = scan_wiki_links("see [[Welcome]] for more");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Welcome");
        assert!(links[0].heading.is_none());
        assert!(links[0].block_ref.is_none());
        assert!(links[0].display.is_none());
        assert_eq!(links[0].raw, "[[Welcome]]");
        assert_eq!(links[0].line, 0);
        assert_eq!(links[0].column, 4);
    }

    #[test]
    fn parses_aliased_heading_and_block_refs() {
        let src = "a [[Note|alt]] b [[Note#Section]] c [[Note#^abc]]";
        let links = scan_wiki_links(src);
        assert_eq!(links.len(), 3);

        assert_eq!(links[0].target, "Note");
        assert_eq!(links[0].display.as_deref(), Some("alt"));

        assert_eq!(links[1].target, "Note");
        assert_eq!(links[1].heading.as_deref(), Some("Section"));

        assert_eq!(links[2].target, "Note");
        assert_eq!(links[2].block_ref.as_deref(), Some("abc"));
    }

    #[test]
    fn handles_folder_paths_and_multiple_lines() {
        let src = "first [[Folder/Note]]\nsecond [[Other Note#Heading|short]]";
        let links = scan_wiki_links(src);
        assert_eq!(links.len(), 2);

        assert_eq!(links[0].target, "Folder/Note");
        assert_eq!(links[0].line, 0);

        assert_eq!(links[1].target, "Other Note");
        assert_eq!(links[1].heading.as_deref(), Some("Heading"));
        assert_eq!(links[1].display.as_deref(), Some("short"));
        assert_eq!(links[1].line, 1);
    }

    #[test]
    fn ignores_unclosed_and_newline_bridged_links() {
        let src = "open [[never closed and [[Real]]";
        let links = scan_wiki_links(src);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Real");
    }
}
