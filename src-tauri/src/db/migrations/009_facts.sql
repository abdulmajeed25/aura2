-- Phase batch step 6: Mem0-style fact memory.
--
-- One row per distinct fact extracted from conversation. The mem0
-- decision engine in `crate::memory::mem0` writes here with ADD,
-- mutates in place on UPDATE, and tombstones via `deleted_at` on
-- DELETE. NOOP leaves both tables untouched.
--
-- We store the embedding inline (BLOB of little-endian f32) instead
-- of going through LanceDB because (a) facts are short and few
-- compared to block embeddings and (b) the libsql table keeps the
-- decision engine self-contained for unit tests.

CREATE TABLE IF NOT EXISTS facts (
    -- UUIDv7 hex (`uuid::Uuid::now_v7().simple()`), monotonic by time.
    id              TEXT PRIMARY KEY,
    -- Canonical statement. Single sentence, present tense, third person.
    text            TEXT NOT NULL,
    -- f32 little-endian; length == 4 * embedding dim. May be NULL on
    -- legacy rows but the writer always populates it.
    embedding       BLOB,
    -- Free-form per-fact tag (e.g. "preference", "biography", "event").
    category        TEXT,
    -- 0.0 .. 1.0. The Anthropic extractor sets this from its own
    -- confidence; the regex stand-in extractor pins it at 1.0.
    confidence      REAL NOT NULL DEFAULT 1.0,
    -- Identifier for the conversation that produced the fact. Lets us
    -- redact every fact from a single session without a full reindex.
    source_session  TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    -- Tombstone. NULL = active. Non-NULL = soft-deleted by a later
    -- DELETE op that contradicts this fact.
    deleted_at      INTEGER
);

CREATE INDEX IF NOT EXISTS idx_facts_updated_at
    ON facts(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_facts_session
    ON facts(source_session);
CREATE INDEX IF NOT EXISTS idx_facts_alive_updated
    ON facts(deleted_at, updated_at DESC);

-- One row per decision. Lets the UI explain why a fact moved.
CREATE TABLE IF NOT EXISTS fact_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Foreign key to facts.id. NOT enforced because a DELETE-then-purge
    -- of facts shouldn't lose the audit trail; we keep history orphaned.
    fact_id     TEXT NOT NULL,
    -- One of: ADD, UPDATE, DELETE, NOOP. CHECK keeps writers honest.
    op          TEXT NOT NULL CHECK (op IN ('ADD','UPDATE','DELETE','NOOP')),
    prev_text   TEXT,
    new_text    TEXT,
    -- Free-form. The decision engine logs the matched neighbour ID
    -- and the cosine similarity that produced the call.
    reason      TEXT,
    timestamp   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_fact_history_fact
    ON fact_history(fact_id);
CREATE INDEX IF NOT EXISTS idx_fact_history_ts
    ON fact_history(timestamp DESC);
