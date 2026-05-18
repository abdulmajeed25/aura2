-- Initial Aura schema.

CREATE TABLE IF NOT EXISTS files (
    id              TEXT PRIMARY KEY,
    path            TEXT NOT NULL UNIQUE,
    title           TEXT NOT NULL,
    content_hash    TEXT NOT NULL,
    size_bytes      INTEGER NOT NULL,
    word_count      INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL,
    modified_at     INTEGER NOT NULL,
    indexed_at      INTEGER,
    frontmatter     TEXT
);

CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
CREATE INDEX IF NOT EXISTS idx_files_modified ON files(modified_at DESC);

-- Bookkeeping table used by Aura to track which migration files have been applied.
CREATE TABLE IF NOT EXISTS _aura_migrations (
    name        TEXT PRIMARY KEY,
    applied_at  INTEGER NOT NULL
);
