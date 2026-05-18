# Aura Knowledge Engine — CLAUDE.md

Project orientation file. Reread at the start of every session before making changes.

## What this is

Local-first, AI-native knowledge engine. Stores plain `.md` files on disk; never
proprietary formats. Built on Tauri 2 (Rust core) + Next.js 15 (App Router,
static export) + CodeMirror 6.

## Hard rules (from the master spec, section 0.1 / 11.1)

1. **Local-First Absolute.** All authoritative data is plain Markdown on disk.
2. **No mock data.** Test every feature against a real vault. The fixture lives
   at `tests/fixtures/sample-vault/`.
3. **Vertical Completion.** A feature ships top-to-bottom (DB → API → UI) before
   the next one starts. Don't half-implement across phases.
4. **No `unwrap()` outside tests.** Use `anyhow::Result` + `?` in core code and
   `CmdResult<T>` in `#[tauri::command]` functions.
5. **Stop after each phase.** Wait for explicit "تابع" / "continue" before
   starting the next phase.

## Repo layout

```
src-tauri/              Rust core (Tauri 2 backend)
├── src/commands/       #[tauri::command] handlers exposed to the frontend
├── src/core/           vault, markdown_parser, file_watcher
├── src/db/             libsql wrapper + migrations
├── src/utils/          error type (AuraError + AuraErrorWire)
├── tests/              integration tests against the fixture vault
├── Cargo.toml
└── tauri.conf.json

src/                    Next.js 15 App Router (static export → out/)
├── app/                page.tsx (single root view), layout.tsx, globals.css
├── components/         editor/CodeMirrorEditor, sidebar/FileExplorer
├── lib/tauri/          typed wrappers around invoke()
├── lib/store/          Zustand stores: vaultStore, editorStore
└── types/              VaultInfo, TreeNode, FileEntry, …

tests/fixtures/sample-vault/   Real fixture vault used by integration tests
```

## State model

- **Rust:** `AppState` (in `lib.rs`) holds `Arc<Mutex<Option<VaultState>>>`.
  Exactly one vault open at a time. `VaultState` carries `root: PathBuf` and
  `db: Arc<VaultDb>`.
- **Frontend:** two Zustand stores — `vaultStore` (info, tree, open/close)
  and `editorStore` (activePath, content, dirty, save).
- **Event channel:** the Rust file watcher emits `vault://changed` events
  carrying `{ kind, path }`. The frontend listens once in `app/page.tsx`.

## Database

- `libsql 0.6` local SQLite file at `<vault>/.aura/aura.db`.
- Migrations are embedded via `include_str!` in `db/sqlite.rs` and tracked in
  `_aura_migrations`. Add a new file in `db/migrations/` and append to the
  `MIGRATIONS` const to ship a new one.
- Phase 1 schema: `files` table only. Phases 2/3/4 will add `blocks`, `links`,
  `tags`, `blocks_fts`, vector indexes in LanceDB, etc.

## Path safety

`VaultState::resolve(rel)` rejects:
- absolute paths (`/foo`, `\foo`, `C:\foo`),
- any path containing a `..` component,
- empty input.

Always go through `resolve()` before touching the filesystem from a command
handler. Tests cover this in `tests/vault_integration.rs::rejects_paths_escaping_vault_root`.

## Run / verify

```bash
# Rust
cd src-tauri
cargo check
cargo clippy --no-deps
cargo test                       # runs unit + integration tests against the fixture

# Frontend
pnpm install
pnpm typecheck
pnpm build                       # static export → out/

# Full app (requires display)
pnpm tauri dev
```

In this sandbox the GUI cannot launch (no display). Validate changes with
`cargo test` + `pnpm typecheck` + `pnpm build`; integration tests exercise the
real backend code paths against the fixture vault.

## Tauri command registry

When adding a `#[tauri::command]`, **also** add it to the
`tauri::generate_handler![…]` list in `src-tauri/src/lib.rs`. Forgetting this
results in a clean `cargo build` but a 500 at runtime from the frontend.

## Currently implemented (end of Phase 1)

Commands wired through the handler:
- `open_vault`, `close_vault`, `current_vault`, `reindex_vault`
- `list_files`, `file_tree`, `read_file`, `write_file`,
  `create_file`, `delete_file`, `rename_file`

Frontend surfaces: pick + open vault → tree explorer → CodeMirror editor with
Ctrl/Cmd-S save → status bar with indexed file count → live tree refresh on
file watcher events.

## Roadmap pointer

Full multi-phase plan lives in the master spec (Arabic). Quick recap of
upcoming phases:

| Phase | Adds                                           |
|------:|------------------------------------------------|
| 2     | Block UUIDs, wiki-links, backlinks, outline    |
| 3     | Live preview + transclusion (`![[note#^block]]`) |
| 4     | Graph view (Pixi.js + Rust force-directed)     |
| 5     | LanceDB + semantic search                      |
| 6     | HDC encoder                                    |
| 7     | GraphRAG (Leiden + hierarchical summaries)     |
| 8     | SSM/Mamba streaming                            |
| 9     | Multimedia ingestion (Whisper, SigLIP, yt-dlp) |
| 10    | MCP server + Aura Control Port (WebSocket)     |
| 11    | Agent workspace + vault optimization           |
| 12    | Infinite canvas                                |
| 13    | Polish + signing + distribution                |
