-- Phase 6: HDC text hypervectors per note (10,000-bit packed).

CREATE TABLE IF NOT EXISTS note_text_hvs (
    file_id      TEXT PRIMARY KEY,
    dim          INTEGER NOT NULL,
    hv_packed    BLOB NOT NULL,
    content_hash TEXT NOT NULL,
    indexed_at   INTEGER NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);
