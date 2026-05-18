use serde::{Deserialize, Serialize};

/// A row in the `files` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRow {
    pub id: String,
    pub path: String,
    pub title: String,
    pub content_hash: String,
    pub size_bytes: i64,
    pub word_count: i64,
    pub created_at: i64,
    pub modified_at: i64,
    pub indexed_at: Option<i64>,
    pub frontmatter: Option<String>,
}

/// A row in the `blocks` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRow {
    pub id: String,
    pub file_id: String,
    pub parent_id: Option<String>,
    pub order_index: i64,
    pub block_type: String,
    pub level: i64,
    pub content: String,
    pub content_hash: String,
    pub line_number: i64,
    pub user_ref: Option<String>,
    pub metadata: Option<String>,
    pub created_at: i64,
    pub modified_at: i64,
}

/// A row in the `links` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkRow {
    pub id: i64,
    pub source_file_id: String,
    pub source_block_id: Option<String>,
    pub target_file_id: Option<String>,
    pub target_block_id: Option<String>,
    pub target_heading: Option<String>,
    pub target_block_ref: Option<String>,
    pub link_text: String,
    pub display_text: Option<String>,
    pub link_type: String,
    pub line_number: i64,
    pub column_number: i64,
    pub is_resolved: i64,
    pub created_at: i64,
}

/// Wire-format backlink entry rendered in the sidebar.
#[derive(Debug, Clone, Serialize)]
pub struct BacklinkEntry {
    pub source_file_id: String,
    pub source_path: String,
    pub source_title: String,
    pub link_text: String,
    pub display_text: Option<String>,
    pub line_number: i64,
    pub context: Option<String>,
}

/// Wire-format outgoing link rendered in the references panel.
#[derive(Debug, Clone, Serialize)]
pub struct OutgoingLinkEntry {
    pub link_text: String,
    pub display_text: Option<String>,
    pub target_path: Option<String>,
    pub target_title: Option<String>,
    pub target_heading: Option<String>,
    pub target_block_ref: Option<String>,
    pub line_number: i64,
    pub column_number: i64,
    pub is_resolved: bool,
}

/// Suggestion entry used by the `[[` autocomplete.
#[derive(Debug, Clone, Serialize)]
pub struct LinkCandidate {
    pub path: String,
    pub title: String,
}

/// A row in the `media_files` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaRow {
    pub id: String,
    pub path: String,
    pub kind: String,
    pub size_bytes: i64,
    pub duration_ms: Option<i64>,
    pub description: String,
    pub indexed_at: i64,
}
