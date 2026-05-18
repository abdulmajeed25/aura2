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
- Phase 1 schema: `files`. Phase 2 added `blocks` and `links` (002).
- Future phases will add `tags`, `blocks_fts`, vector indexes in LanceDB, etc.

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

## Currently implemented (end of Phase 5)

Commands wired through the handler:
- `open_vault`, `close_vault`, `current_vault`, `reindex_vault`
- `list_files`, `file_tree`, `read_file`, `write_file`,
  `create_file`, `delete_file`, `rename_file`
- `get_backlinks`, `get_outgoing_links`, `get_outline`,
  `list_link_candidates`
- `resolve_embed`: file / heading-section / `^anchor` block
- `get_graph_snapshot`: positioned `GraphNode`s + edges
- `search_vault` (Phase 5): semantic / FTS / hybrid block search

Frontend surfaces: pick + open vault → tree explorer → editor with four view
modes (Source / Live Preview / Reading / Graph) → `Mod+S` save → wiki-link
decoration with Ctrl/Cmd-click navigation → `[[` autocomplete → inline
`![[…]]` embed widgets in Live Preview → fully rendered Reading mode (via
`marked`) → interactive Canvas-2D graph with pan/zoom/filter/click-to-open →
right-side panel with Outline + Backlinks → status bar with indexed file
count and current mode → live tree refresh on watcher events → `⇧⌘F` opens
the Search palette (hybrid / semantic / fts modes, debounced, arrow-key
navigation, ↵ to open).

## Phase 2/3 internals

- `core::markdown_parser::extract_blocks` walks pulldown-cmark's offset
  iterator and emits one `ParsedBlock` per top-level Markdown element
  (Paragraph, Heading, BlockQuote, CodeBlock, List, HtmlBlock, Table,
  FootnoteDefinition).
- `core::markdown_parser::extract_section_by_heading` returns the slice
  starting at a matched heading and continuing until the next heading at the
  same or higher level — backs the `![[file#Heading]]` embed.
- `core::markdown_parser::extract_block_by_user_ref` returns the block whose
  trailing `^anchor` matches — backs `![[file#^anchor]]`.
- `core::link_resolver::scan_wiki_links` is a hand-written scanner for
  `[[target#heading|alias]]` and `[[target#^block-ref|alias]]`. The `!`
  prefix is detected and surfaced as `RawWikiLink.is_embed`, which maps to
  link_type `embed`/`embed_block`/`embed_heading` in the DB.
- `VaultDb::resolve_link_target` tries exact path with `.md`/`.markdown`,
  then basename match across folders, then case-sensitive title.
- `VaultDb::reresolve_unresolved_links` reruns resolution every time a file
  is added — links to not-yet-created notes heal automatically.
- `BlockRow.user_ref` stores trailing `^anchor` markers.
- Frontend: `lib/embeds.ts` (`scanEmbeds`, `expandEmbeds` with depth cap to
  break cycles), `lib/markdown.ts` (Marked instance with a custom
  `wikiLink` tokenizer), `components/editor/ReadingView.tsx`, and
  `extensions/transclusion.ts` (block widgets after each `![[…]]` line).

## Phase 4 internals

- `core::graph_engine::compute_graph` runs Fruchterman-Reingold over every
  resolved link, parallelising the all-pairs repulsive step with `rayon`.
  O(n²) per iteration; fine for ≤ a few thousand notes. A Barnes-Hut
  quadtree is the natural drop-in for ≥10k nodes — the `layout` function
  is the single place to swap.
- `LayoutParams` is deterministic via `ChaCha8Rng` seed so reloading the
  same vault produces the same layout (no jitter in the UI).
- `VaultDb::fetch_graph_nodes_and_edges` only returns resolved edges
  (`is_resolved = 1 AND target_file_id IS NOT NULL`) so orphan links don't
  pollute the graph.
- Frontend `components/graph/GraphView.tsx` is plain Canvas 2D with manual
  hit-testing, pan/zoom, and a filter input. Clicks on a node call
  `editorStore.openFile`. The Pixi.js dependency the master spec lists is
  deferred — Canvas 2D handles typical vault sizes (≤ ~2k nodes) at 60 FPS
  and keeps the bundle small.

## Phase 5 internals

- Migration 003 adds `block_embeddings` (block_id → blob+dim+content_hash)
  and a `blocks_fts` FTS5 virtual table backing the keyword search.
- `core::embeddings::HashEmbedder` is a 384-dim feature-hashed encoder
  over unigrams + bigrams (FNV-1a hash for sign+index, L2-normalised).
  It implements the `TextEncoder` trait; swapping in an ONNX-backed
  `all-MiniLM-L6-v2` later changes one file. We use 384 dims so the
  schema doesn't need to migrate when that swap happens.
- `core::search` exposes three modes:
  - `Semantic`: encode query → cosine similarity vs every stored
    embedding (parallel `rayon`).
  - `Fts`: SQLite FTS5 `MATCH` with `bm25` ordering. Query tokens are
    escaped and prefix-matched (`"term"*`) with implicit AND.
  - `Hybrid`: reciprocal rank fusion (`1/(k+rank)`) over the FTS and
    semantic result lists; `k=60`.
- `VaultState::index_one` now writes embeddings + FTS rows alongside
  blocks, so a single index pass keeps three tables in sync.
- Frontend: `components/search/SearchPalette.tsx` debounces typing,
  exposes a hybrid/semantic/fts toggle, and supports arrow-key
  navigation + ↵ to open. Bound to `Ctrl/Cmd+Shift+F`.

### Real-MiniLM swap checklist

The Hash embedder is a stand-in. When a real ONNX `all-MiniLM-L6-v2`
becomes available, the swap is mechanical:
1. Add `ort` + `tokenizers` to `Cargo.toml`.
2. Create `core::embeddings::OnnxMiniLmEncoder` implementing
   `TextEncoder` with dim=384.
3. Replace `HashEmbedder::new()` references in `core::vault` and
   `core::search` with the new encoder.
4. Run `reindex_vault` to repopulate `block_embeddings`.
5. No schema change is required.

## Roadmap pointer

Full multi-phase plan lives in the master spec (Arabic). Quick recap of
upcoming phases:

| Phase | Adds                                           |
|------:|------------------------------------------------|
| ✓ 2   | Block UUIDs, wiki-links, backlinks, outline    |
| ✓ 3   | Live preview + transclusion (`![[note#^block]]`) |
| ✓ 4   | Graph view (Canvas 2D + Rust force-directed)   |
| ✓ 5   | FTS5 + hash-feature semantic search (ONNX swap pending) |
| 6     | HDC encoder                                    |
| 7     | GraphRAG (Leiden + hierarchical summaries)     |
| 8     | SSM/Mamba streaming                            |
| 9     | Multimedia ingestion (Whisper, SigLIP, yt-dlp) |
| 10    | MCP server + Aura Control Port (WebSocket)     |
| 11    | Agent workspace + vault optimization           |
| 12    | Infinite canvas                                |
| 13    | Polish + signing + distribution                |
