# Hamiltonian + FHRR Telemetry — Day 0

Started 2026-05-20 as part of the autonomous-batch offline subset.
Per the v5.0 spec (PART 10.3, PART 12), the Hamiltonian fusion layer
and the FHRR holographic-memory branch are research-grade constructs
that ship behind telemetry. The decision rule was: build + instrument +
**measure whether they improve any downstream metric** over a 30-day
window. If they don't, strip them and keep the simpler CAN + LSM +
Langevin core.

Day 0 marks the window start. Day 30 review: **2026-06-19**.

## What got instrumented in this batch

### 1. Pattern-conditioned `∇V` (`cognition::hamiltonian`)

The kernel was already in code (`leapfrog_step` + `energy`, math-TDD
tested). This batch added the two missing helpers the spec called for:

- `pattern_grad(x, targets) → Vec<f32>`  — produces `∇V(x) = Σⱼ wⱼ · (x − targetⱼ)`
- `pattern_energy(x, targets) → f32`     — its integrated form `V(x) = Σⱼ ½ wⱼ ‖x − targetⱼ‖²`

These are the pieces that turn the leapfrog from "free-particle"
(`grad_v = 0`) into "trajectory bent by the user's documented
patterns." A future workflow's `trigger_vector` plugs in as one of the
`targets`; multiple workflows superpose.

Math-TDD tests (4 new):

- gradient is zero at the basin floor
- gradient points back toward the target with correct magnitude
- multi-pattern superposition is linear
- finite-difference of `pattern_energy` matches `pattern_grad` to ≤ 1e-2

### 2. FHRR side-by-side storage (migration `010_fhrr_and_telemetry.sql`)

New libsql tables:

- `fhrr_vectors(file_id PK, dim, fhrr_real BLOB, fhrr_imag BLOB, encoder_ver, indexed_at, seed)`
- `fhrr_block_vectors(block_id PK, dim, fhrr_real BLOB, fhrr_imag BLOB, indexed_at)`

The spec called for LanceDB columns; Aura's vector store is libsql, so
the side-by-side representation lands as dedicated tables keyed by
file/block id. Same f32-LE byte format the bipolar HV blob uses
elsewhere — no new encoder/decoder shape.

### 3. Telemetry sink (`cognition::telemetry`)

Two helpers write into the existing `audit_log` table with
`actor = 'cortex'`:

| Operation          | metadata_json keys                                           |
|--------------------|--------------------------------------------------------------|
| `hamiltonian_step` | `energy_before`, `energy_after`, `energy_delta`, `state_dim`, `dt`, `grad_norm`, `momentum_norm_after` |
| `fhrr_compare`     | `bipolar_similarity`, `fhrr_similarity`, `delta`, `corpus_size` |

Reusing `audit_log` (rather than a dedicated table) is deliberate: the
day-30 review joins cortex rows against Anthropic-call rows from the
same table to compare cortex activity against LLM spend. New partial
index `idx_audit_log_cortex` scopes the lookup so the day-30 query
doesn't sweep the full audit log.

Both helpers are best-effort — failures `tracing::warn!` but never
propagate into the perpetual-loop tick path.

## What's NOT in this batch

- **Wire into `Cortex::tick`.** The pattern-conditioned grad + telemetry
  writes still need a caller in `cognition::perpetual_loop` for them to
  actually fire. That's a 1-file change once the cortex picks an active
  workflow vector to bias toward — the seam is `Cortex::step_hamiltonian`,
  which already accepts a `grad_v` closure. Trivial wiring, intentionally
  deferred so this Day-0 commit lands self-contained.
- **The Anthropic-narrated reflection body.** With no credits on the
  account, the LLM-narrated reflection synthesis stays in
  `reflection_writer`'s template-only mode. Telemetry records still
  land — the cortex thinks; the narration is silent.

## Day-30 review query

```sql
-- Hamiltonian energy drift distribution
SELECT
    json_extract(metadata_json, '$.energy_delta') AS de,
    COUNT(*) AS n
FROM audit_log
WHERE actor = 'cortex' AND operation = 'hamiltonian_step'
GROUP BY ROUND(de, 4)
ORDER BY de;

-- FHRR vs bipolar retrieval similarity, sliding window
SELECT
    DATE(timestamp / 1000, 'unixepoch') AS day,
    AVG(json_extract(metadata_json, '$.delta')) AS mean_delta,
    COUNT(*) AS n
FROM audit_log
WHERE actor = 'cortex' AND operation = 'fhrr_compare'
GROUP BY day
ORDER BY day;
```

Decision rule (verbatim from the spec):
> If after 30 days of telemetry it shows no measurable improvement, we
> strip it and keep the simpler CAN+LSM+Langevin core.

Concrete pass/fail criteria for 2026-06-19:

- **Hamiltonian**: median `energy_delta` per step stays within `±0.5%` of
  `H₀` over a 24 h run (symplectic stability holds in production).
- **FHRR**: mean `delta` (FHRR − bipolar similarity) on real recall
  workloads is positive *or* there's a documented retrieval class where
  FHRR demonstrably resolves a query the bipolar HV cannot. Otherwise
  rip out the FHRR storage tables and the `Cmplx` math.

## Reproduce

```bash
cd src-tauri && cargo test --lib cognition::hamiltonian
# All 10 hamiltonian tests + 4 new pattern-grad/energy tests pass.

# Inspect cortex telemetry from a running vault
sqlite3 <vault>/.aura/aura.db \
  "SELECT operation, COUNT(*) FROM audit_log WHERE actor='cortex' GROUP BY operation;"
```
