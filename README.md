# Aura Knowledge Engine

Local-first, AI-native knowledge engine. Plain-Markdown vault on disk; nothing
proprietary. Built on Tauri 2 (Rust core) + Next.js 15 (App Router, static
export) + CodeMirror 6 + Canvas 2D.

## What's in the box

- **Vault management** — open any folder of `.md` / `.markdown` files;
  `<vault>/.aura/aura.db` holds the SQLite index. File watcher mirrors edits
  back into the DB so the UI never goes stale.
- **Block-level model** — every Markdown top-level element gets a UUIDv7;
  optional `^anchor` markers stay stable across reindexes.
- **Wiki links** — `[[Note]]`, `[[Note|alias]]`, `[[Note#Heading]]`,
  `[[Note#^anchor]]`. Auto-healing: a link to a not-yet-created note resolves
  the moment that note lands. `[[` autocomplete inside the editor.
- **Editor view modes** — Source, Live Preview (inline `![[…]]` embed
  widgets), Reading (full `marked` render), Graph (Canvas 2D
  Fruchterman-Reingold layout from the Rust core with pan/zoom/filter/
  click-to-open), Global Query (GraphRAG), Agent workspace, Canvas board.
- **Right-side panel** — Outline + Backlinks + HDC-driven Related notes,
  refreshed on save / watcher events.
- **Search palette** (`Ctrl/Cmd+Shift+F`) — semantic / FTS / hybrid modes.
  Unified 384-dim space, so media files surface alongside text blocks.
- **HDC engine** — 10 000-bit bipolar hypervectors per note. The Related
  panel picks up notes that share *neighbours* as well as words.
- **GraphRAG** — Label-propagation communities + extractive summaries +
  query routing. The AIChat panel returns a compact context payload ready
  for an LLM swap.
- **Streaming SSM** — fixed-memory EMA hidden state behind a Mamba-shaped
  interface. Continuous-mode chat keeps conversational context across turns.
- **Local media** — scan the vault for audio / video / image files; they're
  encoded in the same 384-dim space as text and merged into search.
- **MCP server** — JSON-RPC 2.0 over HTTP on `127.0.0.1`, Bearer-token auth,
  six tools (search, read/write notes, list, backlinks, GraphRAG query).
- **Agent workspace** — HDC-ranked link suggestions + orphan detection;
  one-click append under a `## Related` heading.
- **Infinite canvas** — `.canvas` JSON files, pan/zoom/drag, file + text
  cards, click to open the underlying note.

## Phase status

| Phase | Status | Notes                                          |
|------:|:------:|------------------------------------------------|
| 1     |   ✅   | Foundation: Tauri + Next.js scaffold, vault CRUD |
| 2     |   ✅   | Blocks, wiki-links, backlinks, outline         |
| 3     |   ✅   | Live preview + transclusion (`![[note#^block]]`) |
| 4     |   ✅   | Graph view (Canvas 2D + Rust force-directed)   |
| 5     |   ✅   | FTS5 + hash-feature semantic search            |
| 6     |   ✅   | HDC encoder + neighbourhood-aware Related      |
| 7     |   ✅   | GraphRAG (LPA + extractive summaries)          |
| 8     |   ✅   | Streaming SSM (EMA stand-in)                   |
| 9     |   ✅   | Local-media ingestion (byte+desc stand-in)     |
| 10    |   ✅   | MCP HTTP server                                |
| 11    |   ✅   | Agent workspace                                |
| 12    |   ✅   | Infinite canvas                                |
| 13    |   ✅   | Polish + CI; production signing/sync deferred  |

See [`CLAUDE.md`](./CLAUDE.md) for the per-phase swap checklists (real
ONNX models, Mamba, Whisper/SigLIP, LLM provider, Aura Control Port WS).

## Quick start

```bash
# Install JS deps and verify the frontend
pnpm install
pnpm typecheck
pnpm build              # static export → out/

# Verify the Rust core
cd src-tauri
cargo check
cargo clippy --no-deps -- -D warnings
cargo test              # full sweep against tests/fixtures/sample-vault

# Run the desktop app (needs a display + Linux system deps)
pnpm tauri dev
```

### Linux system dependencies

```
libwebkit2gtk-4.1-dev libsoup-3.0-dev libappindicator3-dev librsvg2-dev patchelf
```

## Tests

Every phase ships integration tests against the real fixture vault under
`tests/fixtures/sample-vault/`. CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml))
runs `cargo clippy -D warnings`, `cargo test`, `pnpm typecheck`, and
`pnpm build` on every push.

## Contributing

See [`CONTRIBUTING.md`](./CONTRIBUTING.md). The short version: vault stays
plain Markdown, tests use real files, no `unwrap()` outside tests, and
every new `#[tauri::command]` must also be wired into
`tauri::generate_handler![…]` in `src-tauri/src/lib.rs`.
