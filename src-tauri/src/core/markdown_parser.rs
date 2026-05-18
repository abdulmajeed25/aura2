use pulldown_cmark::{
    Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Result of parsing a single Markdown document.
#[derive(Debug, Clone, Serialize)]
pub struct ParsedDoc {
    pub title: String,
    pub word_count: i64,
    pub frontmatter: Option<String>,
    pub blocks: Vec<ParsedBlock>,
}

/// A top-level block extracted from a Markdown document.
#[derive(Debug, Clone, Serialize)]
pub struct ParsedBlock {
    pub block_type: BlockType,
    /// Heading level (1-6). Zero for non-heading blocks.
    pub level: u32,
    /// The raw markdown source covered by this block (post-frontmatter).
    pub content: String,
    pub content_hash: String,
    /// Zero-based byte offset in the document (post-frontmatter body) where
    /// this block starts.
    pub byte_start: usize,
    pub byte_end: usize,
    /// Zero-based line number within the document (post-frontmatter body).
    pub line_number: u32,
    pub order_index: u32,
    /// User-assigned reference (`^abc123` trailer), if any.
    pub user_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Paragraph,
    Heading,
    BlockQuote,
    CodeBlock,
    List,
    HtmlBlock,
    Table,
    FootnoteDefinition,
    Other,
}

impl BlockType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlockType::Paragraph => "paragraph",
            BlockType::Heading => "heading",
            BlockType::BlockQuote => "block_quote",
            BlockType::CodeBlock => "code_block",
            BlockType::List => "list",
            BlockType::HtmlBlock => "html_block",
            BlockType::Table => "table",
            BlockType::FootnoteDefinition => "footnote_definition",
            BlockType::Other => "other",
        }
    }
}

/// A heading entry used to render the outline panel.
#[derive(Debug, Clone, Serialize)]
pub struct HeadingEntry {
    pub level: u32,
    pub text: String,
    pub line_number: u32,
}

/// Strip a leading YAML frontmatter block and return `(frontmatter, body, body_offset)`
/// where `body_offset` is the byte offset of `body` within the original input.
pub fn split_frontmatter(input: &str) -> (Option<String>, &str, usize) {
    let trimmed = input.trim_start_matches('\u{feff}');
    let bom_skip = input.len() - trimmed.len();
    if let Some(rest) = trimmed.strip_prefix("---\n") {
        if let Some(end_idx) = rest.find("\n---\n") {
            let fm = &rest[..end_idx];
            let body_start_in_rest = end_idx + "\n---\n".len();
            let body = &rest[body_start_in_rest..];
            let offset = bom_skip + "---\n".len() + body_start_in_rest;
            return (Some(fm.to_string()), body, offset);
        }
        if let Some(end_idx) = rest.find("\n---") {
            let fm = &rest[..end_idx];
            let body_start_in_rest = end_idx + "\n---".len();
            let body = &rest[body_start_in_rest..];
            let offset = bom_skip + "---\n".len() + body_start_in_rest;
            return (Some(fm.to_string()), body, offset);
        }
    }
    (None, trimmed, bom_skip)
}

/// Parse a Markdown document, returning title, word count, frontmatter, and blocks.
pub fn parse_document(content: &str, fallback_title: &str) -> ParsedDoc {
    let (frontmatter, body, _body_offset) = split_frontmatter(content);

    let title_from_fm = frontmatter
        .as_deref()
        .and_then(extract_title_from_frontmatter);

    let blocks = extract_blocks(body);
    let word_count = visible_word_count(body);

    let title = title_from_fm
        .or_else(|| first_heading_text(&blocks))
        .unwrap_or_else(|| fallback_title.to_string());

    ParsedDoc {
        title,
        word_count,
        frontmatter,
        blocks,
    }
}

/// Convenience: parse and return only the heading outline.
pub fn parse_outline(content: &str) -> Vec<HeadingEntry> {
    let (_, body, _) = split_frontmatter(content);
    let blocks = extract_blocks(body);
    blocks
        .into_iter()
        .filter(|b| b.block_type == BlockType::Heading)
        .map(|b| HeadingEntry {
            level: b.level,
            text: heading_text(&b.content),
            line_number: b.line_number,
        })
        .collect()
}

fn extract_title_from_frontmatter(fm: &str) -> Option<String> {
    for line in fm.lines() {
        if let Some(rest) = line.strip_prefix("title:") {
            let v = rest.trim().trim_matches(|c: char| c == '"' || c == '\'');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn first_heading_text(blocks: &[ParsedBlock]) -> Option<String> {
    blocks
        .iter()
        .find(|b| b.block_type == BlockType::Heading && b.level == 1)
        .map(|b| heading_text(&b.content))
}

fn heading_text(content: &str) -> String {
    let line = content.lines().next().unwrap_or("");
    line.trim_start_matches('#').trim().to_string()
}

/// Count visible words in a Markdown body, excluding syntax characters
/// (`#`, `>`, fence backticks, etc.). Backed by the parser's Text/Code events.
fn visible_word_count(body: &str) -> i64 {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let mut count: i64 = 0;
    for event in Parser::new_ext(body, options) {
        match event {
            Event::Text(t) | Event::Code(t) => {
                count += t.split_whitespace().count() as i64;
            }
            _ => {}
        }
    }
    count
}

fn block_kind_from_tag(tag: &Tag) -> Option<BlockType> {
    match tag {
        Tag::Paragraph => Some(BlockType::Paragraph),
        Tag::Heading { .. } => Some(BlockType::Heading),
        Tag::BlockQuote(_) => Some(BlockType::BlockQuote),
        Tag::CodeBlock(_) => Some(BlockType::CodeBlock),
        Tag::List(_) => Some(BlockType::List),
        Tag::HtmlBlock => Some(BlockType::HtmlBlock),
        Tag::FootnoteDefinition(_) => Some(BlockType::FootnoteDefinition),
        Tag::Table(_) => Some(BlockType::Table),
        _ => None,
    }
}

fn block_tag_end_matches(end: &TagEnd) -> bool {
    matches!(
        end,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::FootnoteDefinition
            | TagEnd::Table
    )
}

fn heading_level_to_u32(level: HeadingLevel) -> u32 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Walk the parser at offset granularity, emitting one block for every
/// top-level Markdown element (Paragraph, Heading, BlockQuote, CodeBlock, List,
/// HtmlBlock, Table, FootnoteDefinition).
fn extract_blocks(body: &str) -> Vec<ParsedBlock> {
    let mut blocks: Vec<ParsedBlock> = Vec::new();
    let mut depth: u32 = 0;
    let mut open: Option<(BlockType, usize, u32)> = None; // kind, byte_start, level
    let mut order: u32 = 0;

    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;

    for (event, range) in Parser::new_ext(body, options).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Some(kind) = block_kind_from_tag(&tag) {
                    if depth == 0 && open.is_none() {
                        let level = match &tag {
                            Tag::Heading { level, .. } => heading_level_to_u32(*level),
                            _ => 0,
                        };
                        open = Some((kind, range.start, level));
                    }
                    depth += 1;
                }
            }
            Event::End(end) => {
                if block_tag_end_matches(&end) {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        if let Some((kind, start, level)) = open.take() {
                            let end_byte = range.end.min(body.len());
                            // Trim trailing newlines from the recorded source so
                            // each block sits on its own lines without dragging
                            // along the blank line that separates it from the next.
                            let mut content_end = end_byte;
                            while content_end > start
                                && matches!(
                                    body.as_bytes()[content_end - 1],
                                    b'\n' | b'\r'
                                )
                            {
                                content_end -= 1;
                            }
                            let content = body[start..content_end].to_string();

                            let mut hasher = Sha256::new();
                            hasher.update(content.as_bytes());
                            let content_hash = format!("{:x}", hasher.finalize());

                            let line_number =
                                body[..start].bytes().filter(|&b| b == b'\n').count() as u32;

                            let user_ref = extract_user_ref(&content);

                            blocks.push(ParsedBlock {
                                block_type: kind,
                                level,
                                content,
                                content_hash,
                                byte_start: start,
                                byte_end: content_end,
                                line_number,
                                order_index: order,
                                user_ref,
                            });
                            order += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    blocks
}

/// Pull `^abc123` out of the trailing line of a block if present.
fn extract_user_ref(content: &str) -> Option<String> {
    let last = content.lines().last()?.trim();
    let rest = last.strip_prefix('^')?;
    let id = rest.trim();
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        None
    } else {
        Some(id.to_string())
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
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[0].block_type, BlockType::Heading);
        assert_eq!(doc.blocks[0].level, 1);
        assert_eq!(doc.blocks[1].block_type, BlockType::Paragraph);
    }

    #[test]
    fn falls_back_to_filename_when_no_h1() {
        let doc = parse_document("just a paragraph here", "my-note");
        assert_eq!(doc.title, "my-note");
        assert_eq!(doc.word_count, 4);
        assert_eq!(doc.blocks.len(), 1);
    }

    #[test]
    fn extracts_frontmatter_title() {
        let src = "---\ntitle: My Note\ntags: [a, b]\n---\n\n# Wrong\n\nbody";
        let doc = parse_document(src, "fallback");
        assert_eq!(doc.title, "My Note");
        assert!(doc.frontmatter.as_deref().unwrap().contains("tags:"));
    }

    #[test]
    fn extracts_multiple_top_level_blocks() {
        let src = "# H1\n\npara one\n\n- item a\n- item b\n\n> quote\n\n```rust\nfn x() {}\n```\n";
        let doc = parse_document(src, "x");
        let kinds: Vec<_> = doc.blocks.iter().map(|b| b.block_type).collect();
        assert_eq!(
            kinds,
            vec![
                BlockType::Heading,
                BlockType::Paragraph,
                BlockType::List,
                BlockType::BlockQuote,
                BlockType::CodeBlock,
            ]
        );
        assert_eq!(doc.blocks[0].level, 1);
    }

    #[test]
    fn detects_user_block_ref() {
        let src = "This is a paragraph.\n^abc123\n\nAnother paragraph.";
        let doc = parse_document(src, "x");
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[0].user_ref.as_deref(), Some("abc123"));
        assert!(doc.blocks[1].user_ref.is_none());
    }

    #[test]
    fn outline_lists_only_headings() {
        let src = "# Top\n\nbody\n\n## Sub\n\nmore body\n\n### Sub-sub";
        let outline = parse_outline(src);
        assert_eq!(outline.len(), 3);
        assert_eq!(outline[0].level, 1);
        assert_eq!(outline[0].text, "Top");
        assert_eq!(outline[2].level, 3);
        assert_eq!(outline[2].text, "Sub-sub");
    }
}
