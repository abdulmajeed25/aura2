-- Phase 7: GraphRAG community summaries.

CREATE TABLE IF NOT EXISTS communities (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    level         INTEGER NOT NULL DEFAULT 0,
    parent_id     INTEGER,
    member_count  INTEGER NOT NULL,
    summary_text  TEXT NOT NULL,
    embedding     BLOB NOT NULL,
    dim           INTEGER NOT NULL,
    indexed_at    INTEGER NOT NULL,
    FOREIGN KEY (parent_id) REFERENCES communities(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS community_files (
    community_id  INTEGER NOT NULL,
    file_id       TEXT NOT NULL,
    PRIMARY KEY (community_id, file_id),
    FOREIGN KEY (community_id) REFERENCES communities(id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_community_files_file ON community_files(file_id);
