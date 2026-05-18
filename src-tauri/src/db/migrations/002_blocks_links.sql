-- Phase 2: block-level addressability and bi-directional links.

CREATE TABLE IF NOT EXISTS blocks (
    id              TEXT PRIMARY KEY,
    file_id         TEXT NOT NULL,
    parent_id       TEXT,
    order_index     INTEGER NOT NULL,
    block_type      TEXT NOT NULL,
    level           INTEGER NOT NULL DEFAULT 0,
    content         TEXT NOT NULL,
    content_hash    TEXT NOT NULL,
    line_number     INTEGER NOT NULL DEFAULT 0,
    user_ref        TEXT,
    metadata        TEXT,
    created_at      INTEGER NOT NULL,
    modified_at     INTEGER NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES blocks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_blocks_file ON blocks(file_id, order_index);
CREATE INDEX IF NOT EXISTS idx_blocks_type ON blocks(block_type);
CREATE INDEX IF NOT EXISTS idx_blocks_user_ref ON blocks(user_ref);

CREATE TABLE IF NOT EXISTS links (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    source_file_id     TEXT NOT NULL,
    source_block_id    TEXT,
    target_file_id     TEXT,
    target_block_id    TEXT,
    target_heading     TEXT,
    target_block_ref   TEXT,
    link_text          TEXT NOT NULL,
    display_text       TEXT,
    link_type          TEXT NOT NULL,
    line_number        INTEGER NOT NULL DEFAULT 0,
    column_number      INTEGER NOT NULL DEFAULT 0,
    is_resolved        INTEGER NOT NULL DEFAULT 0,
    created_at         INTEGER NOT NULL,
    FOREIGN KEY (source_file_id) REFERENCES files(id) ON DELETE CASCADE,
    FOREIGN KEY (target_file_id) REFERENCES files(id) ON DELETE SET NULL,
    FOREIGN KEY (source_block_id) REFERENCES blocks(id) ON DELETE SET NULL,
    FOREIGN KEY (target_block_id) REFERENCES blocks(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_file_id);
CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_file_id);
CREATE INDEX IF NOT EXISTS idx_links_unresolved ON links(is_resolved) WHERE is_resolved = 0;
