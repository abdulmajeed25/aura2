use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};

use crate::core::markdown_parser::parse_document;
use crate::db::schemas::FileRow;
use crate::db::sqlite::VaultDb;
use crate::utils::error::AuraError;

/// In-memory representation of a single opened vault.
pub struct VaultState {
    pub root: PathBuf,
    pub db: Arc<VaultDb>,
}

impl VaultState {
    /// Opens a vault rooted at `root`, initialising the SQLite database under `<root>/.aura/aura.db`.
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

    /// Convert an absolute path back into its vault-relative string form using `/` separators.
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

    /// Walk the vault and index every `.md` / `.markdown` file. Ignores `.git`, `.aura`,
    /// `node_modules`, and anything matched by `.gitignore`.
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
        Ok(ReindexReport { indexed, skipped })
    }

    /// Index a single file by absolute path (no-op if it lies outside the vault).
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
        let id = existing
            .as_ref()
            .map(|r| r.id.clone())
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        let created_at = existing.as_ref().map(|r| r.created_at).unwrap_or(now);

        let row = FileRow {
            id,
            path: rel,
            title: parsed.title,
            content_hash,
            size_bytes: metadata.len() as i64,
            word_count: parsed.word_count,
            created_at,
            modified_at: now,
            indexed_at: Some(now),
            frontmatter: parsed.frontmatter,
        };
        self.db.upsert_file(&row).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReindexReport {
    pub indexed: u32,
    pub skipped: u32,
}
