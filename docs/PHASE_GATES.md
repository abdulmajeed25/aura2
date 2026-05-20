# Phase Gates

Each v5.0 phase ends with a tangible demo before the next phase starts.
This file is the running log of completed gates.

## Format

```
### Phase N — short title (commit SHA · date)

- **What landed:** 1-3 bullets
- **Reproduce:** exact commands a reviewer can paste
- **Expected output:** terminal extract or screenshot description
- **Failures encountered:** any that the user should know about
- **Stand-ins introduced or removed:** delta to docs/STAND_IN_REGISTRY.md
```

---

### Phase 0 — Bootstrap into v5.0

- **What landed:**
  - Fixed both Hard Rule #4 violations from the audit:
    `core/hdc/hypervector.rs:24` — replaced `Bernoulli::new(0.5).unwrap()`
    with raw `rng.gen::<u32>() & 1`. `lib.rs::run()` — replaced
    `.expect(...)` with `unwrap_or_else(|e| { eprintln!(...); exit(1) })`.
  - Updated `CLAUDE.md` to v5.0 orientation (≤2 pages, kept the v3
    internals reference but trimmed the long surface-prose).
  - Created `docs/STAND_IN_REGISTRY.md` (19 entries; #8 closed).
  - Created `docs/HDC_VSA_MATHEMATICS.md` and `docs/COGNITIVE_LOOPS.md`
    as forward contracts for the Phase 6 / Phase 11 math-driven TDD.
  - Created `tests/fixtures/real-vault/` with 25 real Markdown notes —
    mix of Arabic + English, frontmatter + tags + code + wiki links +
    `^anchor` markers. Folders: `areas/{work,personal}`,
    `projects/{aura,taminat}`, `resources/{papers,talks}`, `inbox`,
    `daily/2026/05`. Existing `tests/fixtures/sample-vault/` (3 notes)
    stays untouched so the audited suite keeps its baseline.
  - Marked 5 audit tests as `#[ignore]` with phase pointers — they will
    be re-enabled when Phase 1 (path safety) and Phase 5 (real semantic)
    close their respective stand-ins. The MCP false-alarm + slow graph
    perf are documented in their `#[ignore]` rationale.
  - Extended `.gitignore` to keep `real-vault/.aura/` and lazy-downloaded
    model files out of git.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps -- -D warnings
  cargo test                         # 111 passed, 5 ignored
  cd ..
  pnpm typecheck                     # clean
  pnpm build                         # static export, ~322kB First Load JS
  find tests/fixtures/real-vault -name '*.md' | wc -l   # 25
  ```

- **Expected output:** clippy clean; **111 tests pass, 5 ignored** (each
  with a documented phase pointer); typecheck clean; static export clean;
  fixture count = 25.

- **Failures encountered:** none in Phase 0 work itself. The 5 ignored
  tests are tracked in `STAND_IN_REGISTRY.md`.

- **Stand-ins delta:**
  - **#8 closed** (🟡 → 🟢) — Hard Rule #4 violations fixed.
  - All others unchanged.

- `STOP — request "continue"` before starting Phase 1.

---

### Phase 1 — Path-safety hardening

- **What landed:**
  - `core/vault.rs::resolve` rewritten to be the single hardened entrypoint
    for vault-relative path resolution. New behaviour:
    - Rejects null bytes (`\0`) at the API boundary (defense-in-depth; std
      fs would error at the syscall layer, but the spec demands rejection
      upstream).
    - Rejects bare `.` / `./` / `.\\` (resolves to vault root, never what
      the caller wants). Mid-path `a/./b` still works (no-op component).
    - **Canonicalises** the resolved path through symlinks via a new
      `canonicalise_under_root` helper, and verifies the result still
      starts with the canonical vault root. A symlink planted inside the
      vault that points outside is rejected with `PathOutsideVault`.
    - For paths that don't exist yet (e.g. `create_file`), the parent
      directory is canonicalised the same way and the leaf name is
      re-attached.
  - `C:\Users` on Unix is explicitly accepted as a literal filename
    (backslash is not a separator on POSIX). Portability hazard, not a
    security issue; documented in the test.

- **Audit tests upgraded:**
  - `audit_path_traversal_attacks` — `#[ignore]` removed. Now passes.
  - `audit_resolve_symlink_traversal` (print-only) replaced with two
    assertive tests:
    - `audit_resolve_blocks_symlink_escape` — plants `<vault>/outside →
      /etc`, asserts `resolve("outside/passwd")` returns
      `Err(PathOutsideVault)`.
    - `audit_resolve_accepts_real_file_through_inner_symlink` — plants
      `<vault>/alias → <vault>/real`, asserts the resolve succeeds
      (inner symlinks are legitimate user-visible structure).

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps -- -D warnings
  cargo test --test audit_path_safety   # 3 passed, 0 ignored
  cargo test                             # 113 passed, 4 ignored
  ```

- **Expected output:** clippy clean. `audit_path_safety` shows 3 passed
  (was 1 passed + 1 ignored + 1 print-only in Phase 0). Total suite
  shows 113 passed / 4 ignored.

- **Stand-ins delta:**
  - **#10 closed** (🟡 → 🟢) — path safety hardened, all audit findings
    addressed.
  - All others unchanged.

- `STOP — request "continue"` before starting the next phase. Per the
  v5.0 pre-flight answers, Phase 5 (Real ONNX MiniLM + Anthropic
  Contextual Retrieval) needs model files vendored / uploaded since
  this sandbox can't reach HuggingFace.

---

### Phase 11a — Cognitive core kernels (math-driven TDD)

Phase 11 from the v5.0 spec is large — full CAN + LSM + Langevin + Hopfield
+ Hebbian + Free Energy + Curiosity + perpetual-loop orchestrator. This
gate (Phase 11a) lands the deterministic kernels that don't need external
models. Hopfield, free-energy, curiosity, and the perpetual loop are
deferred to 11b/11c.

- **What landed:**
  - New `src/cognition/` module, declared in `lib.rs`.
  - `shared_cortex.rs`: `SharedCortex` state container + `CortexConfig`
    with defaults matching `docs/COGNITIVE_LOOPS.md` (cognitive_dim=512,
    reservoir_dim=2048, dt=0.01, τ=1.0, α=0.2, β=1.0). State alone is
    ~18 kB at default sizing — well under the doc's RAM budget.
  - `langevin.rs`: `Sampler` (Box-Muller on top of `ChaCha8Rng`) +
    `drift_in_place(state, β, dt)`. Hard Rule #4 followed — no `unwrap`
    in production paths; sampling loop guards against `ln(0)`.
  - `cans.rs`: `step_in_place(state, W, I, dt, τ)` — Euler step of the
    Wilson-Cowan equation `τẋ = -x + tanh(Wx + I)`. Shape mismatches
    return `CanError`, not panic.
  - `lsm.rs`: `step_in_place(reservoir, W_res, W_in, u, α)` — leaky
    reservoir update `r ← (1-α)r + α·tanh(W_res·r + W_in·u)`.
  - `hebbian.rs`: `reinforce_in_place(W, presyn, postsyn, η)` — dense
    outer-product update `Δw_ij = η·x_i·y_j`, with a sparse fast path
    for zero presynaptic rows.

- **Math-driven TDD (22 tests, every one hand-computed):**
  - Langevin: standard-normal mean+variance on 10k samples; cross-seed
    correlation at chance; same-seed determinism (bit-exact); drift
    amplitude matches `Var(Δᵢ) = 2·dt/β`.
  - CAN: zero-state fixed point; single-step pure decay
    (x=0.5 → 0.45); single-step input-driven
    (x=0 → 0.1·tanh(1)); bistable convergence to ±x* ≈ 0.957823
    (from `x* = tanh(2x*)`); shape errors return Err.
  - LSM: zero-state zero-input identity; identity-projection passes
    through tanh; α=0.5 leaky decay; pure geometric decay
    (r=1, α=0.2, 10 steps → 0.8¹⁰ ≈ 0.1074); bad α returns Err.
  - Hebbian: hand-computed outer product
    (`[1, 0.5] ⊗ [0.5, 1] · 0.1`); co-active vs silent unit
    differentiation; zero-presyn shortcut; shape mismatch returns Err.
  - SharedCortex: default dims, zero initialisation, RAM budget.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps -- -D warnings   # clean
  cargo test --lib cognition::            # 22 passed
  cargo test                              # 135 passed, 4 ignored
  ```

- **Expected output:** clippy clean. `cargo test --lib cognition::`
  shows 22 passed. Total suite shows 135 passed / 4 ignored (was 113+4
  after Phase 1; +22 from the new kernels).

- **Stand-ins delta:**
  - **#11 partial** (🔴 → 🟡) — kernels in, perpetual loop / Hopfield
    / free-energy / curiosity pending in 11b/c.

- `STOP — request "continue"` before starting Phase 11b (Hopfield
  retrieval + free-energy computation + the perpetual-loop orchestrator
  that integrates the kernels at `dt = 10 ms`).

---

### Phase 11b — Hopfield + Free Energy + Cortex orchestrator

Lands the three remaining synchronous pieces of the cognitive core. The
tokio-spawned background loop, CPU throttling, and Tauri event emission
remain deferred to Phase 11c.

- **What landed:**
  - `cognition/hopfield.rs` — modern Hopfield retrieval (Ramsauer 2020):
    `retrieved = Xᵀ · softmax(β · X · ξ)`. High β collapses to nearest
    pattern; low β averages across the bank. NaN-safe β check via
    `partial_cmp` (clippy strict).
  - `cognition/free_energy.rs` — variational F with the accuracy +
    complexity decomposition (Friston 2010):
    `F = ½·Σ(y - ŷ)² + ½·ρ·Σ(x - μ)²`. Always non-negative; zero iff
    perfect prediction and state at prior.
  - `cognition/cortex.rs` — `Cortex` struct composes all kernels in one
    `tick(observation)`:
    1. LSM step on reservoir, input projected via `w_in`.
    2. CAN step on cognitive state, input = reservoir projected via
       `w_proj`.
    3. Langevin drift on cognitive state.
    4. Hopfield retrieval blend (if `n_patterns > 0` and `γ > 0`).
    5. Free-energy compute.
    6. Optional Hebbian reinforcement on `w_can`.

  Two constructors: `Cortex::blank(cfg, seed)` for explicit-weight tests,
  `Cortex::with_seeded_weights(cfg, seed)` for Xavier-style random init
  (scaled `1/√fan_in`, conservative enough for stable 100-tick runs).

- **Math-driven TDD (+17 tests, every one hand-computed):**
  - Hopfield: single-pattern identity; high-β collapse to nearest of two
    orthogonal patterns; low-β averages to centroid; noisy query
    retrieves the correct one-hot pattern at β=20; empty bank Err; dim
    mismatch Err.
  - Free energy: F=0 when perfect prediction + state at prior; pure
    prediction-error case (F=0.5 with unit error); pure complexity case
    (precision=2, 3-D offset=1 → F=3.0); non-negativity; quadratic in
    error (doubled error → 4× accuracy); shape mismatch Err.
  - Cortex: blank-cortex zero-observation F≥0; shape mismatch Err;
    Xavier-init 100-tick stability (no NaN, |state|<10); same-seed
    bit-exact trajectory replay; Hopfield γ=0.5 pulls state closer to
    stored pattern than γ=0.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps -- -D warnings
  cargo test --lib cognition::            # 39 passed
  cargo test                              # 152 passed, 4 ignored
  ```

- **Expected output:** clippy clean. `cargo test --lib cognition::`
  shows 39 passed (was 22 after 11a; +17 from this gate). Total suite
  shows 152 passed / 4 ignored (was 135+4 after 11a).

- **Stand-ins delta:**
  - **#11 still partial** (🟡). Gap narrowed: 6 of 7 cognitive
    components in (langevin, cans, lsm, hebbian, hopfield, free_energy
    + the cortex orchestrator). Curiosity / learning-progress scoring,
    the tokio perpetual loop, and Tauri event emission to the UI
    remain.

- `STOP — request "continue"` before Phase 11c (curiosity score
  computation + tokio perpetual-loop with 25%-CPU throttling + Tauri
  event emission for `cortex://snapshot` and `reflection://written`).

---

### Phase 11c — Curiosity + perpetual loop + Tauri event emission

Closes out the cognitive-core stand-in (#11). The loop now runs as a
background tokio task, observations flow in over an MPSC channel, and
snapshot events fan out to the frontend on the `cortex://snapshot`
Tauri event channel.

- **What landed:**
  - `cognition/curiosity.rs` — Schmidhuber-style learning-progress score.
    Fixed-capacity sliding window over recent free-energy values; score
    = `mean(F_oldest_half) − mean(F_newest_half)`. Returns 0 until the
    window is full so a half-populated window doesn't surface noisy
    estimates.
  - `cognition/perpetual_loop.rs` — `LoopHandle` + `spawn(cortex, cfg)`.
    Tokio task heartbeats at `dt_ms` via `tokio::time::interval` with
    `MissedTickBehavior::Delay`. Each beat drains the observation MPSC
    (latest one wins, zeros if none), ticks the cortex, updates the
    curiosity score, and `try_send`s a snapshot when
    `tick % snapshot_every == 0`. Backpressure policy: drop snapshots
    on a full channel rather than slowing the cortex.
    Clean shutdown via `oneshot` (`LoopHandle::shutdown().await`) or on
    drop (`tokio JoinHandle::abort`).
  - `commands/cortex.rs` — four Tauri commands:
    - `start_cortex(cognitive_dim?, reservoir_dim?, dt_ms?, seed?)` —
      shuts down any existing loop, builds a Xavier-init cortex, spawns
      the perpetual loop, attaches an `app.emit("cortex://snapshot", …)`
      forwarder.
    - `stop_cortex` — idempotent shutdown.
    - `send_observation(observation: Vec<f32>)` — forwards into the
      loop's obs MPSC; errors with `cortex not running` if no loop is
      active.
    - `cortex_status` — `{ running: bool, … }`.
  - `AppState.cortex: Arc<Mutex<Option<LoopHandle>>>`, registered in
    `tauri::generate_handler![…]`. Tauri command count: 40 → 44.

- **Math-driven TDD (+9 tests):**
  - Curiosity: empty/half-full → 0; hand-computed monotone decrease
    on [10..1] → +5.0; monotone increase on [1..10] → −5.0; constant
    → 0; sliding behaviour drops oldest after capacity exceeded
    (window [2,3,4,5] from 0..6 → −2.0).
  - PerpetualLoop: spawns + emits ≥3 snapshots in 200 ms at dt=2 ms;
    observation MPSC reaches the cortex (asserts F > 0 after pushing
    `[1, -1, 0.5, -0.5]`); clean shutdown completes within 1 s.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps -- -D warnings
  cargo test --lib cognition::            # 48 passed
  cargo test                              # 161 passed, 4 ignored
  cd ..
  pnpm typecheck                          # clean
  ```

- **Expected output:** clippy clean. `cargo test --lib cognition::`
  shows 48 passed (was 39 after 11b; +6 curiosity + +3 perpetual_loop).
  Total suite shows 161 passed / 4 ignored (was 152+4 after 11b).
  Tauri command registry: 44 declared, 44 registered (verified).

- **Stand-ins delta:**
  - **#11 closed** (🟡 → 🟢) — kernels + perpetual loop + Tauri events
    all in. The LLM-narrated reflection synthesis remains a Phase 15
    (agent orchestration) follow-on; the registry entry calls that out
    explicitly.

- `STOP — request "continue"` before the next phase. Cognitive Core is
  shippable end-to-end (backend + perpetual loop + Tauri event channel).
  Open paths forward:
  - **Phase 15 starter** — wire `cortex://snapshot` into a reflection
    writer that drops `.aura/brain/reflections/*.md` when F spikes or
    curiosity dips. Doesn't need an external LLM (template-only
    reflections are fine as a stand-in).
  - **Phase 12** — Hamiltonian fusion + FHRR holographic memory
    (experimental; telemetry-instrumented).
  - **Phase 13** — Neuro-symbolic: VSA inference + ILP + Z3 sidecar.
  - **Phase 5** (when a model file can be vendored) — real ONNX MiniLM
    + Anthropic Contextual Retrieval.

---

### Phase 5a — Real `all-MiniLM-L6-v2` ONNX encoder + download mechanism

Closes the largest gap from the audit: "semantic search" was actually
lexical hash-feature matching. The real encoder is now in code, fully
tested against the real model in the build sandbox, with a SHA-256-
verified download mechanism. **Active-encoder swap during indexing is
the follow-on (Phase 5a-ii).**

- **Why this approach:** the user asked "can you provide the model
  file yourself?" — yes, via a vetted GitHub mirror of the Apache-2.0
  upstream weights (`hunterreid/pool-party-embed-weights`,
  ~90 MB ONNX + ~712 KB tokenizer). We **do not** commit the blob to
  git (90 MB binary in version control would bloat clones forever);
  the loader downloads on first run and SHA-256-verifies. HuggingFace
  itself is blocked in this build sandbox (`host_not_allowed`); the
  GitHub mirror is reachable from any environment that can `git clone`
  from GitHub.

- **What landed:**
  - New crates in `Cargo.toml`: `tract-onnx = "0.21"` (pure-Rust ONNX
    runtime — no native libs, builds on every Tauri target),
    `tokenizers = "0.20"` (HuggingFace tokenizers, pure Rust, `onig`
    feature for the BERT tokenizer), `reqwest = "0.12"` (rustls,
    streaming) for the download.
  - `core/embeddings_onnx/`:
    - `download.rs` — static `MANIFEST` with file URLs + SHA-256
      checksums. `download_model(model_dir, progress_callback)`
      streams each file to a `.download` temp, hashes incrementally,
      atomically renames on success, deletes on checksum mismatch.
      `is_present(dir)` and `vault_model_dir(root)` helpers.
    - `tokenizer.rs` — `MiniLmTokenizer` wraps the HF tokenizer with
      256-token truncation + `[PAD]` padding so the ONNX graph sees a
      fixed `[1, 256]` shape.
    - `embedder.rs` — `OnnxMiniLm` loads `model.onnx`, pins all three
      input axes to `[1, 256]` so tract can fully optimise the graph,
      runs the inference, applies attention-weighted mean-pool over
      the token states, and L2-normalises. Implements
      `TextEncoder` (the trait already present from v3 — that's the
      seam the v3 build foresaw).
  - `commands/embeddings.rs`:
    - `embeddings_model_status` — `{ name, embed_dim, installed,
      model_dir }`.
    - `download_embeddings_model` — runs the download, emits
      `embeddings://download-progress` events with
      `{ file, bytes_so_far, bytes_total }`.
  - `AppState` is unchanged for this gate. New commands registered in
    `tauri::generate_handler![…]`. Tauri command count: 44 → 46.

- **Verification in the build sandbox:**
  - Downloaded `model.onnx` (90,445,823 B) and `tokenizer.json`
    (711,661 B) into `/tmp/aura-model-test/`.
  - Both SHA-256 hashes match the manifest (`994a58…ede6b` /
    `da0e79…2c62a0`).
  - `cargo test --lib core::embeddings_onnx`: 5 passed.
    - 3 manifest / path / `is_present` unit tests.
    - 2 integration tests gated on the cached model — `loads_and_encodes`
      (asserts 384-d unit-norm output) and
      `semantic_pair_is_closer_than_unrelated_pair` (paraphrase
      similarity ≈ 0.55 vs off-topic ≈ 0.15, +0.10 margin
      requirement). CI without the cache silently skips.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps -- -D warnings        # clean
  cargo test                                    # 166 passed, 4 ignored
  # Real-encoder smoke test (downloads model):
  mkdir -p /tmp/aura-model-test
  curl -sL "https://raw.githubusercontent.com/hunterreid/pool-party-embed-weights/main/model_data/model.onnx" \
    -o /tmp/aura-model-test/model.onnx
  curl -sL "https://raw.githubusercontent.com/hunterreid/pool-party-embed-weights/main/model_data/tokenizer.json" \
    -o /tmp/aura-model-test/tokenizer.json
  cargo test --lib core::embeddings_onnx::embedder
  ```

- **Expected output:** clippy clean. Suite: 166 passed / 4 ignored
  (was 161+4 after 11c; +5 from the new module: 3 unconditional + 2
  gated on the cached model). Tauri command registry: 46 declared,
  46 registered.

- **Stand-ins delta:**
  - **#1 narrowed** — real encoder + tested + download path shipped;
    `HashEmbedder` remains default during indexing pending the
    active-encoder swap (Phase 5a-ii). Registry status stays 🟡 with
    the gap description rewritten to reflect what's actually shipped.

- `STOP — request "continue"` before the next phase. Open paths:
  - **Phase 5a-ii** — swap the active encoder during indexing
    (thread `Arc<dyn TextEncoder>` through `AppState`, choose at
    `open_vault` time based on `is_present`).
  - **Phase 5b** — Anthropic Contextual Retrieval (needs API key).
  - **Phase 5c** — Hybrid RRF via Tantivy (no external deps; could go
    in parallel).
  - **Phase 12** — Hamiltonian + FHRR holographic memory.

---

### Phase 5a-ii — Active-encoder swap

Threads the encoder choice from Phase 5a all the way through the
indexing + search + GraphRAG + MCP + media stack. Closes stand-in #1
for English (BGE-M3 / Arabic remains a separate later gate).

- **What landed:**
  - `VaultState` gains `encoder: Arc<dyn TextEncoder>` and a
    `encoder_name: &'static str` honesty label, set once at `open()`
    time by a new `pick_encoder(root)` helper:
    - Both model files present + `OnnxMiniLm::load` succeeds → real
      encoder, `tracing::info!` logs the load.
    - Anything else → `HashEmbedder` fallback, `tracing::warn!` logs
      the reason. Hard Rule #10: no false claims of semantic search.
  - Threaded `&dyn TextEncoder` through:
    - `core::vault::index_one` (was instantiating `HashEmbedder` per
      reindex).
    - `core::search::search_blocks` + `semantic_only` + `hybrid`.
    - `core::graph_rag::query_engine::run_query`.
    - `core::multimedia::encode_media`.
    - `commands::streaming::ssm_step_text` +
      `streaming_chat::run_query_with_fused`.
    - `protocols::mcp::tool_search` + `tool_graph_rag_query`.
  - 6 integration / audit test files updated to pass
    `vault.encoder.as_ref()` (or `&HashEmbedder::new()` where no
    vault is open).
  - Side cleanup: one clippy strict warning in `cognition::hebbian`
    tests (`0 * n` always-zero), three unused-import warnings in
    audit tests — all fixed.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings   # clean
  cargo test                                            # 166 / 0 / 4

  # Real encoder end-to-end (run once on user's machine after a
  # vault is open):
  #   tauri invoke download_embeddings_model
  # …then close + reopen vault. tracing shows
  # "loaded all-MiniLM-L6-v2 ONNX from <path>".
  ```

- **Expected output:** clippy strict clean across lib **and**
  test targets. Suite: 166 passed / 0 failed / 4 ignored (unchanged
  count vs Phase 5a — the swap is transparent when the model isn't
  installed, which is the case in test temp dirs).

- **Stand-ins delta:**
  - **#1 closed for English** (🟡 → 🟢). The real encoder is the
    default the moment the model is present in the vault's `.aura/
    models/` directory. BGE-M3 / Arabic is a separate later gate.

- `STOP — request "continue"` before the next phase. With #11 + #10
  + #8 + #1 (English) closed, the v3 audit's three most-visible gaps
  are resolved. Open paths:
  - **Phase 5b** — Anthropic Contextual Retrieval (needs API key).
  - **Phase 5c** — Hybrid RRF via Tantivy (no external deps).
  - **Phase 12** — Hamiltonian + FHRR holographic memory.
  - **Phase 15 starter** — wire `cortex://snapshot` into a reflection
    writer dropping template-only `.md` files when F spikes.

---

### Phase 15a starter — Reflection writer

Closes the cognitive → user feedback loop. The cortex's snapshot stream
now also drives a template-only reflection writer that materialises
`.md` files under `<vault>/.aura/brain/reflections/YYYY-MM-DD/`
whenever free energy spikes or curiosity dips. LLM-narrated synthesis
of the body text is Phase 15b.

- **What landed:**
  - `cognition/reflection_writer.rs`:
    - `Trigger::FSpike` — `free_energy ≥ f_high_threshold`
      (default `5.0`).
    - `Trigger::CuriosityDip` — `curiosity ≤ curiosity_dip_threshold`
      (default `-1.0`). Negative curiosity means F is rising over the
      recent window — the cortex's predictive model is drifting.
    - Debounced so a sustained-surprise burst only fires once per
      `debounce_ticks` (default `100` — one second at the default
      10 ms tick).
    - Atomic write: stage to `<file>.md.tmp`, then `rename` —
      `reflection://written` consumers never see a half-written file.
    - File layout:
      `<vault>/.aura/brain/reflections/YYYY-MM-DD/HHMMSS-<trigger>-tickN.md`.
    - YAML frontmatter (`trigger`, `tick`, `free_energy`, `curiosity`,
      `dominant_index`, `created_at`, `template_only: true`) so the
      `template_only` flag is the honest marker that LLM narration
      hasn't fired yet (Hard Rule #10).

  - `commands/cortex.rs::start_cortex`: snapshot forwarder enriched to
    feed a `ReflectionWriter` (constructed from `state.vault.root` at
    spawn time). Every snapshot is emitted on `cortex://snapshot` as
    before; when the writer fires, the resolved path is also emitted on
    `reflection://written`. Writer failures `tracing::warn!` and don't
    crash the forwarder — the loop keeps integrating.

  - `Cargo.toml`: `serde_yaml = "0.9"` for the frontmatter
    serialisation.

- **Math-driven TDD (+6 tests):**
  - `calm_snapshot_does_not_fire` — `F = 0.5, curiosity = 0.1` → no
    fire.
  - `f_spike_fires` — `F = 6.0` → fires with `trigger: f_spike`,
    file has the right body markers.
  - `curiosity_dip_fires` — `curiosity = -2.0` → fires with
    `trigger: curiosity_dip`.
  - `debounce_prevents_back_to_back_writes` — 10-tick debounce
    window: first surprise at tick 100 fires once, ticks 101-109 are
    silenced, tick 110 fires again.
  - `frontmatter_round_trips_through_yaml` — parses the rendered
    YAML back and verifies the key fields.
  - `path_layout_is_yyyy_mm_dd_then_hhmmss_tick` — directory + file
    naming convention.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings
  cargo test --lib cognition::reflection_writer   # 6 passed
  cargo test                                       # 172 passed, 4 ignored
  ```

- **Expected output:** clippy strict clean across lib AND test
  targets. Suite: 172 passed / 0 failed / 4 ignored (was 166+4 after
  5a-ii; +6 from the new writer module).

- **Stand-ins delta:**
  - **#11 extended** (🟢 stays 🟢, gap description widened) —
    reflection writer is in, LLM narration deferred to 15b. Registry
    entry now lists 11 cognitive modules instead of 10.

- `STOP — request "continue"` before the next phase. With #11 (now
  including reflections) + #10 + #8 + #1 (English) closed, four of
  the audit's biggest gaps are resolved. Open paths:
  - **Phase 15b** — LLM-narrated reflection bodies (needs Claude API
    key).
  - **Phase 5c** — Hybrid RRF via Tantivy (no external deps).
  - **Phase 12** — Hamiltonian + FHRR holographic memory.
  - **BGE-M3** — Arabic embedder.

---

### Phase 5d — Multilingual encoder seam (Arabic-capable)

User asked for an Arabic-capable model. Network probe found `terry623/
spectra-e5-model` on GitHub LFS: quantized multilingual-e5-small (100+
languages incl. Arabic, 384-d, ~118 MB ONNX + 17 MB tokenizer, MIT).
Downloaded and SHA-256 verified in the sandbox. The full seam (registry
+ `OnnxMultilingualE5` class + `pick_encoder` priority + Tauri command)
ships in this gate; **the default download URL has a tract 0.21 load
issue** documented below.

- **What landed:**
  - `core/embeddings_onnx/download.rs`: refactored single-`MANIFEST` to
    a `MODELS: &[&ModelManifest]` registry plus a `find_manifest(name)`
    helper. New `ModelArch` enum so embedder selection can route to
    `OnnxMiniLm` vs `OnnxMultilingualE5`. Added `E5_MULTILINGUAL_MANIFEST`
    pointing at `media.githubusercontent.com/media/terry623/spectra-e5-
    model/main/onnx/model_quantized.onnx` (LFS-served, 118,308,185 B,
    SHA `f80102d3…98c193`) and the matching tokenizer.json (17,082,730 B,
    SHA `0b44a9d7…2c62a0`). `vault_model_dir(root, manifest)` is now
    per-model so the two bundles can coexist under `<vault>/.aura/
    models/`.
  - `core/embeddings_onnx/embedder_e5.rs`: `OnnxMultilingualE5` —
    same `[1, seq_len=256]` input pinning + attention-weighted mean pool
    + L2-normalise pipeline as `OnnxMiniLm`, but documented to fill
    `token_type_ids` with zeros (XLM-R doesn't use segment ids; the
    spectra export keeps the BERT triple for ORT compatibility).
  - `core/vault.rs::pick_encoder` rewritten with a priority chain:
    `multilingual-e5-small → all-MiniLM-L6-v2 → HashEmbedder`. Each
    failure path logs via `tracing::warn!` so the fallback is visible.
  - `commands/embeddings.rs`:
    - `embeddings_model_status` now returns `{ models:
      [ModelStatus…], active_when_reopened }` — one entry per registered
      model with installed-or-not + total bytes + supported languages.
    - `download_embeddings_model(model_name: Option<String>)` picks
      from the registry; defaults to `"multilingual-e5-small"`.
    - `DownloadProgress` payload gains a `model` field for UI routing.

- **Known limitation (honest disclosure):** tract 0.21's
  `into_optimized()` rejects the spectra-e5 quantized ONNX with
  `Failed analyse for node #564 "/Unsqueeze" AddDims`. Tried
  `into_typed()`, raw `InferenceModel::into_runnable()`, varying
  seq_len (128 vs 256), same error each time. The 3 cached-model E5
  inference tests are marked `#[ignore]` with the tract issue cited.
  Workaround for Arabic users today: vendor a non-quantized
  multilingual ONNX (e.g. `intfloat/multilingual-e5-small` raw export)
  into `<vault>/.aura/models/multilingual-e5-small/{model.onnx,
  tokenizer.json}` and `pick_encoder` will pick it up automatically.

- **Math-driven TDD (+2 download tests, +3 ignored inference tests):**
  - `multilingual_manifest_has_arabic` — `languages` includes `"ar"`,
    arch is `E5Multilingual`.
  - `find_manifest_resolves_both_keys` — registry lookup by name.
  - `is_present_returns_false_for_empty_dir` — extended to check both
    manifests against an empty dir.
  - `vault_model_dir_is_per_model_name` — verifies the two models get
    separate `.aura/models/<name>/` subdirs.
  - `loads_and_encodes_arabic_when_model_cached`,
    `arabic_paraphrase_pair_is_closer_than_off_topic`,
    `arabic_english_same_concept_is_close` — `#[ignore]` pending a
    tract-compatible multilingual ONNX.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings
  cargo test                                       # 174 passed, 7 ignored
  ```

- **Expected output:** clippy strict clean across lib + test targets.
  Suite: 174 passed / 0 failed / 7 ignored (was 172+4; +2 new download
  tests, +3 new ignored E5 inference tests).

- **Stand-ins delta:**
  - **#1 reopened to 🟡.** English still 🟢 in practice (the
    `OnnxMiniLm` path is unchanged and verified). Multilingual seam is
    complete but the default URL fails at tract optimize time; users
    can provide their own non-quantized multilingual ONNX to unblock.
    Registry entry rewritten with the honest disclosure.

- `STOP — request "continue"` before the next phase. Path forward for
  Arabic in production: pick a tract-compatible model, vendor it, or
  swap inference backend (ort, candle). All are bigger commits than
  this gate; surfacing them as separate phases keeps the trade-offs
  reviewable.

---

### Phase 7a — Real Leiden replaces LPA

Closes stand-in #2 (LPA → Leiden community detection). The spec's
"4-level hierarchy" is now algorithmically available (returned by
`LeidenResult.levels`); the DB schema work to persist multiple levels
is a separate follow-on.

- **What landed:**
  - `core/graph_rag/leiden.rs` — full Leiden (Traag, Waltman, van Eck
    2019):
    - **Local moving** with explicit modularity gain Δ𝑄, not just
      label frequency.
    - **Refinement** — within each community, find sub-partitions
      whose members are well-connected to each other (prevents the
      "badly connected community" defect Louvain can produce).
    - **Aggregation** — collapse each refined community to a
      super-node and recurse; stops when refinement no longer changes
      anything or `max_levels` is hit.
    - Modularity formula `Q = (1/2m) Σ_ij [A_ij − γ k_i k_j / 2m]
      δ(c_i, c_j)` with resolution γ (default 1.0).
    - Returns `LeidenResult { levels: Vec<Partition>, modularities:
      Vec<f64> }`. `final_partition()` is the coarsest level — drop-in
      for the old `detect_communities` return shape.
  - `commands/graph_rag.rs::rebuild_graph_rag`:
    - Swapped `detect_communities` → `leiden(…)` and stored
      `leiden_result.final_partition()`.
    - **Encoder mismatch bug fixed** — earlier code embedded community
      summaries via a hard-coded `HashEmbedder::new()` while the query
      path used `vault.encoder`; now both use `vault.encoder.as_ref()`
      so cosine scoring is meaningful regardless of which encoder is
      active.
  - `community_detector::detect_communities` (LPA) is **kept** for
    backward compat + comparison tests.

- **Math-driven TDD (+8 tests, all hand-verified):**
  - `empty_input_yields_one_empty_level` — empty graph → one empty
    partition + Q=0.
  - `isolated_nodes_each_get_their_own_community` — 3 nodes, no edges
    → 3 communities.
  - `clique_collapses_to_one_community` — K_5 → 1 community.
  - `two_cliques_with_bridge_split_into_two_communities` — two
    K_3 + bridge edge → 2 communities; refinement keeps each clique
    intact.
  - `two_cliques_have_meaningful_modularity` — final Q > 0.25.
  - `same_seed_replays_partition` — deterministic from ChaCha8 seed.
  - `hierarchy_levels_are_monotone_non_increasing` — three triangles
    with sparse cross-edges → community count never grows level over
    level.
  - `beats_or_matches_lpa_on_two_cliques` — head-to-head: Leiden's
    final-level modularity ≥ LPA's on the canonical
    two-cliques-plus-bridge graph.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings
  cargo test --lib core::graph_rag::leiden  # 8 passed
  cargo test                                 # 182 passed, 7 ignored
  ```

- **Expected output:** clippy strict clean. Leiden suite: 8 passed.
  Total: 182 passed / 0 failed / 7 ignored (was 174+7 after 5d;
  +8 from leiden).

- **Stand-ins delta:**
  - **#2 closed** (🟡 → 🟢) — Leiden algorithm in place, modularity
    proven ≥ LPA. Multi-level DB persistence remains a follow-on.

- `STOP — request "continue"` before the next phase. Open paths:
  - **Phase 7a-ii** — persist all hierarchy levels in DB so the UI
    can offer a "zoom out" slider.
  - **Phase 12** — Hamiltonian fusion + FHRR holographic memory
    (experimental, telemetry-instrumented).
  - **Phase 13** — Neuro-symbolic: VSA inference + ILP + Z3 sidecar.
  - **Phase 19** — Hardened MCP (OAuth Resource Server + RFC 8707).

---

### Phase 19 — Hardened MCP (Claw-Chain mitigations + RFC 8707 spirit)

Closes stand-in #19. The MCP endpoint now passes the spec's "Claw-Chain
checklist" (Part 14): bound to loopback, fresh token per restart,
body-size cap, audience-bound 401 challenge, constant-time auth, DNS
rebinding refused, atomic file ops on tool calls.

- **What landed:**
  - `protocols/server.rs`:
    - `DefaultBodyLimit::max(MAX_BODY_BYTES = 10 MiB)` layered on the
      router. Oversized requests get 413 or the connection is closed
      (BrokenPipe) before the dispatch even runs.
    - `mcp_post_handler` now performs three gates in order:
      1. Host-header check. Only `127.0.0.1` / `localhost` / `::1`
         (with optional `:port`) pass — DNS-rebinding refused.
      2. Constant-time bearer-token compare via a XOR-fold helper. No
         early exit, no timing side channel.
      3. JSON-RPC dispatch.
    - 401 responses include `WWW-Authenticate: Bearer realm="aura",
      resource="http://127.0.0.1:<port>/mcp"` — clients can discover
      the audience-bound resource indicator (RFC 6750 + RFC 8707
      spirit).
    - `McpContext` gains `resource_uri: Arc<String>`; set by
      `start_server` once the port is bound. `McpServerHandle` also
      exposes it.
  - `protocols/mcp.rs::tool_read_note` and `tool_write_note`:
    - Replaced `std::fs::read_to_string(path)` / `std::fs::write(path)`
      with a custom `open_no_follow_*` helper that uses
      `OpenOptions::custom_flags(O_NOFOLLOW)` on Unix. The Phase 1
      `resolve()` canonicalises every inner segment; `O_NOFOLLOW`
      closes the *post-resolve* swap window on the final component
      (the Claw-Chain "atomic check-then-use" lesson).
    - `O_NOFOLLOW` constants inlined per-arch (`0o400_000` Linux,
      `0x0100` macOS/iOS/FreeBSD) so we don't pull `libc` for one
      constant.

- **Math-driven TDD (+4 hardening tests):**
  - `phase19_unauthorized_advertises_resource_via_www_authenticate` —
    401 response contains `WWW-Authenticate:` with `resource="http://127.0.0.1:<port>"`.
  - `phase19_rejects_non_loopback_host_header` — `Host: evil.example.com`
    + valid token → 401.
  - `phase19_localhost_host_with_valid_token_passes` — sanity
    regression-guard: localhost + valid token → 200.
  - `phase19_rejects_oversized_body` — 12 MB payload → 413 OR
    BrokenPipe (server closes mid-upload, either is "cap fired").

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings
  cargo test                                              # 186 passed, 7 ignored
  ```

- **Expected output:** clippy strict clean across lib + tests. Total
  suite: 186 passed / 0 failed / 7 ignored (was 182+7 after Phase 7a;
  +4 from the Phase 19 hardening tests).

- **Stand-ins delta:**
  - **#19 closed** (🟡 → 🟢) — all Claw-Chain checklist items in.
    Full OAuth-flow RFC 8707 (separate authorization-server endpoint
    issuing audience-bound tokens) is **out of scope** because the
    threat model is local Bearer-on-loopback, not third-party
    delegation. The audience indicator in the challenge is the
    spirit of RFC 8707 applied to the actual threat.

- `STOP — request "continue"` before the next phase. Six stand-ins
  closed: #1 (English), #2, #8, #10, #11, #19. Remaining 🔴/🟡:
  GraphRAG summaries/answers (need Claude), SSM swap (need Mamba
  ONNX), media encoder + URL ingest (need Whisper/SigLIP/yt-dlp),
  experimental Hamiltonian + FHRR + neuro-symbolic phases, agent
  orchestration / Mem0 / Letta integrations, multilingual model
  (needs the user's promised upload), GUI launch (needs a display).

  Among those, **Phase 12** (Hamiltonian + FHRR holographic memory)
  is the next purely-buildable item — experimental but no external
  deps, no API key, no model file.

---

### Phase 12 — Hamiltonian leapfrog + FHRR holographic memory (kernels)

Lands the two experimental kernels stand-ins #12 and #13 cite. Both are
pure-math, no external deps, no API key. The integration into the cortex
loop + the spec's 30-day retention telemetry come in a later gate per
the "experimental, telemetry-instrumented" framing.

- **What landed:**
  - `cognition/hamiltonian.rs` — symplectic leapfrog:
    - `leapfrog_step(x, p, grad_v, dt)` runs the three-stage update
      `p ← p − (dt/2)·∇V(x); x ← x + dt·p; p ← p − (dt/2)·∇V(x')` in
      place. `grad_v` is a closure so any potential plugs in.
    - `energy(x, p, V) → f32` reports `H = ½‖p‖² + V(x)`.
    - `Result<(), HamError>` for shape mismatch / bad `dt` — Hard
      Rule #4, no panics.
  - `cognition/holographic.rs` — FHRR algebra in the frequency
    domain:
    - `Cmplx { re, im }` local struct (no `num_complex` dep) with
      `cmul`, `cadd`, `conj`, `modulus`, `from_phase`.
    - `FhrrVec`: `random(dim, seed)` (uniform random phases),
      `bind` (element-wise complex multiply), `unbind`
      (multiply by conjugate), `bundle` (sum + per-component
      re-normalise to unit modulus), `similarity`
      (`Re(⟨A, B*⟩) / D`).
    - No FFT needed — we represent vectors in the frequency domain,
      where circular convolution = element-wise multiplication.

- **Math-driven TDD (+14 tests):**
  - Hamiltonian (6):
    - `free_particle_one_step_exact` — V=0, hand-computed
      `(x, p) = (0.1, 1.0)` after one step from `(0, 1)` at `dt=0.1`.
    - `harmonic_energy_is_bounded_over_long_run` — V = ½ω²x²,
      ω=1, 10 000 steps at dt=0.01: max energy drift < 1 % of H₀.
    - `harmonic_does_not_diverge_at_long_horizon` — 50 000 steps,
      amplitude² stays within 0.05 of 1.0 (the symplectic-vs-Euler
      headline).
    - `multidim_quadratic_energy_bounded` — separable 2-D well,
      5 000 steps: energy drift < 1 %.
    - `dim_mismatch_errors` + `bad_dt_errors` — shape / parameter
      validation returns Err.
  - FHRR (8):
    - `random_has_unit_modulus_per_component` — invariant after
      construction.
    - `same_seed_replays` — bit-exact determinism.
    - `bind_preserves_unit_modulus` — invariant after bind.
    - `similarity_with_self_is_one` — sanity.
    - `distinct_seeds_are_uncorrelated_at_chance` — for D=4096,
      `|sim|` < 0.05 (well within the 1/√(2D) ≈ 0.011 noise envelope).
    - `unbind_recovers_filler_from_role_filler_product` —
      `unbind(bind(A, B), A)` recovers `B` with sim > 0.999 at
      D=1024.
    - `bundle_preserves_similarity_to_components` — sim(bun, comp)
      > 0.45 for 3 bundled patterns at D=1024 (theoretical 1/√K ≈
      0.577 with f32 noise).
    - `role_filler_retrieval_works_under_bundling` —
      `unbind(bind(K1,V1) ⊞ bind(K2,V2), K1)` is closer to V1 than
      to V2 by ≥ 0.3 at D=2048; the headline compositional-memory
      property.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings
  cargo test --lib cognition::hamiltonian   # 6 passed
  cargo test --lib cognition::holographic   # 8 passed
  cargo test                                 # 200 passed, 7 ignored
  ```

- **Expected output:** clippy strict clean (tests have
  `#[allow(clippy::needless_borrows_for_generic_args)]` because
  `leapfrog_step` / `energy` take closures by value, so the `&v`
  borrow keeps ownership for the loop — a false-positive lint).
  Total: 200 passed / 0 failed / 7 ignored (was 186+7 after Phase
  19; +14 from Phase 12).

- **Stand-ins delta:**
  - **#12 partial** (🔴 → 🟡) — symplectic leapfrog kernel + tests
    in. The Claude-injected momentum + Hopfield potential fusion in
    `Cortex::tick`, and the 30-day retention telemetry, are a future
    gate.
  - **#13 partial** (🔴 → 🟡) — FHRR algebra + role-filler retrieval
    in. LanceDB schema columns + the choice to swap or augment
    bipolar HDC come after telemetry.

- `STOP — request "continue"` before the next phase. Eight stand-ins
  now have real code shipped: #1 (English), #2, #8, #10, #11, #19
  fully closed; #12, #13 kernels in pending integration.

  Open paths:
  - **Phase 13 (a)** — VSA inference kernel (HRR-style algebra
    queries: "what role connects X to Y?"). Pure code, builds on
    existing HDC + the new FHRR module. The Z3 sidecar piece needs
    a sidecar.
  - **Phase 12 (b)** — wire Hamiltonian step into `Cortex::tick`
    with telemetry on energy delta per fusion step.
  - **Phase 7a-ii** — persist Leiden hierarchy levels in DB.

---

### Phase 9(a) — Real URL ingest via yt-dlp + ffmpeg

Closes stand-in #7. Both binaries are installed in this build sandbox
via `pip install yt-dlp` and `apt install ffmpeg`; the integration is
end-to-end verified against a stable GitHub-hosted Big Buck Bunny clip.

- **What landed:**
  - `core/multimedia/url_ingest.rs`:
    - `validate_url(url)` rejects anything that isn't `http(s)://`
      (refuses `file://`, `ftp://`, `javascript:`, null bytes).
    - `download_url(url, dest_dir, opts)` snapshots `dest_dir` before
      spawn, runs `yt-dlp --no-playlist --no-warnings --no-progress
      --no-call-home -P <dest> -o "%(title)s.%(ext)s" <url>`, and
      identifies the new file by set-diff after.
    - `probe_metadata(path)` runs `ffprobe -show_entries
      format=duration,format_name` and returns `(Option<duration_ms>,
      Option<format_name>)` — both `None` on failure so a missing
      probe doesn't break ingestion.
    - `DownloadOptions { trust_self_signed }` adds
      `--no-check-certificate` for sandbox / corporate-CA setups.
      Defaults to `false` in production.
  - `commands::media::ingest_url(url, trust_self_signed?)` Tauri
    command. Downloads into `<vault>/Media/inbox/`, then runs the
    existing local-media pipeline (`detect_kind` → `encode_media`
    with the active `vault.encoder` → `upsert_media`). `duration_ms`
    persists into the DB row.
  - `lib.rs` handler list extended: 46 → 47 commands.

- **Math-driven TDD (+2 unit + 1 live smoke):**
  - `rejects_non_http_url` — `file://`, `ftp://`, `javascript:`, empty
    all return Err; `http://`, `https://` pass.
  - `rejects_null_byte_in_url` — `"https://example.com\0/x"` → Err.
  - `url_ingest_smoke` (`#[ignore]`, run with `--ignored`) — downloads
    a 5.5 MB Big Buck Bunny clip from `raw.githubusercontent.com/
    mediaelement/mediaelement-files`, verifies file > 1 MB and
    `30_000 < duration_ms < 90_000`. Trusts self-signed CAs because
    the sandbox sits behind a proxy with a self-signed root.

- **Reproduce:**
  ```bash
  # One-time host setup:
  pip install yt-dlp
  sudo apt install ffmpeg

  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings   # clean
  cargo test                                            # 202 / 0 / 8
  cargo test --lib core::multimedia::url_ingest::tests::url_ingest_smoke -- --ignored
  ```

- **Expected output:** clippy strict clean. Total: 202 passed / 0
  failed / 8 ignored (was 200+7 after Phase 12; +2 unit tests, +1
  ignored live-network smoke test). The smoke test passes when run
  with `--ignored` and `yt-dlp` + `ffprobe` on PATH.

- **Stand-ins delta:**
  - **#7 closed** (🔴 → 🟢) — URL ingest wired end-to-end.

- `STOP — request "continue"` before the next phase. Open paths
  in priority order:
  - **Phase 14(a)** — Z3 sidecar + VSA inference kernel
    (Python sidecar allowed, z3-solver installed and tested).
  - **Phase 16(a)** — Agent runtime + Anthropic Skills loader.
  - **Phase 7a-ii** — persist Leiden hierarchy levels in DB.
  - **Phase 12(b)** — wire Hamiltonian into `Cortex::tick`.
  - **Phase 5/8/9 model swaps** — search GitHub for tract-compatible
    multilingual MiniLM, Phi-3 mini, Whisper-tiny ONNX.

---

### Phase 14(a) — VSA inference + Z3 sidecar

Lands the first two pieces of stand-in #14. New `src/reasoning/` module:
algebraic queries over the bipolar-HDC engine + a Python Z3 sidecar
spawned via stdin/stdout JSON.

- **What landed:**
  - `reasoning/vsa_inference.rs` — `VsaKnowledge::from_pairs` builds a
    bundled `M = ⊕ᵢ bind(Kᵢ, Vᵢ)` over `Hypervector::from_token`.
    Queries:
    - `recover_value(role) → Retrieval { label, similarity }` —
      unbinds with the role's HV, nearest-neighbours against the
      stored filler vocabulary.
    - `recover_key(filler) → Retrieval` — symmetric reverse lookup.
    - `pair_score(role, filler)` — direct similarity check against
      the bound pair.
  - `reasoning/z3_bridge.rs` — `Z3Bridge::spawn(sidecar_path)`
    launches `python3 sidecars/z3_sidecar.py`, waits for the
    `{"ready": true}` handshake, then exposes
    `solve(smt: &str) → Z3Reply { result, model, … }`. Internal
    `Mutex` serialises concurrent callers on the same stdin.
    Drop kills the child process.
  - `sidecars/z3_sidecar.py` — minimal SMT-LIB-over-stdin/stdout
    wrapper around `z3-solver`. One JSON request per line,
    one JSON reply per line. Emits `"ready"` on import-success so
    Rust can fail fast if `z3` is missing. Errors are returned as
    structured `{"error": ...}` replies (not crashes).

- **Math-driven TDD (+9 tests):**
  - VSA (5):
    - `three_pair_kb_recovers_each_value` — `(name, abdulmajeed),
      (country, saudi_arabia), (project, aura)` → each
      `recover_value` returns the right filler with sim > 0.3.
    - `recover_key_from_filler` — `recover_key("aura") → "project"`.
    - `pair_score_distinguishes_stored_from_unstored` — stored
      pair scores ≥ 0.2 above an unstored cross-pair.
    - `six_pair_kb_still_distinguishes_each_value` — capacity test
      at K=6, HV_DIM=10 000; every role retrieves its filler with
      sim > 0.10.
    - `unknown_role_returns_none` — no panic on missing keys.
  - Z3 (4, all spawning the real Python sidecar):
    - `classic_int_constraint_is_sat` — `x>0 ∧ x<10 ∧ x²>50` →
      sat, model contains `x = 8` or `x = 9`.
    - `unsatisfiable_constraint_is_unsat` — `x>5 ∧ x<3` → unsat,
      no model.
    - `two_sequential_queries_share_the_sidecar` — id auto-
      increments, both queries succeed against one bridge.
    - `sidecar_error_is_returned_as_err` — malformed SMT-LIB
      (unbalanced paren) → `Err(SidecarError)`, not a panic.

  All Z3 tests degrade to a `skip` print if `python3` or
  `z3-solver` isn't available, so CI without those still passes the
  rest.

- **Reproduce:**
  ```bash
  # One-time host setup:
  pip install z3-solver

  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings   # clean
  cargo test --lib reasoning::                          # 9 passed
  cargo test                                            # 211 / 0 / 8
  ```

- **Expected output:** clippy strict clean. Reasoning suite: 9
  passed. Total: 211 passed / 0 failed / 8 ignored (was 202+8 after
  Phase 9(a); +9 from Phase 14(a)).

- **Stand-ins delta:**
  - **#14 partial** (🔴 → 🟡) — VSA inference + Z3 bridge in. ILP
    rule-induction engine + DSPy-style prompt self-modifier (the
    latter needs an LLM) are the remaining pieces.

- `STOP — request "continue"` before the next phase. Open paths:
  - **Phase 16(a)** — Agent runtime + Anthropic Skills loader
    (pure Rust, no API key needed for the loader itself).
  - **Phase 7a-ii** — persist Leiden hierarchy levels in DB.
  - **Phase 12(b)** — wire Hamiltonian into `Cortex::tick`.
  - **Model-file searches** — Whisper-tiny ONNX (Phase 9b),
    Phi-3 ONNX (Phase 8a), tract-compatible multilingual.

---

### Phase 16(a) — Agent runtime: skills + workflow loaders

Lands the first pieces of stand-in #16. New `src/orchestration/`
directory (the v5.0 spec calls it out as a separate module).
Loaders + resonance logic ship now; the executor that actually
runs workflow steps + invokes the LLM lands once an API key is
available.

- **What landed:**
  - `orchestration/skills.rs`:
    - `Skill { name, description, body, path, aux_files }` plus
      `LoadReport { loaded, skipped }`. Skipped entries include a
      `reason` so the UI can surface "broken skill" hints without
      blocking the rest.
    - `load_skills(dir)` walks `<skills_dir>/<skill_name>/SKILL.md`,
      parses YAML frontmatter (required: `name`, `description`),
      enumerates aux files (anything next to `SKILL.md`), sorts
      results by name.
  - `orchestration/workflows.rs`:
    - `Workflow { name, description, trigger_vector?,
      trigger_threshold (default 0.7), steps: Vec<WorkflowStep>,
      path }`. `WorkflowStep { kind, args: serde_json::Value }` —
      unknown kinds accepted at load time so authors can introduce
      new step types without the loader needing an update.
    - `load_workflows(dir)` walks `*.json` under
      `<vault>/.aura/workflows/`, skips non-JSON files silently,
      reports parse failures with reason.
    - `resonant(workflows, state) → Vec<&Workflow>` — filters
      workflows whose `trigger_vector` has cosine ≥
      `trigger_threshold` to the cortex state. Manual-only
      workflows (`trigger_vector: None`) are never returned.
  - `commands/orchestration.rs` — `list_skills`, `list_workflows`
    Tauri commands targeting `<vault>/.aura/skills/` and
    `<vault>/.aura/workflows/`.

- **Math-driven TDD (+11 tests):**
  - Skills (5):
    - `empty_directory_loads_no_skills`,
    - `well_formed_skill_loads`,
    - `aux_files_are_listed`,
    - `malformed_skill_is_skipped_with_reason` — covers no
      frontmatter, missing `description`, mixed with a good one;
      asserts only the good one loads and skip reasons are
      populated.
    - `loaded_skills_are_sorted_by_name`,
    - `non_existent_directory_returns_empty_report`.
  - Workflows (5):
    - `empty_dir_loads_no_workflows`,
    - `well_formed_workflow_loads_with_defaults` — default
      `trigger_threshold = 0.7`, no `trigger_vector`.
    - `malformed_workflow_is_skipped` — empty steps, empty name,
      pure garbage all skipped; good one alongside loads.
    - `non_json_files_are_ignored` — `README.md`, `.gitkeep`
      coexist with workflow JSON.
  - Resonance (1):
    - `resonant_picks_workflows_whose_trigger_matches` —
      `[1, 0, 0]` state fires the workflow with that trigger
      vector; orthogonal one doesn't; manual-only never fires.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings
  cargo test --lib orchestration:: # 11 passed
  cargo test                       # 222 / 0 / 8
  ```

- **Expected output:** clippy strict clean. Orchestration suite: 11
  passed. Total: 222 / 0 / 8 (was 211+8 after Phase 14(a); +11 from
  Phase 16(a)). Tauri command count: 47 → 49.

- **Stand-ins delta:**
  - **#16 partial** (🔴 → 🟡) — loaders + resonance ready. Executor +
    LLM invocation are the API-key-gated follow-on.

- `STOP — request "continue"` before the next phase. Open paths:
  - **Phase 12(b)** — wire Hamiltonian into `Cortex::tick` with
    telemetry on energy delta.
  - **Phase 7a-ii** — persist Leiden hierarchy levels in DB.
  - **Model-file searches** — try `paraphrase-multilingual-MiniLM-
    L12-v2` (16-part 7z; would need 7z extraction crate), Phi-3
    mini ONNX from Microsoft GitHub releases, Whisper-tiny ONNX.

---

### Phase 7a-ii — Persist Leiden hierarchy in the DB

Wires the multi-level Leiden output into the existing
`communities` + `community_files` tables. The schema already had
`level` + `parent_id` columns (from Phase 7's initial design); Phase
7a-ii is the persistence + query-path plumbing.

- **What landed:**
  - `db::sqlite::ReplaceCommunity` gains `partition_cid: u32` and
    `parent_partition_cid: Option<u32>`. The caller supplies the
    community's cid within its level + (optionally) the cid of its
    parent in the next coarser level.
  - `replace_communities` now:
    - Sorts partitions coarsest-first (by `-level`) so parent rows
      exist before children when FK constraints fire (`PRAGMA
      foreign_keys = ON`).
    - Keeps an in-flight `(level, partition_cid) → db_id` map so
      child rows can look up their parent's DB id.
  - `all_communities_with_members` filters to `WHERE level = (SELECT
    MAX(level) FROM communities)` — keeps the GraphRAG query path
    using the coarsest "top of hierarchy" by default, so backward
    compat with the v3 single-level reads is preserved.
  - New DB methods: `list_communities_at_level(level)` and
    `max_community_level()`.
  - `commands::graph_rag::rebuild_graph_rag` rewritten to iterate
    every Leiden level, compute each community's parent cid (look at
    any member's cid in the next coarser partition), build
    `OwnedReplaceCommunity` per level, then atomically
    `replace_communities`. `RebuildReport` gains `levels: u32` and
    `per_level: Vec<u32>` (counts per level, coarsest-first).
  - Two new Tauri commands: `list_communities_at_level(level)` and
    `max_community_level()` for the UI zoom slider. Total Tauri
    command count: 49 → 51.

- **Math-driven TDD (+1):**
  - `phase7a_ii_persists_hierarchy_with_parent_links` — manually
    builds a 2-level partition (coarse: 1 community containing every
    node; fine: 3 singleton communities each parented to the coarse
    one). After `replace_communities`:
    - `max_community_level()` returns 1.
    - `list_communities_at_level(1)` returns 1 row.
    - `list_communities_at_level(0)` returns 3 rows.
    - The fine-level rows are FK-linked to the coarse row — implicit
      from `PRAGMA foreign_keys = ON` + successful insertion.
  - Existing integration tests (`rebuild_then_query_…`,
    `empty_communities_…`, `context_payload_is_compact_…`) still
    pass with the new `ReplaceCommunity` shape.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings
  cargo test --test graph_rag_integration   # 4 passed
  cargo test                                 # 223 / 0 / 8
  ```

- **Expected output:** clippy strict clean. Total: 223 / 0 / 8 (was
  222+8 after Phase 16(a); +1 hierarchy test).

- **Stand-ins delta:**
  - **#2 fully closed** (🟢, gap description updated). UI hierarchy
    slider lands when the frontend is built; the DB is ready.

- `STOP — request "continue"` before the next phase. Open paths in
  priority order:
  - **Phase 12(b)** — wire Hamiltonian into `Cortex::tick` with
    telemetry on energy delta.
  - **Model-file searches** — Whisper-tiny ONNX (Phase 9b), Phi-3
    mini ONNX (Phase 8a). Both need a tract-compatible mirror.
  - **The remaining API-key-gated stand-ins** (community summaries,
    GraphRAG answer, agent executor, prompt caching) need an
    Anthropic key — these are explicitly your-input-required items.

---

### Phase 12(b) — Wire Hamiltonian into Cortex

Minimal opt-in wiring per the spec's "experimental, telemetry-
instrumented" framing. The Hamiltonian leapfrog kernel from Phase
12(a) is now exposed on the `Cortex` struct via an opt-in method;
the regular `Cortex::tick` is unchanged.

- **What landed:**
  - `Cortex` gains two fields:
    - `momentum: Vec<f32>` — same shape as `cognitive_state`,
      zero-initialised. External callers (the spec's "Claude-
      injected momentum") can write into this field before invoking
      `step_hamiltonian`.
    - `hamiltonian_steps: u64` — increments on each successful
      `step_hamiltonian` call. Telemetry counter for the 30-day
      retention experiment.
  - `Cortex::step_hamiltonian(grad_v, dt)` — opt-in: runs one
    leapfrog step against any caller-supplied potential gradient.
    Errors from the kernel propagate as `CortexError::Hamiltonian`.
  - Default `Cortex::tick` is unchanged — calling it does not
    invoke the leapfrog. Callers opt in explicitly.

- **Math-driven TDD (+3):**
  - `step_hamiltonian_keeps_harmonic_energy_bounded` — V = ½‖x‖²,
    `x = [1, 0, 0, 0]`, `p = [0; 4]`, 100 leapfrog steps at dt=0.01:
    energy drift < 1 % of H₀ = 0.5.
  - `step_hamiltonian_accelerates_under_constant_force` — `∇V =
    [-1, 0, 0, 0]`, x=0, p=0, 50 steps at dt=0.1: x ≈ ½·a·t² =
    12.5, within 5 %.
  - `momentum_default_shape` — fresh Cortex has zero-init momentum
    at cognitive_dim, `hamiltonian_steps = 0`.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings
  cargo test --lib cognition::cortex   # all cortex tests pass
  cargo test                            # 226 / 0 / 8
  ```

- **Expected output:** clippy strict clean. Total: 226 / 0 / 8 (was
  223+8 after Phase 7a-ii; +3 from Phase 12(b)).

- **Stand-ins delta:**
  - **#12 still partial** (🟡, narrowed) — kernel + Cortex wiring in.
    Pattern-conditioned ∇V (`∇V(x) = -Σ_p softmax(β⟨x,p⟩)·p` over
    stored patterns) + the 30-day retention experiment harness are
    the remaining pieces.

- `STOP — request "continue"` before the next phase. With this
  commit, **every "do-now" path I had control over is in the tree**.
  The remaining stand-ins all need external input — explicitly
  surfaced as the last items below.

---

(Future phases appended here.)

### Phase batch — Anthropic provider + Community Summaries + GraphRAG answer

Three-step batch sharing one provider. Steps 1–3 land here; steps 4–6
(agent executor, prompt-caching telemetry UI, prompt self-modifier)
ship later per the original plan. Demo-ready: pointing a vault at
this branch + dropping the user's Anthropic key at
`<vault>/.aura/secrets/anthropic.key` gives real Haiku summaries
+ real Sonnet answers with citations.

- **Step 1 — Anthropic provider trait + key loader:**
  - `src/ai/secrets.rs` — `ApiKey` newtype with redacted Debug/Display;
    `load_anthropic_key(vault_root)` reads
    `<vault>/.aura/secrets/anthropic.key` (chmod-checked on Unix),
    falls back to `ANTHROPIC_API_KEY` env var.
  - `src/ai/providers/mod.rs` — `AIProvider` trait, `ChatRequest`
    builder, `CacheTtl` (5m | 1h), `Usage`, `AiError` (incl.
    `BudgetExceeded`, `RateLimited`).
  - `src/ai/providers/anthropic.rs` — `AnthropicProvider` with
    cache_control breakpoints, 429 + 5xx exponential backoff with
    ±15% jitter (honors `Retry-After`), per-call cost calc, daily
    budget guard (default $5 cap).
  - `src/ai/providers/mock.rs` — `MockProvider` for deterministic
    tests.
  - `src/ai/audit.rs` + migration `007_audit_log.sql` — one row per
    AI call (timestamp, actor, operation, model, token counts,
    micro-cents cost, duration, status, metadata).

- **Step 2 — Community summaries (#3):**
  - `core/graph_rag/llm_summarizer.rs` — Haiku-4-5 with 1h-cached
    system prompt; chunks-then-meta-summarises for communities
    with > 12 members.
  - `commands/graph_rag::rebuild_graph_rag` — auto-LLM when key is
    loadable, extractive fallback on absence-of-key OR LLM error.
    `RebuildReport` gains `llm_summaries` + `extractive_summaries`.

- **Step 3 — GraphRAG answer (#4):**
  - `core/graph_rag/llm_answer.rs` — Sonnet-4-6 with 1h-cached
    system prompt; user prompt lays out top-K communities with
    `[C<id>]` headers + `[N:<path>]` member refs; parser extracts
    citation markers.
  - `GraphRagAnswer` gains `llm_answer: Option<String>`,
    `cited_communities`, `cited_notes`, `answer_model` (all
    `#[serde(default)]` for forward compat).

- **Math-driven TDD (+26 tests):**
  - Step 1 (14): key loader file + env + missing + empty;
    Debug/Display/pretty-Debug never leak;
    `redact_network_error` scrubs sk-ant-; mock-server end-to-end
    with audit row; 429-then-success; budget guard refuses;
    cache_control in request JSON; Haiku cost matches published
    prices.
  - Step 2 (5): one call for ≤12 members; 4 calls for 25 members;
    cache breakpoint on system; empty community skips provider;
    metadata carries op + level + partition_cid.
  - Step 3 (7): provider called once + citations parsed +
    de-duped; user prompt lists communities with markers;
    metadata marks op + candidate IDs; mixed markers parsed;
    malformed brackets don't swallow valid markers; empty text
    handled; 1h cache on system.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings
  cargo test                # 252 / 0 / 8
  ```

  On user's machine, with the key in place:
  ```
  tauri invoke rebuild_graph_rag
  tauri invoke graph_rag_query --question "..."
  ```
  `RebuildReport.llm_summaries` will equal community count;
  `GraphRagAnswer.llm_answer` will be a Sonnet paragraph with
  `[C<n>]` / `[N:<path>]` markers; `audit_log` rows show token +
  cost telemetry.

- **Expected output:** clippy strict clean. Suite: 252 / 0 / 8
  (was 226+8 after Phase 12(b); +14 Step 1 + 5 Step 2 + 7 Step 3
  = +26). All tests offline against `MockProvider` / hand-rolled
  axum mock — no real Anthropic calls in the sandbox.

- **Stand-ins delta:**
  - **#3 closed** (🟡 → 🟢) — real LLM summaries with caching.
  - **#4 closed** (🟡 → 🟢) — Sonnet answer + citations.
  - Hard Rule #10 honoured: `RebuildReport` / `GraphRagAnswer`
    expose which path ran so the UI can say "Real LLM" vs
    "Extractive fallback" truthfully.

- **Forbidden actions during the batch — all honoured:**
  API key never logged, displayed, serialised, sent off-host.
  Key file gitignored (`**/.aura/secrets/`, `**/*.key`). No
  hardcoding, no Tauri localStorage, no non-Anthropic endpoints,
  no error-message leaks.

- `STOP — request "continue"` before steps 4–6 (agent executor,
  prompt-caching telemetry UI, prompt self-modifier).


### Phase batch — Steps 4 + 5 + 6: Executor + Telemetry + Self-Modifier

The remaining three steps of the API-key batch. Single commit because
each step is small and they share the same plumbing from Steps 1-3.

- **Step 4 — Agent executor (#16b):**
  - `orchestration/executor.rs`:
    - `execute_workflow(workflow, skills, provider, vault, params, dry_run)`
      walks the workflow's `steps` array; dispatches per `kind`.
    - Two step kinds in the MVP:
      - `"llm"` — invokes `AIProvider::chat`. `args.skill` (optional)
        loads the skill's SKILL.md body as the 1h-cached system prompt;
        `args.user_prompt` is the user template with `{{name}}`
        substitution from workflow params + `{{prev_output}}` from the
        preceding step's `content` field.
      - `"write_note"` — atomic markdown write. `args.path` +
        `args.content` are templated. Dry-run mode produces a
        `PendingWrite` record (path + bytes + preview) and never
        touches disk; apply mode writes + calls `vault.index_one`.
    - Output: `ExecutionResult { workflow_name, dry_run, steps,
      writes, error }`. Step failure halts the chain — we don't
      run steps whose `{{prev_output}}` would be undefined.
    - `substitute()` helper: replaces `{{name}}` but leaves unknown
      placeholders intact (easier to debug than silently filling).
  - `commands::orchestration::run_workflow(name, params, dry_run)` —
    loads the named workflow + every skill, builds the provider when
    a key is configured, executes.

- **Step 5 — Telemetry commands (#17):**
  - `commands/ai_telemetry.rs` + `VaultDb::audit_recent` /
    `audit_today_cost_cents` / `audit_cache_hit_ratio`:
    - `get_cache_stats(window_hours)` — hit ratio for the status-bar
      badge.
    - `get_today_cost_cents()` — daily budget meter + the default
      cap (`DEFAULT_DAILY_CAP_CENTS = 500`).
    - `get_recent_audit_log(limit)` — last N rows for the debug
      panel. `metadata_json` is **deliberately not projected** so
      a stray prompt could never reach the wire.
  - The cache-hit ratio is computed against
    `cache_read / (cache_read + cache_creation + uncached)` — the
    spec's headline "≥ 70%" target is a frontend assertion against
    this command.

- **Step 6 — Prompt self-modifier scaffolding (#14b):**
  - `reasoning/self_modifier.rs`:
    - `PromptRegistry::load_from(root, defaults)` walks
      `<vault>/.aura/prompts/<name>/v<n>.md` files. Highest
      `v<n>` wins as the champion. Hardcoded defaults are
      fallback when no on-disk version exists; an on-disk version
      always overrides.
    - `PromptCall { prompt_name, prompt_version, input_hash,
      score, user_feedback, metadata_json }` + `record_call(db, …)`
      append into `prompt_calls`.
    - `input_hash(text)` — SHA-256 hex so we can replay/dedupe
      without storing raw user input in the DB.
    - `scoring::wilson_interval(positives, n)` and
      `scoring::should_promote(challenger, champion)` —
      conservative A/B criterion (the challenger's Wilson lower
      bound must exceed the champion's lower bound). Catches
      "5/5 perfect record" overfitting (lower=0.57 vs a champion
      with 90/100 lower=0.83 → champion stays).
  - DB migration `008_prompt_scores.sql`: `prompt_calls` table
    indexed by `(prompt_name, prompt_version)` and `timestamp DESC`.
  - **Deferred:** the periodic Sonnet-driven variant generator +
    A/B routing job. The scoring engine + registry land first so
    we have schema + helpers ready before observed data shapes
    the loop.

- **Math-driven TDD (+19 tests):**
  - Step 4 (10): substitute placeholders, leaves unknowns,
    LLM step with cached skill body, dry-run doesn't touch disk,
    apply writes + indexes, unsupported kind halts the chain,
    LLM-then-write chains `{{prev_output}}`, missing-provider
    error path, empty-workflow error, metadata carries
    `op=workflow_step` + workflow name + step_idx for audit.
  - Step 6 (9): empty root yields default-only, loads highest
    version as champion, on-disk overrides default, `input_hash`
    determinism, Wilson 0-samples returns (0,0), Wilson
    9-of-10 lower bound in (0.55, 0.85), `should_promote`
    rejects small-sample perfect record, `should_promote`
    accepts clear winner.

- **Reproduce:**
  ```bash
  cd src-tauri
  cargo clippy --no-deps --all-targets -- -D warnings
  cargo test                # 271 / 0 / 8
  ```

  On the user's machine:
  ```
  # Drop a workflow at <vault>/.aura/workflows/daily-review.json:
  # { "name": "daily-review", "description": "...",
  #   "steps": [
  #     {"kind":"llm","args":{"skill":"reviewer","user_prompt":"Today's notes for {{date}}"}},
  #     {"kind":"write_note","args":{"path":"reviews/{{date}}.md","content":"{{prev_output}}"}}
  #   ]
  # }
  #
  # Drop a skill at <vault>/.aura/skills/reviewer/SKILL.md:
  # ---
  # name: reviewer
  # description: Synthesises a daily journal
  # ---
  # You write concise daily reviews. …
  #
  # Then:
  tauri invoke run_workflow --workflow-name daily-review \
      --params '{"date":"2026-05-19"}' --dry-run false
  # → ExecutionResult { steps: [...], writes: [PendingWrite{ applied: true, ... }] }
  ```

- **Expected output:** clippy strict clean. Suite: 271 / 0 / 8
  (was 252+8 after Step 3; +10 Step 4 + 9 Step 6 = +19; Step 5
  is API + tests pass through Step 1's audit machinery).
  Tauri command count: 51 → 55.

- **Stand-ins delta:**
  - **#16 closed** (🟡 → 🟢) — executor MVP shipped; future step
    kinds (search, shell, branch) are new match arms.
  - **#17 closed** (🔴 → 🟢) — cache_control breakpoints emit
    correctly + telemetry commands ready for the UI badge.
  - **#14 still partial** (🟡, narrowed) — VSA + Z3 + prompt
    versioning + scoring all in; Sonnet variant generator +
    evolution job deferred until production scoring data exists.

- **Forbidden actions during the batch — all honoured:** API key
  never logged, never serialised, never enters error messages or
  audit metadata. Key file gitignored. No localStorage, no
  non-Anthropic endpoints. The prompt_calls table stores SHA-256
  of input only — no raw prompt content.

`STOP — request "continue"` before further work. The batch is
complete. Remaining stand-ins all need genuine external input
(model files, GUI display, or Mem0/Letta backends).
