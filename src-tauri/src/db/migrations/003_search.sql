-- Phase 5: semantic + full-text search.

CREATE TABLE IF NOT EXISTS block_embeddings (
    block_id     TEXT PRIMARY KEY,
    file_id      TEXT NOT NULL,
    dim          INTEGER NOT NULL,
    embedding    BLOB NOT NULL,
    content_hash TEXT NOT NULL,
    indexed_at   INTEGER NOT NULL,
    FOREIGN KEY (block_id) REFERENCES blocks(id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_block_embeddings_file ON block_embeddings(file_id);

-- Full-text search over block content. FTS5 ships with SQLite/libsql.
CREATE VIRTUAL TABLE IF NOT EXISTS blocks_fts USING fts5(
    content,
    block_id UNINDEXED,
    file_id UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);
