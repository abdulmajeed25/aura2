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

(Future phases appended here.)
