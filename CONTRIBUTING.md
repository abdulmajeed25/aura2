# Contributing to Aura

Thanks for poking around. Aura is a Tauri 2 + Rust + Next.js 15 project; if
you're new to either side, [`CLAUDE.md`](./CLAUDE.md) is the orientation
file Claude Code and humans both read at the start of every session.

## Setup

```bash
# Linux system deps (Tauri's WebView2 equivalent)
sudo apt-get install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev \
                        libappindicator3-dev librsvg2-dev patchelf

# Frontend
pnpm install

# Rust toolchain
rustup default stable
```

## Local verification loop

```bash
cd src-tauri
cargo clippy --no-deps -- -D warnings   # treats warnings as errors
cargo test                              # unit + integration

cd ..
pnpm typecheck
pnpm build                              # static export → out/
pnpm tauri dev                          # full app, needs a display
```

CI runs the same set on every push (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

## House rules

These are the non-negotiables for any change:

1. **Local-First Absolute.** Vault data stays as plain Markdown on disk. No
   proprietary formats. Anything indexed in `.aura/aura.db` must be
   reproducible from the source files.
2. **No mock data.** Tests run against `tests/fixtures/sample-vault/`, not
   in-memory fixtures.
3. **Vertical completion.** A feature ships top-to-bottom (DB → core →
   command → store → UI) before the next one starts.
4. **No `unwrap()` outside tests.** Core code uses `anyhow::Result` + `?`;
   `#[tauri::command]` returns `CmdResult<T>`.
5. **Tauri command registry.** Every new `#[tauri::command]` must also be
   added to the `tauri::generate_handler![…]` list in `src-tauri/src/lib.rs`,
   or it'll 500 at runtime from the frontend even though `cargo build`
   passes.

## Adding a feature

The pattern that ships cleanly:

1. **Schema.** Add a new SQL file in `src-tauri/src/db/migrations/` and
   append it to the `MIGRATIONS` array in `db/sqlite.rs`. The runner is
   idempotent — it skips migrations already in `_aura_migrations`.
2. **Core logic.** Put the pure-data work in `src-tauri/src/core/<area>/`
   and write its tests alongside. Avoid `tauri::*` types in core.
3. **Command.** Add a `#[tauri::command]` in `src-tauri/src/commands/<area>.rs`
   that calls into core, converts errors via `AuraError`, and wires through
   `tauri::generate_handler![…]`.
4. **TS wrapper.** Add `src/lib/tauri/<area>.ts` with strongly-typed
   `invoke<…>` calls. Mirror the Rust DTOs in `src/types/vault.ts`.
5. **UI.** Component lives under `src/components/`. Wire it into a sidebar
   mode or right-panel widget in `src/app/page.tsx`.
6. **Integration test.** Add `src-tauri/tests/<area>_integration.rs` that
   spins up a `VaultState` against a fresh copy of the fixture and
   exercises the new command end-to-end. Cleanup is automatic via
   `remove_dir_all`.

## Stand-ins and their swap checklists

Several Phase 5-9 features ship deterministic stand-ins (hash embedder,
LPA communities, EMA SSM, byte-fingerprint media encoder) because real
ML model files require network access to HuggingFace or local binaries
we couldn't assume. Each one has a swap checklist in `CLAUDE.md` — the
trait surfaces are designed so swapping in the real models is a 1-file
change. PRs that bring real Whisper/SigLIP/MiniLM/Mamba into play are
very welcome.

## Pull-request expectations

- Branch from `main`.
- Keep PRs scoped to a single phase or fix.
- Update `CLAUDE.md` if you add a command, change a stand-in, or shift a
  schema.
- Run the local verification loop before pushing. CI will catch the rest.
