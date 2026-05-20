use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use libsql::{Builder, Connection, Database};

use crate::db::schemas::{
    BacklinkEntry, BlockRow, FileRow, LinkCandidate, MediaRow, OutgoingLinkEntry,
};

/// Embedded migration files. Order matters; they run in array order.
const MIGRATIONS: &[(&str, &str)] = &[
    ("001_initial", include_str!("migrations/001_initial.sql")),
    (
        "002_blocks_links",
        include_str!("migrations/002_blocks_links.sql"),
    ),
    ("003_search", include_str!("migrations/003_search.sql")),
    ("004_hdc", include_str!("migrations/004_hdc.sql")),
    (
        "005_graph_rag",
        include_str!("migrations/005_graph_rag.sql"),
    ),
    ("006_media", include_str!("migrations/006_media.sql")),
    (
        "007_audit_log",
        include_str!("migrations/007_audit_log.sql"),
    ),
    (
        "008_prompt_scores",
        include_str!("migrations/008_prompt_scores.sql"),
    ),
    ("009_facts", include_str!("migrations/009_facts.sql")),
    (
        "010_fhrr_and_telemetry",
        include_str!("migrations/010_fhrr_and_telemetry.sql"),
    ),
];

/// Wrapper around a libsql connection scoped to a single vault.
pub struct VaultDb {
    _db: Database,
    conn: Connection,
}

impl VaultDb {
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
        conn.execute("PRAGMA foreign_keys = ON", ()).await?;

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

    pub async fn delete_file_by_path(&self, path: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM files WHERE path = ?1",
                libsql::params![path.to_string()],
            )
            .await?;
        Ok(())
    }

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

    pub async fn count_files(&self) -> Result<i64> {
        let mut rows = self.conn.query("SELECT COUNT(*) FROM files", ()).await?;
        match rows.next().await? {
            Some(row) => Ok(row.get::<i64>(0)?),
            None => Ok(0),
        }
    }

    // ─── Blocks ───────────────────────────────────────────────────────────

    pub async fn replace_blocks_for_file(
        &self,
        file_id: &str,
        blocks: &[BlockRow],
    ) -> Result<()> {
        // Drop the old blocks AND the corresponding FTS5 rows so the search
        // index doesn't carry stale content.
        self.conn
            .execute(
                "DELETE FROM blocks WHERE file_id = ?1",
                libsql::params![file_id.to_string()],
            )
            .await?;
        // blocks_fts might not exist if migration 003 hasn't run yet (e.g.
        // when a 002-era DB is being upgraded). Tolerate the miss.
        let _ = self
            .conn
            .execute(
                "DELETE FROM blocks_fts WHERE file_id = ?1",
                libsql::params![file_id.to_string()],
            )
            .await;

        for b in blocks {
            self.conn
                .execute(
                    "INSERT INTO blocks (
                        id, file_id, parent_id, order_index, block_type, level,
                        content, content_hash, line_number, user_ref, metadata,
                        created_at, modified_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    libsql::params![
                        b.id.clone(),
                        b.file_id.clone(),
                        b.parent_id.clone(),
                        b.order_index,
                        b.block_type.clone(),
                        b.level,
                        b.content.clone(),
                        b.content_hash.clone(),
                        b.line_number,
                        b.user_ref.clone(),
                        b.metadata.clone(),
                        b.created_at,
                        b.modified_at,
                    ],
                )
                .await?;

            let _ = self
                .conn
                .execute(
                    "INSERT INTO blocks_fts (content, block_id, file_id)
                     VALUES (?1, ?2, ?3)",
                    libsql::params![b.content.clone(), b.id.clone(), b.file_id.clone()],
                )
                .await;
        }
        Ok(())
    }

    /// Insert or replace the embedding rows for a file. `items` is `(block_id,
    /// embedding_bytes, content_hash)`.
    pub async fn replace_block_embeddings_for_file(
        &self,
        file_id: &str,
        items: &[(String, Vec<u8>, String)],
    ) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM block_embeddings WHERE file_id = ?1",
                libsql::params![file_id.to_string()],
            )
            .await?;
        let now = chrono::Utc::now().timestamp_millis();
        for (block_id, bytes, hash) in items {
            self.conn
                .execute(
                    "INSERT INTO block_embeddings
                        (block_id, file_id, dim, embedding, content_hash, indexed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    libsql::params![
                        block_id.clone(),
                        file_id.to_string(),
                        crate::core::embeddings::EMBED_DIM as i64,
                        bytes.clone(),
                        hash.clone(),
                        now,
                    ],
                )
                .await?;
        }
        Ok(())
    }

    /// Stream every block embedding joined with the metadata callers need to
    /// render a search hit.
    pub async fn all_block_embeddings_with_meta(
        &self,
    ) -> Result<Vec<(crate::core::search::BlockMeta, Vec<f32>)>> {
        let mut rows = self
            .conn
            .query(
                "SELECT be.block_id, be.file_id, f.path, f.title,
                        b.block_type, b.line_number, b.content, be.embedding
                 FROM block_embeddings be
                 JOIN blocks b ON b.id = be.block_id
                 JOIN files  f ON f.id = be.file_id",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let bytes: Vec<u8> = row.get(7)?;
            let emb = crate::core::embeddings::bytes_to_embedding(&bytes);
            out.push((
                crate::core::search::BlockMeta {
                    block_id: row.get::<String>(0)?,
                    file_id: row.get::<String>(1)?,
                    file_path: row.get::<String>(2)?,
                    file_title: row.get::<String>(3)?,
                    block_type: row.get::<String>(4)?,
                    line_number: row.get::<i64>(5)?,
                    content: row.get::<String>(6)?,
                },
                emb,
            ));
        }
        Ok(out)
    }

    // ─── HDC (Phase 6) ────────────────────────────────────────────────

    /// Upsert a note's packed text hypervector keyed on `file_id`.
    pub async fn upsert_note_text_hv(
        &self,
        file_id: &str,
        dim: i64,
        packed: &[u8],
        content_hash: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        self.conn
            .execute(
                "INSERT INTO note_text_hvs (file_id, dim, hv_packed, content_hash, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(file_id) DO UPDATE SET
                    dim = excluded.dim,
                    hv_packed = excluded.hv_packed,
                    content_hash = excluded.content_hash,
                    indexed_at = excluded.indexed_at",
                libsql::params![
                    file_id.to_string(),
                    dim,
                    packed.to_vec(),
                    content_hash.to_string(),
                    now,
                ],
            )
            .await?;
        Ok(())
    }

    /// Stream `(file_id, path, title, dim, packed_bytes)` for every note that
    /// has a stored text HV. Used to compute combined HVs on-demand.
    pub async fn all_note_text_hvs(
        &self,
    ) -> Result<Vec<(String, String, String, i64, Vec<u8>)>> {
        let mut rows = self
            .conn
            .query(
                "SELECT h.file_id, f.path, f.title, h.dim, h.hv_packed
                 FROM note_text_hvs h
                 JOIN files f ON f.id = h.file_id",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push((
                row.get::<String>(0)?,
                row.get::<String>(1)?,
                row.get::<String>(2)?,
                row.get::<i64>(3)?,
                row.get::<Vec<u8>>(4)?,
            ));
        }
        Ok(out)
    }

    /// For each note, return (file_id, outgoing_neighbour_ids, incoming_neighbour_ids).
    /// Only resolved links count toward the neighbourhood.
    pub async fn neighbour_map(
        &self,
    ) -> Result<std::collections::HashMap<String, (Vec<String>, Vec<String>)>> {
        let mut rows = self
            .conn
            .query(
                "SELECT source_file_id, target_file_id
                 FROM links
                 WHERE is_resolved = 1 AND target_file_id IS NOT NULL",
                (),
            )
            .await?;
        let mut map: std::collections::HashMap<String, (Vec<String>, Vec<String>)> =
            std::collections::HashMap::new();
        while let Some(row) = rows.next().await? {
            let src: String = row.get(0)?;
            let tgt: String = row.get(1)?;
            map.entry(src.clone()).or_default().0.push(tgt.clone());
            map.entry(tgt).or_default().1.push(src);
        }
        Ok(map)
    }

    // ─── Multimedia (Phase 9) ─────────────────────────────────────────

    pub async fn upsert_media(
        &self,
        row: &MediaRow,
        embedding: &[u8],
        dim: i64,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO media_files (
                    id, path, kind, size_bytes, duration_ms,
                    description, embedding, dim, indexed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(path) DO UPDATE SET
                    kind = excluded.kind,
                    size_bytes = excluded.size_bytes,
                    duration_ms = excluded.duration_ms,
                    description = excluded.description,
                    embedding = excluded.embedding,
                    dim = excluded.dim,
                    indexed_at = excluded.indexed_at",
                libsql::params![
                    row.id.clone(),
                    row.path.clone(),
                    row.kind.clone(),
                    row.size_bytes,
                    row.duration_ms,
                    row.description.clone(),
                    embedding.to_vec(),
                    dim,
                    row.indexed_at,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_media_by_path(&self, path: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM media_files WHERE path = ?1",
                libsql::params![path.to_string()],
            )
            .await?;
        Ok(())
    }

    pub async fn get_media_by_path(&self, path: &str) -> Result<Option<MediaRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, path, kind, size_bytes, duration_ms, description, indexed_at
                 FROM media_files WHERE path = ?1",
                libsql::params![path.to_string()],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            Ok(Some(MediaRow {
                id: row.get(0)?,
                path: row.get(1)?,
                kind: row.get(2)?,
                size_bytes: row.get(3)?,
                duration_ms: row.get(4)?,
                description: row.get(5)?,
                indexed_at: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    // ---------- audit_log writes (Phase 1 of the API-key batch) -----

    /// Append one row to `audit_log`. Public so `ai::audit::DbAuditLogger`
    /// can call it; the API key is never one of the parameters.
    #[allow(clippy::too_many_arguments)]
    pub async fn audit_insert(
        &self,
        timestamp: i64,
        actor: &str,
        operation: &str,
        model: Option<&str>,
        input_tokens: i64,
        cache_creation_input_tokens: i64,
        cache_read_input_tokens: i64,
        output_tokens: i64,
        cost_micro_cents: i64,
        duration_ms: i64,
        status: &str,
        metadata_json: &str,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO audit_log (
                    timestamp, actor, operation, model,
                    input_tokens, cache_creation_input_tokens,
                    cache_read_input_tokens, output_tokens,
                    cost_micro_cents, duration_ms, status, metadata_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                libsql::params![
                    timestamp,
                    actor.to_string(),
                    operation.to_string(),
                    model.map(str::to_string),
                    input_tokens,
                    cache_creation_input_tokens,
                    cache_read_input_tokens,
                    output_tokens,
                    cost_micro_cents,
                    duration_ms,
                    status.to_string(),
                    metadata_json.to_string(),
                ],
            )
            .await?;
        Ok(())
    }

    /// Today's cumulative AI cost in **whole cents** (rounded).
    pub async fn audit_today_cost_cents(&self) -> Result<i64> {
        let day_start = {
            let now = chrono::Utc::now();
            now.date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|n| n.and_utc().timestamp_millis())
                .unwrap_or(0)
        };
        let mut rows = self
            .conn
            .query(
                "SELECT COALESCE(SUM(cost_micro_cents), 0)
                 FROM audit_log
                 WHERE timestamp >= ?1 AND actor IN ('anthropic', 'openai')",
                libsql::params![day_start],
            )
            .await?;
        let micro = rows
            .next()
            .await?
            .map(|r| r.get::<i64>(0).unwrap_or(0))
            .unwrap_or(0);
        Ok(micro / 10_000)
    }

    /// Cache-hit ratio over `window_hours` of anthropic chat calls.
    /// Returns `None` when no calls have happened in the window.
    pub async fn audit_cache_hit_ratio(&self, window_hours: i64) -> Result<Option<f32>> {
        let now = chrono::Utc::now().timestamp_millis();
        let start = now - window_hours * 3_600_000;
        let mut rows = self
            .conn
            .query(
                "SELECT
                    COALESCE(SUM(cache_read_input_tokens), 0),
                    COALESCE(SUM(cache_creation_input_tokens), 0),
                    COALESCE(SUM(input_tokens), 0)
                 FROM audit_log
                 WHERE actor = 'anthropic'
                   AND operation = 'chat'
                   AND timestamp >= ?1",
                libsql::params![start],
            )
            .await?;
        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        let cache_read: i64 = row.get(0).unwrap_or(0);
        let cache_creation: i64 = row.get(1).unwrap_or(0);
        let uncached: i64 = row.get(2).unwrap_or(0);
        let total = cache_read + cache_creation + uncached;
        if total == 0 {
            return Ok(None);
        }
        Ok(Some(cache_read as f32 / total as f32))
    }

    /// Append one row to `prompt_calls` (Phase 6 self-modifier).
    #[allow(clippy::too_many_arguments)]
    pub async fn prompt_call_insert(
        &self,
        timestamp: i64,
        prompt_name: &str,
        prompt_version: i64,
        input_hash: &str,
        score: Option<f64>,
        user_feedback: i64,
        metadata_json: &str,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO prompt_calls
                    (timestamp, prompt_name, prompt_version, input_hash,
                     score, user_feedback, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                libsql::params![
                    timestamp,
                    prompt_name.to_string(),
                    prompt_version,
                    input_hash.to_string(),
                    score,
                    user_feedback,
                    metadata_json.to_string(),
                ],
            )
            .await?;
        Ok(())
    }

    /// Most recent audit_log rows, newest first.
    pub async fn audit_recent(&self, limit: i64) -> Result<Vec<AuditLogRowDb>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, timestamp, actor, operation, model,
                        input_tokens, cache_creation_input_tokens,
                        cache_read_input_tokens, output_tokens,
                        cost_micro_cents, duration_ms, status
                 FROM audit_log
                 ORDER BY timestamp DESC, id DESC
                 LIMIT ?1",
                libsql::params![limit],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(AuditLogRowDb {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                actor: row.get(2)?,
                operation: row.get(3)?,
                model: row.get(4).ok(),
                input_tokens: row.get(5).unwrap_or(0),
                cache_creation_input_tokens: row.get(6).unwrap_or(0),
                cache_read_input_tokens: row.get(7).unwrap_or(0),
                output_tokens: row.get(8).unwrap_or(0),
                cost_micro_cents: row.get(9).unwrap_or(0),
                duration_ms: row.get(10).unwrap_or(0),
                status: row.get(11).unwrap_or_default(),
            });
        }
        Ok(out)
    }

    pub async fn list_media(&self) -> Result<Vec<MediaRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, path, kind, size_bytes, duration_ms, description, indexed_at
                 FROM media_files ORDER BY indexed_at DESC",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(MediaRow {
                id: row.get(0)?,
                path: row.get(1)?,
                kind: row.get(2)?,
                size_bytes: row.get(3)?,
                duration_ms: row.get(4)?,
                description: row.get(5)?,
                indexed_at: row.get(6)?,
            });
        }
        Ok(out)
    }

    pub async fn count_media(&self) -> Result<i64> {
        let mut rows = self
            .conn
            .query("SELECT COUNT(*) FROM media_files", ())
            .await?;
        Ok(rows.next().await?.map(|r| r.get::<i64>(0)).transpose()?.unwrap_or(0))
    }

    /// Stream every media file with its embedding bytes, joined with the
    /// fields needed to render a search hit.
    pub async fn all_media_with_embeddings(
        &self,
    ) -> Result<Vec<(MediaRow, Vec<f32>)>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, path, kind, size_bytes, duration_ms, description,
                        indexed_at, embedding
                 FROM media_files",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let bytes: Vec<u8> = row.get(7)?;
            let emb = crate::core::embeddings::bytes_to_embedding(&bytes);
            out.push((
                MediaRow {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    kind: row.get(2)?,
                    size_bytes: row.get(3)?,
                    duration_ms: row.get(4)?,
                    description: row.get(5)?,
                    indexed_at: row.get(6)?,
                },
                emb,
            ));
        }
        Ok(out)
    }

    // ─── GraphRAG (Phase 7) ───────────────────────────────────────────

    /// Replace the entire community hierarchy for the vault. Inserts
    /// partitions coarsest-first so the `parent_id` foreign key for each
    /// finer-level community resolves to an already-inserted row.
    ///
    /// Atomic in the sense that the old rows are wiped before the new
    /// ones land — readers during a rebuild may see an empty table for
    /// a moment. Hierarchy links use `parent_partition_cid` from the
    /// payload to find the parent's DB id.
    pub async fn replace_communities(
        &self,
        partitions: &[ReplaceCommunity<'_>],
    ) -> Result<()> {
        self.conn.execute("DELETE FROM community_files", ()).await?;
        self.conn.execute("DELETE FROM communities", ()).await?;
        let now = chrono::Utc::now().timestamp_millis();

        // Insert coarsest level first so parent rows always exist before
        // their children. Sort ascending by `-level` (== descending by
        // level), tie-break by partition_cid for determinism.
        let mut ordered: Vec<&ReplaceCommunity<'_>> = partitions.iter().collect();
        ordered.sort_by(|a, b| {
            b.level
                .cmp(&a.level)
                .then_with(|| a.partition_cid.cmp(&b.partition_cid))
        });

        // (level, partition_cid) → DB id, for parent_id lookup.
        let mut id_map: std::collections::HashMap<(i64, u32), i64> =
            std::collections::HashMap::with_capacity(partitions.len());

        for p in ordered {
            let parent_id: Option<i64> = p
                .parent_partition_cid
                .and_then(|pc| id_map.get(&(p.level + 1, pc)).copied());

            let mut rows = self
                .conn
                .query(
                    "INSERT INTO communities
                        (level, parent_id, member_count, summary_text, embedding, dim, indexed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     RETURNING id",
                    libsql::params![
                        p.level,
                        parent_id,
                        p.member_file_ids.len() as i64,
                        p.summary_text.to_string(),
                        p.embedding.to_vec(),
                        p.dim,
                        now,
                    ],
                )
                .await?;
            let community_id: i64 = match rows.next().await? {
                Some(row) => row.get(0)?,
                None => continue,
            };
            id_map.insert((p.level, p.partition_cid), community_id);
            for fid in &p.member_file_ids {
                self.conn
                    .execute(
                        "INSERT INTO community_files (community_id, file_id) VALUES (?1, ?2)",
                        libsql::params![community_id, fid.to_string()],
                    )
                    .await?;
            }
        }
        Ok(())
    }

    /// Read every community at the **coarsest** level (= `MAX(level)` in
    /// the table). After Phase 7a-ii multi-level persistence, the table
    /// contains rows at every Leiden hierarchy level; the GraphRAG query
    /// path wants only the top of the tree by default.
    pub async fn all_communities_with_members(
        &self,
    ) -> Result<Vec<crate::core::graph_rag::query_engine::CommunityRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, level, member_count, summary_text, embedding
                 FROM communities
                 WHERE level = (SELECT MAX(level) FROM communities)
                 ORDER BY id",
                (),
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: i64 = row.get(0)?;
            let level: i64 = row.get(1)?;
            let member_count: i64 = row.get(2)?;
            let summary_text: String = row.get(3)?;
            let embedding: Vec<u8> = row.get(4)?;

            let mut mrows = self
                .conn
                .query(
                    "SELECT f.path, f.title FROM community_files c
                     JOIN files f ON f.id = c.file_id
                     WHERE c.community_id = ?1
                     ORDER BY f.path",
                    libsql::params![id],
                )
                .await?;
            let mut member_paths = Vec::new();
            let mut member_titles = Vec::new();
            while let Some(m) = mrows.next().await? {
                member_paths.push(m.get::<String>(0)?);
                member_titles.push(m.get::<String>(1)?);
            }

            out.push(crate::core::graph_rag::query_engine::CommunityRow {
                id,
                level,
                member_count,
                member_paths,
                member_titles,
                summary_text,
                embedding,
            });
        }
        Ok(out)
    }

    /// Read every community at a specific `level`. Used by the UI's
    /// "zoom" feature in `Phase 7a-ii` — picks one hierarchy level and
    /// reports the partition + summaries at that granularity. Skips
    /// embedding to keep payloads small.
    pub async fn list_communities_at_level(
        &self,
        level: i64,
    ) -> Result<Vec<crate::core::graph_rag::query_engine::CommunityRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, level, member_count, summary_text, embedding
                 FROM communities
                 WHERE level = ?1
                 ORDER BY id",
                libsql::params![level],
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: i64 = row.get(0)?;
            let level: i64 = row.get(1)?;
            let member_count: i64 = row.get(2)?;
            let summary_text: String = row.get(3)?;
            let embedding: Vec<u8> = row.get(4)?;

            let mut mrows = self
                .conn
                .query(
                    "SELECT f.path, f.title FROM community_files c
                     JOIN files f ON f.id = c.file_id
                     WHERE c.community_id = ?1
                     ORDER BY f.path",
                    libsql::params![id],
                )
                .await?;
            let mut member_paths = Vec::new();
            let mut member_titles = Vec::new();
            while let Some(m) = mrows.next().await? {
                member_paths.push(m.get::<String>(0)?);
                member_titles.push(m.get::<String>(1)?);
            }

            out.push(crate::core::graph_rag::query_engine::CommunityRow {
                id,
                level,
                member_count,
                member_paths,
                member_titles,
                summary_text,
                embedding,
            });
        }
        Ok(out)
    }

    /// Highest level present in the communities table — `0` if empty.
    pub async fn max_community_level(&self) -> Result<i64> {
        let mut rows = self
            .conn
            .query("SELECT COALESCE(MAX(level), 0) FROM communities", ())
            .await?;
        Ok(rows
            .next()
            .await?
            .map(|r| r.get::<i64>(0).unwrap_or(0))
            .unwrap_or(0))
    }

    /// FTS5 full-text search returning `(meta, bm25_score)`. Lower bm25 is better.
    pub async fn fts_search(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<(crate::core::search::BlockMeta, f32)>> {
        let mut rows = self
            .conn
            .query(
                "SELECT b.id, b.file_id, f.path, f.title, b.block_type,
                        b.line_number, b.content, bm25(blocks_fts)
                 FROM blocks_fts
                 JOIN blocks b ON b.id = blocks_fts.block_id
                 JOIN files  f ON f.id = b.file_id
                 WHERE blocks_fts MATCH ?1
                 ORDER BY bm25(blocks_fts)
                 LIMIT ?2",
                libsql::params![fts_query_for(query), limit],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push((
                crate::core::search::BlockMeta {
                    block_id: row.get::<String>(0)?,
                    file_id: row.get::<String>(1)?,
                    file_path: row.get::<String>(2)?,
                    file_title: row.get::<String>(3)?,
                    block_type: row.get::<String>(4)?,
                    line_number: row.get::<i64>(5)?,
                    content: row.get::<String>(6)?,
                },
                row.get::<f64>(7)? as f32,
            ));
        }
        Ok(out)
    }

    pub async fn get_block_by_user_ref(
        &self,
        file_id: &str,
        user_ref: &str,
    ) -> Result<Option<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id FROM blocks WHERE file_id = ?1 AND user_ref = ?2 LIMIT 1",
                libsql::params![file_id.to_string(), user_ref.to_string()],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            Ok(Some(row.get::<String>(0)?))
        } else {
            Ok(None)
        }
    }

    // ─── Links ────────────────────────────────────────────────────────────

    pub async fn replace_links_for_file(
        &self,
        source_file_id: &str,
        rows: &[InsertLink],
    ) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM links WHERE source_file_id = ?1",
                libsql::params![source_file_id.to_string()],
            )
            .await?;

        let now = Utc::now().timestamp_millis();
        for l in rows {
            self.conn
                .execute(
                    "INSERT INTO links (
                        source_file_id, source_block_id, target_file_id, target_block_id,
                        target_heading, target_block_ref, link_text, display_text,
                        link_type, line_number, column_number, is_resolved, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    libsql::params![
                        source_file_id.to_string(),
                        l.source_block_id.clone(),
                        l.target_file_id.clone(),
                        l.target_block_id.clone(),
                        l.target_heading.clone(),
                        l.target_block_ref.clone(),
                        l.link_text.clone(),
                        l.display_text.clone(),
                        l.link_type.clone(),
                        l.line_number,
                        l.column_number,
                        if l.target_file_id.is_some() { 1i64 } else { 0i64 },
                        now,
                    ],
                )
                .await?;
        }
        Ok(())
    }

    /// Resolve a `[[target]]` string to a file row by trying, in order:
    /// 1. Exact vault-relative path with `.md`/`.markdown` extension.
    /// 2. Basename match (filename without extension), any folder.
    /// 3. Case-sensitive title match.
    pub async fn resolve_link_target(&self, target: &str) -> Result<Option<FileRow>> {
        if target.is_empty() {
            return Ok(None);
        }

        let lower = target.to_ascii_lowercase();
        let already_md = lower.ends_with(".md") || lower.ends_with(".markdown");

        let candidates: Vec<String> = if already_md {
            vec![target.to_string()]
        } else {
            vec![format!("{}.md", target), format!("{}.markdown", target)]
        };
        for path in &candidates {
            if let Some(row) = self.get_file_by_path(path).await? {
                return Ok(Some(row));
            }
        }

        let basename = target.rsplit('/').next().unwrap_or(target);
        let basename_md = format!("/{}.md", basename);
        let basename_markdown = format!("/{}.markdown", basename);
        let mut rows = self
            .conn
            .query(
                "SELECT id, path, title, content_hash, size_bytes, word_count,
                        created_at, modified_at, indexed_at, frontmatter
                 FROM files
                 WHERE path = ?1 OR path = ?2
                       OR path LIKE ?3 OR path LIKE ?4
                 LIMIT 1",
                libsql::params![
                    format!("{}.md", basename),
                    format!("{}.markdown", basename),
                    format!("%{}", basename_md),
                    format!("%{}", basename_markdown),
                ],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            return Ok(Some(file_row_from(&row)?));
        }

        let mut rows = self
            .conn
            .query(
                "SELECT id, path, title, content_hash, size_bytes, word_count,
                        created_at, modified_at, indexed_at, frontmatter
                 FROM files WHERE title = ?1 LIMIT 1",
                libsql::params![target.to_string()],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            return Ok(Some(file_row_from(&row)?));
        }

        Ok(None)
    }

    /// Backlinks for the file at `target_path`: every link whose target_file_id
    /// points at this file.
    pub async fn get_backlinks(&self, target_path: &str) -> Result<Vec<BacklinkEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT l.source_file_id, sf.path, sf.title, l.link_text,
                        l.display_text, l.line_number, b.content
                 FROM links l
                 JOIN files sf ON sf.id = l.source_file_id
                 JOIN files tf ON tf.id = l.target_file_id
                 LEFT JOIN blocks b ON b.id = l.source_block_id
                 WHERE tf.path = ?1
                 ORDER BY sf.path, l.line_number",
                libsql::params![target_path.to_string()],
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(BacklinkEntry {
                source_file_id: row.get::<String>(0)?,
                source_path: row.get::<String>(1)?,
                source_title: row.get::<String>(2)?,
                link_text: row.get::<String>(3)?,
                display_text: row.get::<Option<String>>(4)?,
                line_number: row.get::<i64>(5)?,
                context: row.get::<Option<String>>(6)?,
            });
        }
        Ok(out)
    }

    /// Outgoing links from the file at `source_path`, joined with target info.
    pub async fn get_outgoing_links(
        &self,
        source_path: &str,
    ) -> Result<Vec<OutgoingLinkEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT l.link_text, l.display_text, tf.path, tf.title,
                        l.target_heading, l.target_block_ref,
                        l.line_number, l.column_number, l.is_resolved
                 FROM links l
                 JOIN files sf ON sf.id = l.source_file_id
                 LEFT JOIN files tf ON tf.id = l.target_file_id
                 WHERE sf.path = ?1
                 ORDER BY l.line_number, l.column_number",
                libsql::params![source_path.to_string()],
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(OutgoingLinkEntry {
                link_text: row.get::<String>(0)?,
                display_text: row.get::<Option<String>>(1)?,
                target_path: row.get::<Option<String>>(2)?,
                target_title: row.get::<Option<String>>(3)?,
                target_heading: row.get::<Option<String>>(4)?,
                target_block_ref: row.get::<Option<String>>(5)?,
                line_number: row.get::<i64>(6)?,
                column_number: row.get::<i64>(7)?,
                is_resolved: row.get::<i64>(8)? != 0,
            });
        }
        Ok(out)
    }

    /// Return `(nodes, edges)` where `nodes` are `(file_id, path, title)` tuples
    /// and `edges` are `(source_file_id, target_file_id)` pairs for every
    /// resolved link in the vault.
    pub async fn fetch_graph_nodes_and_edges(
        &self,
    ) -> Result<(Vec<(String, String, String)>, Vec<(String, String)>)> {
        let mut node_rows = self
            .conn
            .query("SELECT id, path, title FROM files", ())
            .await?;
        let mut nodes = Vec::new();
        while let Some(row) = node_rows.next().await? {
            nodes.push((
                row.get::<String>(0)?,
                row.get::<String>(1)?,
                row.get::<String>(2)?,
            ));
        }

        let mut edge_rows = self
            .conn
            .query(
                "SELECT source_file_id, target_file_id
                 FROM links
                 WHERE is_resolved = 1 AND target_file_id IS NOT NULL",
                (),
            )
            .await?;
        let mut edges = Vec::new();
        while let Some(row) = edge_rows.next().await? {
            edges.push((row.get::<String>(0)?, row.get::<String>(1)?));
        }
        Ok((nodes, edges))
    }

    /// All files in the vault, used as autocomplete candidates for `[[`.
    pub async fn list_link_candidates(&self) -> Result<Vec<LinkCandidate>> {
        let mut rows = self
            .conn
            .query(
                "SELECT path, title FROM files ORDER BY modified_at DESC",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(LinkCandidate {
                path: row.get::<String>(0)?,
                title: row.get::<String>(1)?,
            });
        }
        Ok(out)
    }

    /// After a file is added or renamed, walk through every unresolved link
    /// and try to resolve it against the current `files` table.
    pub async fn reresolve_unresolved_links(&self) -> Result<u32> {
        let mut rows = self
            .conn
            .query(
                "SELECT l.id, l.link_text, l.target_heading, l.target_block_ref
                 FROM links l
                 WHERE l.is_resolved = 0",
                (),
            )
            .await?;
        // Collect first because we'll mutate the table during iteration.
        let mut pending: Vec<(i64, String, Option<String>, Option<String>)> = Vec::new();
        while let Some(row) = rows.next().await? {
            pending.push((
                row.get::<i64>(0)?,
                row.get::<String>(1)?,
                row.get::<Option<String>>(2)?,
                row.get::<Option<String>>(3)?,
            ));
        }

        let mut healed = 0u32;
        for (link_id, link_text, _heading, block_ref) in pending {
            // link_text is the raw `[[...]]`; extract the target portion the
            // same way the parser does.
            let inner = link_text
                .strip_prefix("[[")
                .and_then(|s| s.strip_suffix("]]"))
                .unwrap_or(&link_text);
            let lhs = inner.split('|').next().unwrap_or(inner);
            let file_part = lhs.split('#').next().unwrap_or(lhs).trim();
            let Some(target_file) = self.resolve_link_target(file_part).await? else {
                continue;
            };

            let target_block_id = if let Some(b) = &block_ref {
                self.get_block_by_user_ref(&target_file.id, b).await?
            } else {
                None
            };

            self.conn
                .execute(
                    "UPDATE links
                     SET target_file_id = ?1, target_block_id = ?2, is_resolved = 1
                     WHERE id = ?3",
                    libsql::params![
                        target_file.id.clone(),
                        target_block_id,
                        link_id,
                    ],
                )
                .await?;
            healed += 1;
        }
        Ok(healed)
    }

    // ---- Phase batch step 6: Mem0 fact memory (migration 009) ----

    /// Insert one row into `facts`. Callers (typically
    /// [`FactStore::add`](crate::memory::store::FactStore::add)) own
    /// id allocation so retries against the same logical fact stay
    /// idempotent at the application layer.
    #[allow(clippy::too_many_arguments)]
    pub async fn fact_insert(
        &self,
        id: &str,
        text: &str,
        embedding: &[u8],
        category: Option<&str>,
        confidence: f64,
        source_session: Option<&str>,
        ts_millis: i64,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO facts
                    (id, text, embedding, category, confidence,
                     source_session, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                libsql::params![
                    id.to_string(),
                    text.to_string(),
                    embedding.to_vec(),
                    category.map(|s| s.to_string()),
                    confidence,
                    source_session.map(|s| s.to_string()),
                    ts_millis,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn fact_text(&self, id: &str) -> Result<Option<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT text FROM facts WHERE id = ?1",
                libsql::params![id.to_string()],
            )
            .await?;
        match rows.next().await? {
            Some(r) => Ok(Some(r.get(0)?)),
            None => Ok(None),
        }
    }

    pub async fn fact_update(
        &self,
        id: &str,
        new_text: &str,
        new_embedding: &[u8],
        ts_millis: i64,
    ) -> Result<()> {
        self.conn
            .execute(
                "UPDATE facts
                 SET text = ?2, embedding = ?3, updated_at = ?4
                 WHERE id = ?1",
                libsql::params![
                    id.to_string(),
                    new_text.to_string(),
                    new_embedding.to_vec(),
                    ts_millis,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn fact_soft_delete(&self, id: &str, ts_millis: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE facts SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
                libsql::params![id.to_string(), ts_millis],
            )
            .await?;
        Ok(())
    }

    pub async fn fact_history_insert(
        &self,
        fact_id: &str,
        op: &str,
        prev_text: Option<&str>,
        new_text: Option<&str>,
        reason: Option<&str>,
        ts_millis: i64,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO fact_history
                    (fact_id, op, prev_text, new_text, reason, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                libsql::params![
                    fact_id.to_string(),
                    op.to_string(),
                    prev_text.map(|s| s.to_string()),
                    new_text.map(|s| s.to_string()),
                    reason.map(|s| s.to_string()),
                    ts_millis,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn fact_list_active(&self) -> Result<Vec<FactRowDb>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, text, embedding, category, confidence,
                        source_session, created_at, updated_at
                 FROM facts
                 WHERE deleted_at IS NULL
                 ORDER BY updated_at DESC",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(FactRowDb {
                id: row.get(0)?,
                text: row.get(1)?,
                embedding: row.get::<Vec<u8>>(2)?,
                category: row.get(3).ok(),
                confidence: row.get(4)?,
                source_session: row.get(5).ok(),
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            });
        }
        Ok(out)
    }

    pub async fn fact_history(
        &self,
        fact_id: &str,
    ) -> Result<Vec<crate::memory::store::FactHistoryEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT fact_id, op, prev_text, new_text, reason, timestamp
                 FROM fact_history
                 WHERE fact_id = ?1
                 ORDER BY timestamp ASC, id ASC",
                libsql::params![fact_id.to_string()],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(crate::memory::store::FactHistoryEntry {
                fact_id: row.get(0)?,
                op: row.get(1)?,
                prev_text: row.get(2).ok(),
                new_text: row.get(3).ok(),
                reason: row.get(4).ok(),
                timestamp: row.get(5)?,
            });
        }
        Ok(out)
    }
}

/// Row shape returned by [`VaultDb::fact_list_active`]. Lives here
/// rather than in `schemas.rs` because nothing outside the memory
/// module consumes it.
#[derive(Debug, Clone)]
pub struct FactRowDb {
    pub id: String,
    pub text: String,
    pub embedding: Vec<u8>,
    pub category: Option<String>,
    pub confidence: f64,
    pub source_session: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// ---- Phase batch step 5/6 ----

/// One row returned by [`VaultDb::audit_recent`]. The `metadata_json`
/// field is **not** projected into this struct — telemetry UIs don't
/// need it, and the per-call metadata is the only place a stray prompt
/// could land.
#[derive(Debug, Clone)]
pub struct AuditLogRowDb {
    pub id: i64,
    pub timestamp: i64,
    pub actor: String,
    pub operation: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    pub cost_micro_cents: i64,
    pub duration_ms: i64,
    pub status: String,
}

/// Payload for [`VaultDb::replace_communities`].
///
/// `partition_cid` is the community's id *within* its `level` (as
/// returned by the Leiden partition map). `parent_partition_cid`, if
/// present, is the cid in `level + 1` (one step coarser) that this
/// community is a sub-piece of — used to populate the `parent_id`
/// foreign key. The coarsest level always has
/// `parent_partition_cid = None`.
pub struct ReplaceCommunity<'a> {
    pub level: i64,
    pub partition_cid: u32,
    pub parent_partition_cid: Option<u32>,
    pub member_file_ids: Vec<&'a str>,
    pub summary_text: &'a str,
    pub embedding: &'a [u8],
    pub dim: i64,
}

/// Insert payload for a single link, used by [`VaultDb::replace_links_for_file`].
pub struct InsertLink {
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
}

/// Build an FTS5 MATCH expression from a free-text user query: each
/// alphanumeric token is double-quoted and prefix-matched, joined by implicit AND.
fn fts_query_for(q: &str) -> String {
    q.split_whitespace()
        .map(|t| {
            t.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .map(|s| format!("\"{}\"*", s))
        .collect::<Vec<_>>()
        .join(" ")
}

fn file_row_from(row: &libsql::Row) -> Result<FileRow> {
    Ok(FileRow {
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
    })
}
