use serde::{Deserialize, Serialize};

/// A row in the `files` table. Mirrors the Phase 1 schema.
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
