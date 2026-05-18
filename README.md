# Aura Knowledge Engine

Local-first, AI-native knowledge engine. Phase 1: Tauri 2 + Rust core, Next.js
15 + CodeMirror 6 frontend, libsql for indexing.

## Phase 1 deliverable

Open a folder of `.md` files. Browse the tree. Open a note. Edit. Save
(Cmd/Ctrl-S). Indexed file count tracked in a SQLite database under
`<vault>/.aura/aura.db`. Filesystem changes are watched and the tree refreshes
live.

## Quick start

```bash
# Install JS deps and verify the frontend
pnpm install
pnpm typecheck
pnpm build              # static export to out/

# Verify the Rust core
cd src-tauri
cargo check
cargo clippy --no-deps
cargo test              # 3 unit + 2 integration tests against tests/fixtures/sample-vault

# Run the desktop app (needs a display + Linux system deps)
pnpm tauri dev
```

### Linux system dependencies

```
libwebkit2gtk-4.1-dev libsoup-3.0-dev libappindicator3-dev librsvg2-dev patchelf
```

## Project layout

See [`CLAUDE.md`](./CLAUDE.md) for the full repo orientation, hard rules, and
the multi-phase roadmap.
