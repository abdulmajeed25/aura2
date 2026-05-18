use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use libsql::{Builder, Connection, Database};

use crate::db::schemas::FileRow;

/// Embedded migration files. Order matters; they run in array order.
const MIGRATIONS: &[(&str, &str)] = &[("001_initial", include_str!("migrations/001_initial.sql"))];

/// Wrapper around a libsql connection scoped to a single vault.
pub struct VaultDb {
    _db: Database,
    conn: Connection,
}

impl VaultDb {
    /// Open or create the on-disk SQLite database living inside the vault's `.aura/` folder.
    pub async fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating db dir {}", parent.display()))?;
        }
        let db = Builder::new_local(db_path)
            .build()
            .await
            .with_context(|| format!("opening libsql db at {}", db_path.display()))?;
        let conn = db.connect()?;

        let mut this = Self { _db: db, conn };
        this.run_migrations().await?;
        Ok(this)
    }

    async fn run_migrations(&mut self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS _aura_migrations (
                    name TEXT PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );",
            )
            .await?;

        for (name, sql) in MIGRATIONS {
            let mut rows = self
                .conn
                .query(
                    "SELECT 1 FROM _aura_migrations WHERE name = ?1",
                    libsql::params![*name],
                )
                .await?;
            if rows.next().await?.is_some() {
                continue;
            }

            tracing::info!(target: "aura::db", "applying migration {}", name);
            self.conn.execute_batch(sql).await?;
            let now = Utc::now().timestamp_millis();
            self.conn
                .execute(
                    "INSERT INTO _aura_migrations (name, applied_at) VALUES (?1, ?2)",
                    libsql::params![*name, now],
                )
                .await?;
        }
        Ok(())
    }

    /// Upsert a file row keyed on its vault-relative path.
    pub async fn upsert_file(&self, row: &FileRow) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO files (
                    id, path, title, content_hash, size_bytes, word_count,
                    created_at, modified_at, indexed_at, frontmatter
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(path) DO UPDATE SET
                    title = excluded.title,
                    content_hash = excluded.content_hash,
                    size_bytes = excluded.size_bytes,
                    word_count = excluded.word_count,
                    modified_at = excluded.modified_at,
                    indexed_at = excluded.indexed_at,
                    frontmatter = excluded.frontmatter",
                libsql::params![
                    row.id.clone(),
                    row.path.clone(),
                    row.title.clone(),
                    row.content_hash.clone(),
                    row.size_bytes,
                    row.word_count,
                    row.created_at,
                    row.modified_at,
                    row.indexed_at,
                    row.frontmatter.clone(),
                ],
            )
            .await?;
        Ok(())
    }

    /// Remove a file row by vault-relative path.
    pub async fn delete_file_by_path(&self, path: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM files WHERE path = ?1",
                libsql::params![path.to_string()],
            )
            .await?;
        Ok(())
    }

    /// Look up a file row by vault-relative path.
    pub async fn get_file_by_path(&self, path: &str) -> Result<Option<FileRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, path, title, content_hash, size_bytes, word_count,
                        created_at, modified_at, indexed_at, frontmatter
                 FROM files WHERE path = ?1",
                libsql::params![path.to_string()],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            Ok(Some(FileRow {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2)?,
                content_hash: row.get(3)?,
                size_bytes: row.get(4)?,
                word_count: row.get(5)?,
                created_at: row.get(6)?,
                modified_at: row.get(7)?,
                indexed_at: row.get(8)?,
                frontmatter: row.get(9)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Count indexed files. Used in basic status queries.
    pub async fn count_files(&self) -> Result<i64> {
        let mut rows = self.conn.query("SELECT COUNT(*) FROM files", ()).await?;
        match rows.next().await? {
            Some(row) => Ok(row.get::<i64>(0)?),
            None => Ok(0),
        }
    }
}
