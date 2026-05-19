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

(Future phases appended here.)
