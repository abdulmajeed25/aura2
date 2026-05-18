use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};

use crate::core::link_resolver::scan_wiki_links;
use crate::core::markdown_parser::parse_document;
use crate::db::schemas::{BlockRow, FileRow};
use crate::db::sqlite::{InsertLink, VaultDb};
use crate::utils::error::AuraError;

/// In-memory representation of a single opened vault.
pub struct VaultState {
    pub root: PathBuf,
    pub db: Arc<VaultDb>,
}

impl VaultState {
    pub async fn open(root: PathBuf) -> Result<Self> {
        let root = root
            .canonicalize()
            .with_context(|| format!("canonicalize vault root {}", root.display()))?;
        if !root.is_dir() {
            return Err(anyhow!("vault path is not a directory: {}", root.display()));
        }
        let db_path = root.join(".aura").join("aura.db");
        let db = VaultDb::open(&db_path).await?;
        Ok(Self {
            root,
            db: Arc::new(db),
        })
    }

    /// Validate a vault-relative path and return its absolute form.
    /// Absolute paths and any `..` components are rejected.
    pub fn resolve(&self, rel: &str) -> Result<PathBuf, AuraError> {
        if rel.is_empty() {
            return Err(AuraError::InvalidPath(rel.to_string()));
        }
        let p = Path::new(rel);
        if p.is_absolute() || rel.starts_with('/') || rel.starts_with('\\') {
            return Err(AuraError::PathOutsideVault(rel.to_string()));
        }
        for comp in p.components() {
            use std::path::Component::*;
            if matches!(comp, ParentDir | RootDir | Prefix(_)) {
                return Err(AuraError::PathOutsideVault(rel.to_string()));
            }
        }
        Ok(self.root.join(rel))
    }

    pub fn relativize(&self, abs: &Path) -> Result<String, AuraError> {
        let rel = abs
            .strip_prefix(&self.root)
            .map_err(|_| AuraError::PathOutsideVault(abs.display().to_string()))?;
        Ok(rel
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_string_lossy().to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/"))
    }

    /// Walk the vault and index every `.md` / `.markdown` file. After all files
    /// have been indexed once, attempt to resolve any links that landed
    /// unresolved (e.g. because the target was indexed after the source).
    pub async fn reindex(&self) -> Result<ReindexReport> {
        let mut indexed = 0u32;
        let mut skipped = 0u32;

        let walker = WalkBuilder::new(&self.root)
            .hidden(false)
            .git_ignore(true)
            .git_exclude(true)
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !(name == ".aura" || name == ".git" || name == "node_modules")
            })
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            match path.extension().and_then(|s| s.to_str()) {
                Some("md") | Some("markdown") => {}
                _ => continue,
            }

            match self.index_one(path).await {
                Ok(_) => indexed += 1,
                Err(e) => {
                    tracing::warn!(target: "aura::index", "skipping {}: {}", path.display(), e);
                    skipped += 1;
                }
            }
        }

        let healed = self.db.reresolve_unresolved_links().await.unwrap_or(0);
        if healed > 0 {
            tracing::info!(target: "aura::index", "healed {} previously unresolved links", healed);
        }

        Ok(ReindexReport { indexed, skipped })
    }

    /// Index a single file by absolute path. Updates the `files`, `blocks`,
    /// and `links` tables. Healing of unresolved links elsewhere in the vault
    /// is attempted afterwards.
    pub async fn index_one(&self, abs_path: &Path) -> Result<()> {
        let rel = self
            .relativize(abs_path)
            .map_err(|e| anyhow!(e.to_string()))?;
        let metadata = std::fs::metadata(abs_path)?;
        let content = std::fs::read_to_string(abs_path)?;

        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        let fallback_title = abs_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let parsed = parse_document(&content, &fallback_title);
        let now = Utc::now().timestamp_millis();

        let existing = self.db.get_file_by_path(&rel).await?;
        let file_id = existing
            .as_ref()
            .map(|r| r.id.clone())
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        let created_at = existing.as_ref().map(|r| r.created_at).unwrap_or(now);

        let file_row = FileRow {
            id: file_id.clone(),
            path: rel.clone(),
            title: parsed.title.clone(),
            content_hash,
            size_bytes: metadata.len() as i64,
            word_count: parsed.word_count,
            created_at,
            modified_at: now,
            indexed_at: Some(now),
            frontmatter: parsed.frontmatter,
        };
        self.db.upsert_file(&file_row).await?;

        let blocks: Vec<BlockRow> = parsed
            .blocks
            .iter()
            .map(|b| BlockRow {
                id: uuid::Uuid::now_v7().to_string(),
                file_id: file_id.clone(),
                parent_id: None,
                order_index: b.order_index as i64,
                block_type: b.block_type.as_str().to_string(),
                level: b.level as i64,
                content: b.content.clone(),
                content_hash: b.content_hash.clone(),
                line_number: b.line_number as i64,
                user_ref: b.user_ref.clone(),
                metadata: None,
                created_at: now,
                modified_at: now,
            })
            .collect();
        self.db.replace_blocks_for_file(&file_id, &blocks).await?;

        // Build map from line_number → block_id for source_block_id resolution.
        let mut block_id_by_first_line: Vec<(u32, String)> = parsed
            .blocks
            .iter()
            .zip(blocks.iter())
            .map(|(p, r)| (p.line_number, r.id.clone()))
            .collect();
        block_id_by_first_line.sort_by_key(|(line, _)| *line);

        let raw_links = scan_wiki_links(&content);
        let mut to_insert: Vec<InsertLink> = Vec::with_capacity(raw_links.len());
        for link in raw_links {
            let source_block_id = block_for_line(&block_id_by_first_line, link.line);
            let (target_file_id, target_block_id) =
                self.resolve_target(&link.target, link.block_ref.as_deref()).await?;
            let link_type = if link.block_ref.is_some() {
                "wiki_block".to_string()
            } else if link.heading.is_some() {
                "wiki_heading".to_string()
            } else {
                "wiki".to_string()
            };
            to_insert.push(InsertLink {
                source_block_id,
                target_file_id,
                target_block_id,
                target_heading: link.heading,
                target_block_ref: link.block_ref,
                link_text: link.raw,
                display_text: link.display,
                link_type,
                line_number: link.line as i64,
                column_number: link.column as i64,
            });
        }
        self.db.replace_links_for_file(&file_id, &to_insert).await?;

        // A newly-indexed file may resolve previously-unresolved links elsewhere.
        let _ = self.db.reresolve_unresolved_links().await;

        Ok(())
    }

    async fn resolve_target(
        &self,
        target: &str,
        block_ref: Option<&str>,
    ) -> Result<(Option<String>, Option<String>)> {
        let Some(target_file) = self.db.resolve_link_target(target).await? else {
            return Ok((None, None));
        };
        let block_id = if let Some(b) = block_ref {
            self.db
                .get_block_by_user_ref(&target_file.id, b)
                .await
                .unwrap_or(None)
        } else {
            None
        };
        Ok((Some(target_file.id), block_id))
    }
}

fn block_for_line(map: &[(u32, String)], line: u32) -> Option<String> {
    let mut last: Option<&String> = None;
    for (start, id) in map {
        if *start <= line {
            last = Some(id);
        } else {
            break;
        }
    }
    last.cloned()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReindexReport {
    pub indexed: u32,
    pub skipped: u32,
}
