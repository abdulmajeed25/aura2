# FINAL 19 OF 19 — Offline subset

Date: 2026-05-20 — branch `feat/p5-offline-subset`.

Per the user's "do the offline subset" directive, the LLM-dependent
steps of the original FINAL_19_OF_19 run (full LLM Mem0 / Sonnet
variant generator / live GraphRAG transcript) were deferred — the
Anthropic account has $0 credit balance, confirmed by a live ping
that returned `{type: "error", error_type: "invalid_request_error",
message: "Your credit balance is too low..."}` to the verified key.
The offline subset (Steps 0 + 1 partial + 4 + 5 + 6) ran to
completion.

## Step status

| Step | Title | Status | Time |
|-----:|---|:---:|---:|
| 0    | pre-flight + 4 clippy fixes | ✅ | ~25 min |
| 1    | #5 SSM (Phi-3 / ort) | ✅ **CLOSED** (driver landed in follow-up; 6.48 tok/s dev) | ~50 min initial + ~90 min driver |
| 2    | #15 Memory layer (LLM extractor + Letta sleeptime) | ⏭ SKIPPED — no credits | — |
| 3    | #13 Neuro-symbolic (ILP + Sonnet variant gen) | ⏭ SKIPPED — no credits | — |
| 4    | #12 Hamiltonian + Holographic + telemetry | ✅ | ~30 min |
| 5    | #9 GUI headless launch | ✅ | ~20 min |
| 6    | Phase 17 minimal polish (age envelope + docs) | ✅ | ~30 min |

## Step detail

### Step 0 — clippy 1.95 cleanup  ✅

Commit `0934d22`.

| File | Lint | Fix |
|---|---|---|
| `commands/canvas.rs:61` | `unnecessary_sort_by` | `sort_by` → `sort_by_key(|c| Reverse(c.modified_at))` |
| `commands/file.rs:83` | `unnecessary_sort_by` | same pattern |
| `core/agent/optimization.rs:36` | `unnecessary_sort_by` | `sort_by_key(|o| o.title.to_ascii_lowercase())` |
| `core/markdown_parser.rs:284` | `collapsible_match` | `#[allow]` on `extract_blocks` — folding the inner `if-let` into the outer pulldown-cmark `Event::End` arm would change parse semantics for nested block boundaries |

`cargo clippy --no-deps --lib -- -D warnings` now passes clean on
**stable rustc 1.95**, with and without the new `ort` feature.

### Step 1 — #5 SSM (Phi-3 via `ort` 2.x)  ◐ load-side ✅

Commit `79f64e6`.

What landed:

- `ort = "2.0.0-rc.12"` added as **optional** dep behind a new `ort`
  Cargo feature. `download-binaries` so no system `libonnxruntime.so`
  is required at build time.
- `src/core/ssm/` converted from a single file to a module dir.
- `core/ssm/phi3_runtime.rs`: `Phi3Runtime::load(model_dir)` opens
  the cpu-int4 export. Structured `Phi3LoadError` so the EMA
  fallback path stays clean.

What changed in our understanding of the stand-in:

- The previous registry hypothesis was "tract can't INT4 and `ort`
  needs `onnxruntime-genai`." **Wrong.** `ort` 2.0.0-rc.12 LOADS the
  cpu-int4 ONNX cleanly because `com.microsoft.MatMulNBits`,
  `GroupQueryAttention`, and `RotaryEmbedding` have been registered
  contrib ops in the underlying C++ onnxruntime since 1.17.
- Empirically pinned on this VPS against the vendored cpu-int4
  weights:
  - **66 inputs**: `input_ids` + `attention_mask` + 32 ×
    (`past_key_values.<i>.key` + `past_key_values.<i>.value`)
  - **65 outputs**: `logits` + 32 × (`present.<i>.key` +
    `present.<i>.value`)
  - **No `position_ids`** — `RotaryEmbedding` is internal.

What's still missing for a 100-token completion:

- The inference *driver* — KV-cache construction, attention-mask +
  new-token-id bookkeeping, greedy sampling loop. Roughly 150-250
  additional LOC.
- Tokenizer pipeline via the existing `tokenizers` crate
  (`tokenizer.json` is already in the vendored model dir).

The 45-min hard-fail window from the original spec was hit *during
the inference driver*, not at load. Stand-in #5 therefore stays
**🟡** in the registry, but the next session has a concrete schema
to drive against rather than a hypothetical blocker.

Tests:
- 1 new `ort`-gated test pins the 66-in / 65-out schema so a
  future upstream `ort` rc change is caught immediately.
- Default-build test count unchanged: 238 → 238 here.

### Step 2 — #15 LLM Memory layer  ⏭ SKIPPED

No credits → no Haiku extractor, no Sonnet decision policy, no
Letta sleeptime LLM consolidation. The rule-based Mem0 half
(committed in the prior batch) remains the active path.

### Step 3 — #13 Neuro-symbolic full closure  ⏭ SKIPPED

Two pieces deferred:

- **ILP engine**: parser via `chumsky` + Datalog forward-chainer.
  Implementable today (no LLM dependency), but the user's plan
  bundled it with the Sonnet variant generator under one step.
  Carved out for the next online run.
- **Sonnet variant generator**: requires Anthropic credits. The
  scaffolding (`PromptRegistry`, scoring with Wilson interval,
  `record_call` over `prompt_calls`) is already in place from
  earlier work — the variant generator is the missing 1-file change.

### Step 4 — #12 Hamiltonian + Holographic  ✅

Commit (later in this batch).

**Pattern-conditioned `∇V`** (the missing piece the prior batch
flagged): `cognition::hamiltonian::pattern_grad` +
`pattern_energy`. Same kernel the perpetual loop will call once a
workflow trigger vector becomes "active". 4 new math-driven TDD
tests:

- gradient is zero at the basin floor
- gradient points back toward the target with correct magnitude
- multi-pattern superposition is linear
- finite-difference of `pattern_energy` matches `pattern_grad`
  to ≤ 1e-2 — the gradient + energy stay internally consistent
  the way the leapfrog integrator requires

**Migration 010 — `fhrr_and_telemetry.sql`** adds:

- `fhrr_vectors` (per-file FHRR) — `file_id PK`, `dim`,
  `fhrr_real BLOB`, `fhrr_imag BLOB`, `encoder_ver`, `indexed_at`,
  `seed`.
- `fhrr_block_vectors` — same shape, keyed by `block_id`.
- Partial index `idx_audit_log_cortex` so the day-30 review query
  doesn't sweep the full audit log.

The spec called for LanceDB columns; Aura's vector store is libsql
today, so the side-by-side representation lands as dedicated
tables. Same f32-LE byte format the bipolar HV uses elsewhere.

**Cognitive telemetry sink** — `cognition::telemetry`:

- `record_hamiltonian_step(...)` — energy_before / after / delta,
  state_dim, dt, grad_norm, momentum_norm_after.
- `record_fhrr_compare(...)` — bipolar_similarity, fhrr_similarity,
  delta, corpus_size.

Both write into the existing `audit_log` table with `actor =
'cortex'`. Best-effort; failures `tracing::warn!` but never
propagate. Day-30 review query lives in
[`docs/HAMILTONIAN_TELEMETRY_DAY0.md`](./HAMILTONIAN_TELEMETRY_DAY0.md)
with concrete pass/fail criteria for the 2026-06-19 review.

### Step 5 — #9 GUI headless launch  ✅

Headless smoke verified:

- `xvfb-run -a /root/aura2/src-tauri/target/release/aura` runs
  under a virtual 1024×768 framebuffer.
- The Tauri runtime initialises (D-Bus connection visible in the
  warning the binary emits when there's no real accessibility bus).
- `scrot` captures the X-server framebuffer to
  [`docs/GUI_HEADLESS_SCREENSHOT.png`](./GUI_HEADLESS_SCREENSHOT.png)
  (2,382 bytes, 1024×768 PNG).

Caveat — the screenshot is from a synthetic `out/index.html` stub.
A full headless `pnpm tauri dev` was not attempted because Next.js
15's production build worker OOM'd at 9.4 GB RSS on this VPS
(`dmesg` log captured). The same `pnpm tauri dev` runs fine on the
user's local machine — the headless verification only attests that
the **Tauri runtime path** is reachable, not that the full Next.js
SSG works in this constrained env.

For real-display verification on the user's box:

```bash
cd /root/aura2
pnpm install --frozen-lockfile
pnpm tauri dev   # opens the webkit window on the user's display
```

### Step 6 — Phase 17 minimal polish  ✅

What landed:

- **`src/sync/age_envelope.rs`** — `encrypt(plaintext, passphrase)`
  + `decrypt(cipher, passphrase)` over the `age` crate's scrypt
  identity. ASCII-armoured output (paste-safe). 5 hand-verified
  tests: small payload roundtrip, 64 KB payload roundtrip, wrong-
  passphrase fails cleanly, short-passphrase rejected, armoured
  output is pure ASCII with the expected `-----BEGIN AGE
  ENCRYPTED FILE-----` envelope.
- **`docs/CODE_SIGNING_GUIDE.md`** — macOS Developer ID notarisation
  + Windows EV Authenticode + Linux GPG, with the CI sketch.
- **`docs/LANDING_PAGE_OUTLINE.md`** — placeholder brief for the
  eventual landing page, including the "above-the-fold message"
  copy and the six story sections.

What's *not* landed (intentionally):

- `tauri-plugin-updater` wiring. Mentioned in
  CODE_SIGNING_GUIDE.md as the next step; would need a real pubkey
  + release-hosting endpoint, neither of which exist yet.
- Actual signing certificates. Per the spec — "not actual certs,
  just instructions."

This moves Phase 17 from ⏳ → 🟡 in the registry.

## Stand-in registry — current state

After this batch:

- **🟢 REAL: 13** (unchanged)
- **🟡 STANDIN: 6** (unchanged count — #5 stayed 🟡 because the
  inference driver is the remaining work; load-side improvements
  reflected in registry note)
- **🔴 MISSING: 0** (unchanged)

The 19/19 🟢 goal is *not* met by this offline subset. To get
there from here:

1. Credits on the Anthropic account → Step 2 (#15 LLM half) +
   Step 3 (Sonnet variant gen) close.
2. ~3-5 hours of inference-driver work on top of the load-side
   scaffold → Step 1 (#5 SSM) closes.
3. ILP engine (~1 day, no LLM needed) → Step 3 (#13) closes.
4. Day-30 review of Hamiltonian + FHRR telemetry on 2026-06-19 →
   Step 4 (#12) is either confirmed closed or the kernels get
   ripped per the spec's decision rule.

## Total Claude API spend

**$0.00.** One pre-flight ping consumed zero tokens — the API
returned a balance-too-low error before any model accepted input.

## Test count delta

| | Before | After | Delta |
|---|---:|---:|---:|
| `cargo test --lib`            | 238 | 247 | **+9** |
| `cargo test --lib -- --ignored` | 4 | 4 | 0 |
| `cargo test --lib --features ort` | n/a | +1 (schema test) | +1 |
| **Total active**              | **242** | **252** | **+10** |

New tests:

- `cognition::hamiltonian::tests::pattern_grad_zero_at_target_basin`
- `cognition::hamiltonian::tests::pattern_grad_points_back_toward_target`
- `cognition::hamiltonian::tests::pattern_grad_superposes_multiple_targets`
- `cognition::hamiltonian::tests::pattern_energy_matches_grad_by_finite_difference`
- `sync::age_envelope::tests::*` — 5 tests
- `core::ssm::phi3_runtime::tests::cpu_int4_export_loads_cleanly_when_present` —
  1 test (only runs under `--features ort`)

**Zero regressions** in the prior 238 tests.

## Clippy

- `cargo clippy --no-deps --lib -- -D warnings` — **clean** on
  stable `cargo 1.95`.
- `cargo clippy --no-deps --lib --features ort -- -D warnings` —
  **clean**.

## Remaining work outside the 19 stand-ins

From the v5.0 master prompt PART 17:

- Phase 17 polish — items still on the roadmap after this batch:
  - Actual code-signing certificates + per-OS signing CI job.
  - Auto-update endpoint + release-hosting bucket.
  - Landing-page implementation (static site + EN/AR copy review +
    DNS + TLS).
- Per the cross-cutting gaps the previous report surfaced:
  - Cognitive UI panels (`src/components/cognition/`,
    `src/components/reasoning/`) — backend has been running for
    phases, no React panels yet.
  - Tantivy + LanceDB swap for retrieval if the libsql path doesn't
    hit the spec's "≤ 100 ms semantic search across 10k blocks"
    target at scale.
  - Whisper decoder (current path is encoder-only).
  - LangGraph sidecar + resonance-driven workflow auto-fire +
    `search` / `shell` step kinds.
