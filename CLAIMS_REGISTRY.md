# CLAIMS_REGISTRY.md

Every claim extracted from README.md, CLAUDE.md, code, commit messages, and
session output. Each entry will be cross-tested in TRUTH_AUDIT.md.

| ID  | Claim | Source | Locator |
|----:|-------|--------|---------|
| C1  | Vault open + indexing of `.md` files works end-to-end (CRUD) | README/CLAUDE | "Phase 1" |
| C2  | File watcher mirrors filesystem changes into the DB | CLAUDE.md Phase 1 internals | `core/file_watcher.rs` |
| C3  | `<vault>/.aura/aura.db` SQLite index is created on open | CLAUDE.md State model | `db/sqlite.rs` |
| C4  | `VaultState::resolve` rejects absolute paths and `..` traversal | CLAUDE.md Path safety | `core/vault.rs` |
| C5  | Migrations are idempotent (re-run skipped via `_aura_migrations`) | CLAUDE.md DB | `db/sqlite.rs` |
| C6  | Every Markdown top-level element gets a UUIDv7 block id | README Phase 2 | `core/markdown_parser.rs` |
| C7  | `^anchor` markers stay stable across reindexes | README Phase 2 | `core/markdown_parser.rs` |
| C8  | Wiki-link parser handles `[[Note]]`, `[[Note|alias]]`, `[[Note#Heading]]`, `[[Note#^anchor]]` | README Phase 2 | `core/link_resolver.rs` |
| C9  | Unresolved links auto-heal when target note is created | CLAUDE Phase 2 | `db/sqlite.rs::reresolve_unresolved_links` |
| C10 | `[[` autocomplete suggests indexed files in the editor | README Phase 2 | `components/editor/extensions/wikiAutocomplete.ts` |
| C11 | Backlinks panel updates on watcher events | README Phase 2 | `components/sidebar/Backlinks.tsx` |
| C12 | Live Preview shows inline `![[…]]` embed widgets | README Phase 3 | `components/editor/extensions/transclusion.ts` |
| C13 | Reading mode renders full Markdown via `marked` | README Phase 3 | `components/editor/ReadingView.tsx` |
| C14 | `resolve_embed` returns file / heading-section / `^anchor` block | CLAUDE Phase 3 | `commands/block.rs` |
| C15 | Graph view: Rust force-directed layout via Fruchterman-Reingold | README Phase 4 | `core/graph_engine.rs` |
| C16 | Graph layout is deterministic via ChaCha8 seed | CLAUDE Phase 4 | `core/graph_engine.rs` |
| C17 | Layout is parallel via rayon (all-pairs repulsive) | CLAUDE Phase 4 | `core/graph_engine.rs` |
| C18 | Canvas 2D handles ≤ ~2k nodes at 60 FPS | CLAUDE Phase 4 | `components/graph/GraphView.tsx` |
| C19 | Search modes: semantic / FTS / hybrid | README Phase 5 | `core/search.rs` |
| C20 | FTS5 keyword search with bm25 ranking | CLAUDE Phase 5 | `db/sqlite.rs::fts_search` |
| C21 | Hybrid search uses RRF (k=60) over FTS + semantic ranks | CLAUDE Phase 5 | `core/search.rs::hybrid` |
| C22 | 384-dim hash-feature embedder (unigrams + bigrams, FNV-1a, L2-normalized) | CLAUDE Phase 5 | `core/embeddings.rs` |
| C23 | `Ctrl/Cmd+Shift+F` opens the search palette | CLAUDE Phase 5 | `src/app/page.tsx` |
| C24 | HDC: 10,000-bit bipolar hypervectors stored packed (1250 bytes) | CLAUDE Phase 6 | `core/hdc/hypervector.rs` |
| C25 | Bipolar bind / bundle / permute / similarity operations work | CLAUDE Phase 6 | `core/hdc/hypervector.rs` |
| C26 | HDC `from_token` is deterministic (FNV-1a → ChaCha8Rng) | CLAUDE Phase 6 | `core/hdc/hypervector.rs` |
| C27 | Combined HV bundles text HV + permuted neighbour identities | CLAUDE Phase 6 | `core/hdc/graph_encoder.rs` |
| C28 | "Notes sharing neighbours rank higher even with disjoint text" | CLAUDE Phase 6 | integration test |
| C29 | `find_related` returns top-K by HDC similarity with shared-neighbour count | CLAUDE Phase 6 | `commands/related.rs` |
| C30 | Community detection via Label Propagation (LPA), deterministic | CLAUDE Phase 7 | `core/graph_rag/community_detector.rs` |
| C31 | Per-community extractive summary capped at 1200 chars | CLAUDE Phase 7 | `core/graph_rag/summarizer.rs` |
| C32 | GraphRAG query returns compact context payload with token estimate | CLAUDE Phase 7 | `core/graph_rag/query_engine.rs` |
| C33 | Two cliques with a bridge edge split correctly under LPA | CLAUDE Phase 7 | unit test |
| C34 | SSM streaming state: fixed-size 384-dim hidden, EMA α=0.82 | CLAUDE Phase 8 | `core/ssm.rs` |
| C35 | 10,000 steps don't grow state vector (fixed memory) | CLAUDE Phase 8 | unit test |
| C36 | After many off-topic steps, recent input dominates over the oldest (recency) | CLAUDE Phase 8 | unit test |
| C37 | `streaming_chat` fuses state + question for retrieval | CLAUDE Phase 8 | `commands/streaming.rs` |
| C38 | Media kinds detected from extension (audio/video/image) | CLAUDE Phase 9 | `core/multimedia/mod.rs` |
| C39 | Media embedding = 0.85·text(description) + 0.15·byte_fingerprint | CLAUDE Phase 9 | `core/multimedia/mod.rs` |
| C40 | Media hits appear in unified semantic search alongside text blocks | CLAUDE Phase 9 | `core/search.rs::semantic_only` |
| C41 | yt-dlp/ffmpeg/ffprobe presence probe reports honestly | CLAUDE Phase 9 | `core/multimedia/tools.rs` |
| C42 | MCP server binds 127.0.0.1 only | CLAUDE Phase 10 | `protocols/server.rs` |
| C43 | MCP server gates every POST on `Authorization: Bearer <token>` | CLAUDE Phase 10 | `protocols/server.rs` |
| C44 | MCP server returns 401 to unauthorised requests | CLAUDE Phase 10 | `protocols/server.rs` |
| C45 | Auth token is URL-safe base64, ≥40 chars | CLAUDE Phase 10 | `protocols/auth.rs` |
| C46 | MCP supports initialize / ping / tools/list / tools/call | CLAUDE Phase 10 | `protocols/mcp.rs` |
| C47 | Six aura_* tools exposed (search, read_note, write_note, list_notes, get_backlinks, graph_rag_query) | CLAUDE Phase 10 | `protocols/mcp.rs::tool_catalogue` |
| C48 | Auth token regenerates on every server restart (stale tokens never work) | CLAUDE Phase 10 | `commands/integrations.rs` |
| C49 | MCP `tools/call` for `aura_read_note` returns real file content | README Phase 10 | integration test |
| C50 | Link suggestions skip pairs that are already linked | CLAUDE Phase 11 | `core/agent/suggestions.rs` |
| C51 | Suggestions sorted descending by HDC score | CLAUDE Phase 11 | `core/agent/suggestions.rs` |
| C52 | `find_orphan_notes` returns notes with no incoming AND no outgoing links | CLAUDE Phase 11 | `core/agent/optimization.rs` |
| C53 | `apply_link_suggestion` appends `- [[target]]` under `## Related` and reindexes | CLAUDE Phase 11 | `commands/agent.rs` |
| C54 | Canvas validates dangling edges + duplicate node ids before write | CLAUDE Phase 12 | `core/canvas.rs` |
| C55 | Canvas files have `.canvas` extension and live in vault | README Phase 12 | `commands/canvas.rs` |
| C56 | `Ctrl/Cmd+N` opens new-note prompt (suppressed inside form inputs) | CLAUDE Phase 13 | `src/app/page.tsx` |
| C57 | Last vault + view mode persisted in `localStorage` | CLAUDE Phase 13 | `src/app/page.tsx` |
| C58 | CI workflow runs on push to `main` and `claude/**` | CLAUDE Phase 13 | `.github/workflows/ci.yml` |
| C59 | No `unwrap()` outside test code | Hard Rule #4 | grep target |
| C60 | 98/98 tests pass | session output | full test sweep |
| C61 | `cargo clippy --no-deps -- -D warnings` is clean | session output | clippy run |
| C62 | `pnpm typecheck` clean | session output | tsc run |
| C63 | `pnpm build` produces static export | session output | next build |
| C64 | All `#[tauri::command]` functions are registered in `tauri::generate_handler![…]` | CLAUDE rule | `lib.rs` |
| C65 | Test fixture is a REAL vault (`tests/fixtures/sample-vault/`), not in-memory | Hard Rule #2 | fixture dir |
| C66 | Word count excludes Markdown syntax (only Text/Code events from pulldown-cmark) | CLAUDE Phase 2 | regression discussion |
| C67 | Wiki-link scanner handles `[[never closed and [[Real]]` → finds `[[Real]]` only | CLAUDE Phase 2 | regression discussion |
| C68 | "Production signing/sync/marketplace/landing deferred" — explicitly NOT shipped | CLAUDE Phase 13 | honesty marker |
| C69 | Phase 5 spec promised real ONNX `all-MiniLM-L6-v2` — ships as hash feature stand-in | CLAUDE swap checklist | self-disclosed stand-in |
| C70 | Phase 7 spec promised LLM-summarised + Leiden 4-level hierarchy — ships as extractive + LPA single-level | CLAUDE swap checklist | self-disclosed stand-in |
| C71 | Phase 8 spec promised real Mamba-130M ONNX — ships as EMA recurrent stand-in | CLAUDE swap checklist | self-disclosed stand-in |
| C72 | Phase 9 spec promised Whisper-tiny + SigLIP-small + yt-dlp URL ingestion — ships as byte-fingerprint stand-in, URL ingestion not wired | CLAUDE swap checklist | self-disclosed stand-in |
