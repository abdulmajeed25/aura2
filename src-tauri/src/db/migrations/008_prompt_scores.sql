-- Phase batch step 6: prompt-call outcomes table for the
-- DSPy-style self-modifier.
--
-- One row per AI call that came through a versioned prompt template.
-- Aggregations (mean score, sample size, Wilson interval) live in
-- `crate::reasoning::self_modifier::scoring`.

CREATE TABLE IF NOT EXISTS prompt_calls (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp       INTEGER NOT NULL,
    prompt_name     TEXT NOT NULL,
    prompt_version  INTEGER NOT NULL,
    -- SHA-256 hex of the user input. Lets us replay deterministically
    -- without storing the raw prompt content here (that lives in
    -- `<vault>/.aura/prompts/<name>/v<n>.md` on disk).
    input_hash      TEXT NOT NULL,
    -- 0.0 .. 1.0; higher = better. NULL = not scored yet.
    score           REAL,
    -- 0 = neutral, 1 = accepted, -1 = dismissed (UI feedback).
    user_feedback   INTEGER DEFAULT 0,
    -- Free-form per-call metadata (energy_delta, output_length, …).
    metadata_json   TEXT
);

CREATE INDEX IF NOT EXISTS idx_prompt_calls_name_version
    ON prompt_calls(prompt_name, prompt_version);
CREATE INDEX IF NOT EXISTS idx_prompt_calls_timestamp
    ON prompt_calls(timestamp DESC);
