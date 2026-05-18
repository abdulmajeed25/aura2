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

## Currently implemented (end of Phase 11)

Commands wired through the handler:
- `open_vault`, `close_vault`, `current_vault`, `reindex_vault`
- `list_files`, `file_tree`, `read_file`, `write_file`,
  `create_file`, `delete_file`, `rename_file`
- `get_backlinks`, `get_outgoing_links`, `get_outline`,
  `list_link_candidates`
- `resolve_embed`: file / heading-section / `^anchor` block
- `get_graph_snapshot`: positioned `GraphNode`s + edges
- `search_vault`: semantic / FTS / hybrid block search,
  **merged with media hits in the unified 384-dim space**
- `find_related`: HDC-ranked related notes for any note
- `rebuild_graph_rag`, `graph_rag_query`: community detection +
  extractive summaries + query routing
- `ssm_status`, `ssm_reset`, `ssm_step_text`, `streaming_chat`:
  recurrent streaming state + state-fused chat
- `media_tools_status`, `ingest_media`, `scan_media`, `list_media`,
  `delete_media`: local media ingestion + tool probe
- `start_mcp_server`, `stop_mcp_server`, `mcp_status` (Phase 10):
  bind / shutdown / inspect the MCP HTTP endpoint
- `suggest_links`, `find_orphan_notes`, `apply_link_suggestion`
  (Phase 11): HDC-driven link proposals + orphan detection +
  one-click append-to-source

Frontend surfaces: pick + open vault → tree explorer → editor with view
modes (Source / Live Preview / Reading / Graph / Global Query) plus Agent
and Integrations sidebar panels → `Mod+S`
save → wiki-link decoration with Ctrl/Cmd-click navigation → `[[`
autocomplete → inline `![[…]]` embed widgets in Live Preview → fully
rendered Reading mode (via `marked`) → interactive Canvas-2D graph with
pan/zoom/filter/click-to-open → AIChat panel with "Rebuild index",
**Continuous Mode** toggle (Mamba-style SSM streaming), state saturation
bar, reset-state button, and multi-turn transcript → AgentWorkspace
with Suggested Links + Orphans tabs and one-click Apply → Integrations
panel exposing the MCP endpoint URL, Bearer token and curl example →
right-side panel
with Outline + Backlinks + HDC Related → status bar with indexed file
count and current mode → live tree refresh on watcher events → `⇧⌘F`
opens the Search palette.

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

## Phase 6 internals

- Migration 004 adds `note_text_hvs(file_id, dim, hv_packed, content_hash,
  indexed_at)`. Each row stores a single 10,000-bit packed text HV
  (1250 bytes).
- `core::hdc::hypervector::Hypervector` is bipolar (i8 ±1) with `bind`,
  `bundle`, `permute`, `similarity`, and packed (de)serializers.
  `from_token` seeds `ChaCha8Rng` via FNV-1a so the same token always
  produces the same HV across runs and machines.
- `core::hdc::encoder::encode_text` tokenises text the same way the
  Phase 5 embedder does, builds unigram HVs and `permute`-ordered bigram
  HVs, and bundles them. Document HV depends only on content.
- `core::hdc::graph_encoder::encode_note_combined` bundles the text HV
  with `permute(0)`/`permute(1)` of neighbour identity HVs for outgoing
  vs incoming edges respectively. This means a note can be HDC-similar
  to another because they share *neighbours* (graph topology) even when
  they share no vocabulary — the key property the Phase 5 hash embedding
  cannot capture.
- `find_related` computes the query note's combined HV, then in parallel
  (`rayon`) scores every other note's combined HV by cosine similarity.
  `RelatedNote.shared_neighbours` is reported alongside the score so the
  UI can show why a note ranked.
- Single store: only `text_hv` is persisted. Combined HVs are recomputed
  on every `find_related` call (≤ ~100 ms for ~1k notes) using the live
  link table, so they're never stale.
- Frontend: `components/sidebar/RelatedNotes.tsx` appears in the right
  panel under Backlinks and re-fetches on save / watcher events.

## Phase 7 internals

- Migration 005 adds `communities` and `community_files` tables.
  `communities.embedding` stores a 384-dim embedding of the community's
  textual summary, so a query embeds once and ranks all communities by
  cosine — no per-note scan needed at query time.
- `core::graph_rag::community_detector::detect_communities` runs Label
  Propagation Algorithm (LPA) over the link graph. Each node starts
  with a unique label and adopts the most-common label among its
  neighbours; ties broken by smallest label id, visit order seeded by
  `ChaCha8Rng` so the partition is reproducible. The spec asks for
  Leiden across 4 hierarchical levels; multi-level is the natural
  extension (return a `Vec<HashMap>` from `detect_communities` and
  the summariser / query engine consume it unchanged).
- `core::graph_rag::summarizer::leading_paragraphs` strips frontmatter
  and headings, returning the leading narrative text. The community
  summary is `extractive_summary` over each member's `(title, lead)`,
  hard-capped at 1200 chars so wide communities still summarise in
  bounded space.
- `core::graph_rag::query_engine::run_query` encodes the question with
  the Phase 5 `HashEmbedder` (so we share one embedding space across
  search + GraphRAG), cosine-ranks every community in parallel, and
  assembles a compact `context_payload`. The DTO reports
  `estimated_tokens` and `covered_notes` so the UI can visualise the
  spec's compression headline.
- `commands::graph_rag::rebuild_graph_rag` is the one place that
  combines detection + per-community summarisation + persistence.
  `graph_rag_query` is a thin wrapper around the engine.
- Frontend: `components/ai/AIChat.tsx` is the Phase 7 user surface.
  It deliberately stops at "context payload" — wiring a real LLM call
  is a single replacement of the answer rendering with a streaming
  call to Anthropic/OpenAI/Ollama, sending `context_payload` as the
  system prompt.

## Phase 8 internals

- `core::ssm::StreamingState` holds a single 384-dim recurrent hidden
  vector + step count + last-alignment scalar. `step(input)` runs the
  EMA update `h' = normalise(α·h + (1-α)·x)` (default α = 0.82), so the
  state has fixed memory regardless of how many turns have been
  processed — the property Mamba targets.
- `compose_query(input, blend)` returns a unit-norm interpolation of
  the current state and the freshly-encoded input. `streaming_chat`
  uses it as the retrieval embedding so the conversation's accumulated
  context biases ranking, not just the latest message.
- The Tauri-level `AppState` carries an `Arc<Mutex<Option<StreamingState>>>`
  beside the vault. There's a single active session at a time;
  `ssm_reset` zeroes it.
- `commands::streaming::streaming_chat` runs the SSM step first, then
  reuses the GraphRAG community index but with the fused query vector
  rather than re-encoding the bare question. Falls back to the
  encoder-only path when the index or fused vector is empty.
- Frontend: `components/ai/AIChat.tsx` shows the **Continuous Mode**
  toggle, a saturation progress bar driven by `SsmStatus.saturation`,
  the `last_input_alignment` ("how surprising the input was"), and a
  Reset state button that also clears the on-screen transcript. The
  multi-turn transcript only displays past Q+A pairs — the actual
  conversational memory lives in the SSM hidden state, fixed-size.

## Phase 11 internals

- `core::agent::suggestions::compute_suggestions` walks every
  (source, target) pair of notes that are not already linked, scores
  them by HDC combined-HV similarity (the same encode_note_combined
  pipeline find_related uses), and returns the top-K per source then
  the global top-N. `shared_neighbours` is reported alongside the
  score so the UI can show *why* a pair ranked.
- `core::agent::optimization::find_orphans` returns every note that
  has neither incoming nor outgoing resolved links.
- `commands::agent::apply_link_suggestion` appends `- [[target]]`
  (with optional `|alias`) under a `## Related` heading at the end
  of the source file, creating the section if needed. The file is
  reindexed immediately so backlinks reflect the change. Full
  transactional undo across multiple files is deferred — for now the
  user reverts via standard text-undo or git.
- Frontend `components/ai/AgentWorkspace.tsx` is a two-tab panel
  (Suggested Links / Orphans). Each suggestion shows source → target,
  score, shared-neighbours count, and an Apply button that calls
  `apply_link_suggestion` and marks the row done. A new "Agent"
  sidebar mode opens the workspace.

## Phase 10 internals

- `protocols/auth.rs`: SHA-256-mixed entropy from `Instant`, system time,
  UUIDv7, PID, and thread id → 256-bit URL-safe base64 token. Trivial to
  copy-paste; collisions are infeasible.
- `protocols/mcp.rs`: pure-data JSON-RPC dispatcher. Implements
  `initialize`, `ping`, `tools/list`, `tools/call`. The six aura_* tools
  thinly wrap the existing core functions: `aura_search`, `aura_read_note`,
  `aura_write_note`, `aura_list_notes`, `aura_get_backlinks`,
  `aura_graph_rag_query`. Tool results are returned both as MCP-style
  `content[].text` (pretty JSON for LLM consumption) and as
  `structuredContent` (typed payload for programmatic clients).
- `protocols/server.rs`: axum router bound to `127.0.0.1` only. Every
  POST `/mcp` request is gated on a `Authorization: Bearer <token>`
  header. Health endpoint at GET `/health`. Shutdown is graceful with a
  500ms deadline followed by an abort — keepalive doesn't hang test
  teardown.
- `AppState` gains `Arc<Mutex<Option<McpServerHandle>>>`. Each
  `start_mcp_server` call freshly generates a token and shuts down any
  previous instance first; stale tokens never work.
- Frontend: `components/settings/Integrations.tsx` renders the endpoint
  URL, Bearer token (with copy-to-clipboard), request counter, and a
  curl example. A multimedia-tools section reports yt-dlp / ffmpeg /
  ffprobe presence so the user can see why URL ingestion is disabled.

### Aura Control Port (WebSocket) — deferred

The master spec lists a second control surface — JSON-RPC over WebSocket
at `127.0.0.1:47821` with per-agent permission scopes. Same dispatch
target as MCP, different transport + auth model (first-message
authentication, long-lived connections, granular scope). The
`mcp_dispatch` function is already the natural seam; wiring it under a
`/aura` WebSocket route is a localised follow-up. CLAUDE.md gets that
checklist when we get there.

## Phase 9 internals

- Migration 006 adds `media_files(id, path, kind, size_bytes,
  duration_ms?, description, embedding, dim, indexed_at)`. Path is
  vault-relative and UNIQUE.
- `core::multimedia::detect_kind` recognises audio (mp3/wav/ogg/flac/
  m4a/aac/opus), video (mp4/mov/mkv/webm/avi), and image (png/jpg/
  jpeg/gif/webp/bmp/svg) extensions.
- `core::multimedia::encode_media` produces a 384-dim vector in the
  same space as text. The recipe: `HashEmbedder(description) * 0.85 +
  unit_norm(byte_fingerprint) * 0.15`, then L2-normalised. The text
  half dominates so retrieval ranks media by name + folder + kind
  (the user-typed words); the byte half differentiates duplicates and
  resists trivial renames.
- `byte_fingerprint` projects fixed-position byte windows (leading
  4KB, trailing 4KB, optional interior) through SHA-256 seeds into the
  384-d slot space. Constant cost regardless of file size.
- `core::multimedia::tools::ToolsStatus::probe()` shells out to
  `yt-dlp --version` / `ffmpeg -version` / `ffprobe -version` so the
  frontend can disable URL ingestion when they're missing (the sandbox
  case).
- Unified search: `core::search::semantic_only` now scores text blocks
  AND media rows under the same query embedding, merges, and reranks.
  Media hits carry `block_type = "media:audio" | "media:video" |
  "media:image"` so the UI can render them with a kind icon.
- Frontend: a "Scan media" button in the sidebar walks the vault for
  media extensions and ingests them via `scan_media`. The
  SearchPalette shows kind icons inline.

### Whisper / SigLIP / yt-dlp swap checklist

The byte+description encoder is a stand-in. To wire in the real
multimodal stack:
1. Add `ort` + `tokenizers` + (optional) `image` / `hound` to
   `Cargo.toml`.
2. Implement a `MediaEncoder` trait alongside `TextEncoder` and back
   it with Whisper-tiny (audio → text → embedding) and SigLIP-small
   (image → embedding directly). Both target dim=384 to share the
   space.
3. For video: extract keyframes via `ffmpeg`, encode each with SigLIP,
   then `Hypervector::bundle` them — Phase 6's HDC tooling is already
   available.
4. For URL ingestion: surface the `ToolsStatus` checked status to the
   UI and only enable a "Paste YouTube URL" affordance when both
   `yt-dlp` and `ffmpeg` are present; on submit, call `yt-dlp -x` for
   the audio and pass the resulting WAV to the audio encoder.
5. Re-run `scan_media`. No schema change is required.

### Real-Mamba swap checklist

`StreamingState` is the same trait surface a real Mamba-130M ONNX
runtime would expose (`step`, `reset`, `compose_query`). To wire in
the production runtime:
1. Add `ort` + `tokenizers` to `Cargo.toml`.
2. Create `core::ssm::MambaState` that loads the ONNX model once and
   stores `hidden_state` per the model's `d_state × d_model` shape.
3. Replace `state.ssm` slot's type with a `Box<dyn StreamingBackend>`
   trait object, default to the EMA stand-in, allow switching to Mamba
   at startup.
4. No frontend changes required — the wire format stays identical.

### LLM swap checklist

The pipeline produces a ready-to-consume `context_payload`. To turn
that into a natural-language answer:
1. Add an `anthropic-sdk-rust` (or equivalent) dependency.
2. Create `core::ai::providers` with an `AIProvider` trait
   (`fn summarize(&self, prompt: &str) -> String`).
3. In `AIChat.tsx`, after `graphRagQuery` resolves, send the answer's
   `context_payload` and the user's `question` to a new
   `commands::ai::generate_answer` that calls the provider.
4. No schema or core changes are required.

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
| ✓ 6   | HDC encoder (text + neighbourhood-aware Related panel) |
| ✓ 7   | GraphRAG (LPA + extractive summaries; LLM swap pending) |
| ✓ 8   | Streaming SSM (EMA stand-in; Mamba ONNX swap pending) |
| ✓ 9   | Local-media ingestion (byte+desc stand-in; ONNX/yt-dlp swap pending) |
| ✓ 10  | MCP HTTP server (Control Port WS deferred)     |
| ✓ 11  | Agent workspace (HDC link suggestions + orphans; full undo deferred) |
| 12    | Infinite canvas                                |
| 13    | Polish + signing + distribution                |
