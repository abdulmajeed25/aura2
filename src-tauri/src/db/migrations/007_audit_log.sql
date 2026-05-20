-- Phase 1 of the API-key batch: audit log for AI provider calls.
-- Used by:
-- - `crate::ai::audit::DbAuditLogger` to persist usage + cost per
--   Anthropic call.
-- - The Settings → AI → Budget UI to compute today's cost.
-- - The cache-hit-ratio status badge.
--
-- Schema is provider-agnostic so other backends (OpenAI, Ollama, local
-- ONNX embedder) can write rows with the same shape.

CREATE TABLE IF NOT EXISTS audit_log (
    id                              INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp                       INTEGER NOT NULL,
    actor                           TEXT NOT NULL,    -- 'anthropic', 'user', 'agent', …
    operation                       TEXT NOT NULL,    -- 'chat', 'embed', 'summarize', …
    model                           TEXT,
    input_tokens                    INTEGER DEFAULT 0,
    cache_creation_input_tokens     INTEGER DEFAULT 0,
    cache_read_input_tokens         INTEGER DEFAULT 0,
    output_tokens                   INTEGER DEFAULT 0,
    -- Cost stored in hundredths of a cent (so 100 = 1 cent, no f64).
    cost_micro_cents                INTEGER DEFAULT 0,
    duration_ms                     INTEGER DEFAULT 0,
    status                          TEXT NOT NULL,    -- 'ok' | 'rate_limited' | 'error'
    -- Free-form per-row metadata (JSON). Never includes the API key.
    metadata_json                   TEXT
);

CREATE INDEX IF NOT EXISTS idx_audit_log_timestamp ON audit_log(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_actor_operation ON audit_log(actor, operation);
