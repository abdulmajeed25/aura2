-- Phase batch step 4 (offline subset): FHRR storage + cognitive
-- telemetry index.
--
-- The v5.0 spec (PART 5.2) reserves LanceDB columns `fhrr_real` +
-- `fhrr_imag` alongside the bipolar HV. Aura's vector store is
-- libsql today, so the FHRR side-by-side representation lives in a
-- dedicated table keyed by file_id (one-FHRR-per-note).
--
-- The same migration adds an index on `audit_log` for the cognitive
-- telemetry actor so the day-30 review can scan Hamiltonian / FHRR
-- rows without a full table sweep.

CREATE TABLE IF NOT EXISTS fhrr_vectors (
    -- File the FHRR vector represents. One row per note; if the note
    -- is deleted, the FHRR row goes with it.
    file_id     TEXT PRIMARY KEY,
    -- Real and imaginary parts, each packed as f32 little-endian
    -- bytes. Same dim as the bipolar HV (configured per-vault; we
    -- store the dim explicitly so a future double-dim experiment
    -- doesn't need a schema change).
    dim         INTEGER NOT NULL,
    fhrr_real   BLOB NOT NULL,
    fhrr_imag   BLOB NOT NULL,
    -- Bookkeeping
    encoder_ver TEXT,
    indexed_at  INTEGER NOT NULL,
    -- The FFT-binding seed used so unbind is deterministic given the
    -- same key vectors. NULL = uses the default seed.
    seed        INTEGER
);

-- Optional cross-link to blocks for block-level FHRR experiments.
CREATE TABLE IF NOT EXISTS fhrr_block_vectors (
    block_id    TEXT PRIMARY KEY,
    dim         INTEGER NOT NULL,
    fhrr_real   BLOB NOT NULL,
    fhrr_imag   BLOB NOT NULL,
    indexed_at  INTEGER NOT NULL
);

-- Cognitive telemetry — already lands in `audit_log` with
-- actor='cortex', but this index lets the day-30 review pull
-- only the telemetry rows efficiently. The expression-index form
-- (actor + operation prefix) is a libsql-supported partial index.
CREATE INDEX IF NOT EXISTS idx_audit_log_cortex
    ON audit_log(timestamp DESC)
    WHERE actor = 'cortex';
