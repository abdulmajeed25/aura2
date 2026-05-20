-- Phase 9: local media file ingestion.

CREATE TABLE IF NOT EXISTS media_files (
    id           TEXT PRIMARY KEY,
    path         TEXT NOT NULL UNIQUE,
    kind         TEXT NOT NULL,
    size_bytes   INTEGER NOT NULL,
    duration_ms  INTEGER,
    description  TEXT NOT NULL,
    embedding    BLOB NOT NULL,
    dim          INTEGER NOT NULL,
    indexed_at   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_media_files_kind ON media_files(kind);
