# TRUTH_AUDIT.md

Brutal audit of every claim in `CLAIMS_REGISTRY.md`. Status legend:

- ✅ **WORKS** — exercised and survives the audit.
- ⚠️ **PARTIAL** — happy path works; adversarial / scale / failure paths uncovered weakness.
- ❌ **BROKEN** — the headline claim does not hold under audit.
- 🚨 **FAKE** — the user-facing claim names a feature whose implementation is a stand-in that does not deliver the named behaviour.

Severity is the operator-facing impact if a real user/attacker hit it tomorrow.

---

## Summary card

| Status | Count |
|--------|------:|
| ✅ WORKS | 49 |
| ⚠️ PARTIAL | 12 |
| ❌ BROKEN | 0 |
| 🚨 FAKE | 9 |
| (unverifiable headless) | 2 |

Untested-by-execution claims are noted explicitly below — I refused to ✅ anything that I couldn't observe.

---

## Per-claim verdicts

### Foundation (Phases 1-4)

**C1 — Vault open + indexing CRUD** — ✅ WORKS — `vault_integration::opens_indexes_reads_writes` exercises real fs+libsql round-trip against `tests/fixtures/sample-vault/`. Severity n/a.

**C2 — File watcher mirrors fs changes into DB** — ⚠️ PARTIAL — implementation exists in `core/file_watcher.rs`, spawns a real notify Debouncer thread. No integration test actually triggers a filesystem event and checks DB sync — every test calls `index_one()` directly. Real-world claim unverified. Severity: MEDIUM (regression risk).

**C3 — `<vault>/.aura/aura.db` created on open** — ✅ WORKS — `VaultDb::open` calls `std::fs::create_dir_all(parent)` before `libsql::Builder::new_local(db_path)`. Verified by fixture tests that touch the DB after open.

**C4 — `VaultState::resolve` rejects `..` and absolute paths** — ⚠️ PARTIAL — the headline case is enforced. Audit (`audit_path_safety.rs`) caught 4 holes:
  - `"\0"` and `"a\0b"` (null byte) accepted (`is_ok()=true`). Mitigated downstream because std fs rejects null-byte paths, but defense-in-depth fails at the API boundary.
  - `"."` accepted, resolves to vault root.
  - `"C:\\Users"` accepted on Unix (no backslash-as-separator awareness).
  - Symlinks inside the vault aren't canonicalized; a symlink `<vault>/note.md → /etc/shadow` would be followed on read. **The Local-First Absolute trust assumption leaks through symlink-planting.**
Severity: MEDIUM. Not directly exploitable as a network attack (the vault is opened by the user themselves), but breaks the implicit isolation promise.

**C5 — Migrations idempotent** — ✅ WORKS — `audit_misc_claims::audit_migration_is_idempotent` opens the same vault three times; no errors, file_count stable at 0.

**C6 — Every block gets a UUIDv7** — ✅ WORKS — code path uses `uuid::Uuid::now_v7().to_string()` per block at `core/vault.rs:index_one`. NB: IDs rotate on every reindex (blocks are deleted-and-reinserted). Persistent identifiers require user-typed `^anchor` markers, which IS the documented convention.

**C7 — `^anchor` markers stable across reindexes** — ✅ WORKS — `BlockRow.user_ref` is preserved verbatim and used by `extract_block_by_user_ref` + `resolve_link_target` for `[[#^anchor]]` lookups. Unit + integration tests both cover.

**C8 — Wiki-link parser variants** — ✅ WORKS — `core/link_resolver::scan_wiki_links` parses all four forms; unit tests `parses_aliased_heading_and_block_refs`, `handles_folder_paths_and_multiple_lines`. `audit_misc_claims::audit_wiki_link_handles_unclosed_nested` confirms the nested-unclosed edge case still resolves the inner link.

**C9 — Unresolved-link auto-healing** — ✅ WORKS — `audit_misc_claims::audit_unresolved_link_heals_across_renames` proves a `[[planned]]` link unresolved at first index becomes resolved after a separate file with `title: planned` is indexed later.

**C10 — `[[` autocomplete in editor** — ⚠️ PARTIAL — code exists in `components/editor/extensions/wikiAutocomplete.ts`, wired in `CodeMirrorEditor.tsx`, passes `pnpm typecheck`. **GUI never launched headless** so end-to-end behaviour is unverified. Severity: LOW.

**C11 — Backlinks panel refreshes** — ⚠️ PARTIAL — `refreshKey` state and watcher subscription exist in `page.tsx`. **GUI never launched** so the refresh-on-event flow is code-verified only. Severity: LOW.

**C12 — Live Preview inline embed widgets** — ⚠️ PARTIAL — `transclusion.ts` builds block widgets via CodeMirror's `Decoration.widget`. Same caveat: **GUI never launched.**

**C13 — Reading mode via `marked`** — ⚠️ PARTIAL — `marked` is in `package.json`, `ReadingView.tsx` uses it, typecheck passes. **GUI never launched.**

**C14 — `resolve_embed` resolves file / heading / `^anchor`** — ✅ WORKS — `embed_integration` tests all three modes end-to-end against the fixture, plus the `is_embed` flag distinction is verified.

**C15 — Force-directed layout in Rust (Fruchterman-Reingold)** — ✅ WORKS — `core/graph_engine::layout` implements both repulsive (all-pairs) and attractive (per-edge) forces with cooling. Three unit tests + 2 integration tests + 1 audit cover scale.

**C16 — Layout deterministic via ChaCha8 seed** — ✅ WORKS — `LayoutParams.seed` is passed to `ChaCha8Rng::seed_from_u64`; running twice with the same params produces identical positions (used implicitly by all graph tests, which rely on stable scoring).

**C17 — Layout parallel via rayon** — ✅ WORKS — repulsive step is `(0..n).into_par_iter().map(...).collect()`. Confirmed in source. 2k-node perf test (4M pairs × 50 iter ≈ 200M ops) finishes in 265ms ⇒ parallelism is real.

**C18 — Canvas 2D ≤ ~2k nodes at 60 FPS** — ⚠️ PARTIAL — the claim is rendering-only; Rust **layout** at 2k nodes / 250 iterations costs ~1.3s one-time (extrapolated from `audit_graph_perf`: 265ms @ 50 iter). Rendering 2k circles+lines per frame on Canvas 2D is plausible but unverified because GUI never launched. Wording is ambiguous — readers might assume the full pipeline is 60 FPS. Severity: LOW.

### Search (Phase 5)

**C19 — Three search modes (semantic / FTS / hybrid)** — ✅ WORKS — `core/search::search_blocks` dispatches on `SearchMode`; 4 integration tests + audit. All three paths return non-empty results for fixture queries.

**C20 — FTS5 + bm25 ranking** — ✅ WORKS — `blocks_fts` virtual table is real FTS5; `fts_search` uses `ORDER BY bm25(blocks_fts)`. The literal "Tauri" query against the fixture returns the Roadmap file as expected.

**C21 — Hybrid RRF (k=60)** — ✅ WORKS — `hybrid` in `core/search.rs` uses `1.0 / (HYBRID_RRF_K + idx + 1.0)`; HYBRID_RRF_K is `60.0`. Behaviour matches spec.

**C22 — 384-dim hash-feature embedder, L2 normalized** — ✅ WORKS — `HashEmbedder::encode` produces `vec![0.0f32; 384]`, sums signed contributions, then divides by L2 norm. Unit test `embedding_has_expected_dimension_and_is_unit_norm` checks both.

**C23 — `Ctrl/Cmd+Shift+F` opens palette** — ⚠️ PARTIAL — handler in `page.tsx`, but **GUI never launched.** Severity: LOW.

**C5-CLAIM-FAKE — "Semantic search finds notes by meaning even without keyword overlap"** (Phase 5 Definition of Done) — 🚨 **FAKE in headline sense.** Test `audit_semantic_claim::audit_semantic_without_keyword_overlap`: queried with note A's vocabulary ("morning routine deep work calendar") against three notes (A, B same concept different words, C unrelated) — **only A surfaces, B does not appear at all.** The hash-feature embedder matches lexical, not semantic. The Real-MiniLM swap checklist in CLAUDE.md is the disclosure, but the README/commit prose calls it "semantic" without the caveat. Severity: HIGH — this is a headline feature.

### HDC (Phase 6)

**C24 — 10,000-bit bipolar HVs packed into 1250 bytes** — ✅ WORKS — `Hypervector::to_packed_bytes` returns `vec![0u8; HV_DIM/8]` = 1250 bytes. Unit test `packed_roundtrip` confirms inverse. Integration test asserts 1250-byte payload size.

**C25 — bind / bundle / permute / similarity** — ✅ WORKS — all four operations have dedicated unit tests covering algebraic properties (bind is self-inverse for bipolar, bundle preserves component similarity above chance, permute decorrelates).

**C26 — `from_token` deterministic** — ✅ WORKS — unit test `from_token_is_deterministic_and_distinct` confirms same input → same output.

**C27 — Combined HV bundles text + permuted neighbours** — ✅ WORKS — `encode_note_combined` implementation matches description (`permute(0)` for outgoing, `permute(1)` for incoming). Unit + integration tests + my audit confirm.

**C28 — "Shared neighbours pull HVs together with disjoint text"** — ✅ WORKS — `audit_hdc_claim::audit_find_related_ranks_cluster_above_orphan`: cluster siblings with disjoint vocabularies (`alpha bravo charlie` vs `xylophone yankee zulu`) but both linking to a common Hub get HDC similarity **0.6340** vs **-0.0062** for the orphan. The promise of Phase 6 actually holds.

**C29 — `find_related` returns top-K with shared-neighbours count** — ✅ WORKS — `commands::related::find_related` returns `RelatedNote { score, shared_neighbours, ... }`. Audit test exercises the same code path.

### GraphRAG + SSM + Media (Phases 7-9)

**C30 — LPA deterministic community detection** — ✅ WORKS — uses `ChaCha8Rng::seed_from_u64(seed)` for visit-order shuffle; ties broken by smallest label id. Unit tests for empty / isolated / clique / two-cliques.

**C31 — Extractive summary capped at 1200 chars** — ✅ WORKS — `summarizer::extractive_summary` enforces `SUMMARY_HARD_CAP = 1200`. Unit test `summary_respects_hard_cap` checks with 50 padded members.

**C32 — Compact context_payload + token estimate** — ✅ WORKS — `query_engine::run_query` returns `estimated_tokens = (payload_len / 4).ceil()`, `covered_notes` summed across hit communities. Integration test `context_payload_is_compact_relative_to_full_vault` confirms payload < total vault bytes.

**C33 — Two cliques with bridge edge split correctly under LPA** — ✅ WORKS — `community_detector::tests::two_cliques_split_into_two_communities` runs with 30 iterations and `seed=11`. Test relies on a stable seed; **with a different seed it might flip** (LPA is non-monotonic with bridge edges). Severity: LOW; the partition isn't a hard guarantee.

**C34 — SSM 384-dim hidden, EMA α=0.82** — ✅ WORKS — `StreamingState::new(384)`, `DEFAULT_ALPHA = 0.82`. Unit tests cover all properties.

**C35 — 10,000 steps don't grow state** — ✅ WORKS — explicit unit test `memory_is_fixed_size_regardless_of_step_count` loops 10,000 steps and asserts `hidden.len() == enc.dim()`.

**C36 — Recency: many off-topic steps dominate** — ✅ WORKS — fixed in the regression: a single step doesn't flip, but 40+ steps do. Unit test asserts both bounds.

**C37 — `streaming_chat` fuses state + question** — ✅ WORKS — `compose_query(input, blend)` interpolates, then `run_query_with_fused` uses the fused vector for ranking. Unit test covers the interpolation.

**C38 — Media kinds detected from extension** — ✅ WORKS — `audit_media_claim::audit_media_kind_detection_is_consistent` exercises 7 cases including uppercase, no extension, trailing dot.

**C39 — Media embedding 0.85 text + 0.15 bytes** — ✅ WORKS — code matches.

**C40 — Media hits in unified semantic search** — ⚠️ PARTIAL — backed by `search_blocks::semantic_only` joining `block_embeddings` + `all_media_with_embeddings`. Integration test `ingested_media_shows_up_in_unified_search` proves the merge. **BUT** the encoder is filename-dominated (see C72), so "unified semantic" is real plumbing with weak semantics. Severity: LOW (mechanical correctness is good; semantic quality is the next claim).

**C41 — Tools probe honest** — ✅ WORKS — `ToolsStatus::probe()` correctly reports `None` for missing binaries; verified in the sandbox (yt-dlp/ffmpeg/ffprobe all `None`).

### MCP (Phase 10)

**C42 — MCP bind 127.0.0.1 only** — ✅ WORKS — `TcpListener::bind("127.0.0.1:port")`. (Audit confusion clarified: connecting from a client-side 0.0.0.0 to a 127.0.0.1 server is normal local routing, not a binding leak.)

**C43 — Bearer-token gating on every POST** — ✅ WORKS — `mcp_post_handler` checks `Authorization` header before dispatch.

**C44 — 401 on unauthorised** — ✅ WORKS — `audit_mcp_adversarial::audit_auth_header_variants` confirms 401 for empty/wrong-scheme/lowercase-scheme/wrong-token; 200 only for exact `Bearer <token>`.

**C45 — Token URL-safe base64, ≥40 chars** — ✅ WORKS — `audit_misc_claims::audit_token_actually_high_entropy` generates 1000 tokens, all unique, all `[A-Za-z0-9_-]+`, length 43.

**C46 — initialize / ping / tools/list / tools/call** — ✅ WORKS — unit + integration coverage. `unknown_method_returns_jsonrpc_error` confirms `-32601` for others.

**C47 — Six aura_* tools** — ✅ WORKS — `tools_list_includes_all_seven_tools` (the test name lies — there are six) asserts the catalogue contains all six. (NB: test name says "seven", actual catalogue is six. Cosmetic inconsistency.)

**C48 — Token rotates on every restart** — ✅ WORKS — `start_mcp_server` calls `new_token()` unconditionally; the previous handle is taken and shutdown. Verified via audit `audit_auth_token_rotates_on_restart`.

**C49 — `aura_read_note` returns real content** — ✅ WORKS — `mcp_integration::read_note_tool_returns_file_contents` over real HTTP returns the Welcome.md body. Path-traversal is blocked at the MCP layer (`audit_mcp_adversarial::audit_tool_call_path_traversal_is_blocked` — returns `-32602 "path is outside the active vault"`).

### Agent + Canvas (Phases 11-12)

**C50 — Suggestions skip already-linked pairs** — ✅ WORKS — `agent_integration::suggestions_skip_already_linked_pairs` confirms with fixture's Daily→Welcome pair.

**C51 — Suggestions sorted desc by score** — ✅ WORKS — `agent_integration::suggest_links_proposes_unlinked_related_notes` verifies monotone descending.

**C52 — `find_orphan_notes` returns truly disconnected notes** — ✅ WORKS — `agent_integration::find_orphans_returns_truly_disconnected_notes` proves both empty-set (fixture) and post-injection (Orphan.md).

**C53 — `apply_link_suggestion` appends under `## Related` and reindexes** — ⚠️ PARTIAL — implementation appends `- [[target]]` and calls `vault.index_one(&abs)`. **No integration test exercises the apply path end-to-end** (no test asserts the file is mutated + reindexed). Code-verified only. Severity: LOW.

**C54 — Canvas validates dangling edges + duplicate ids** — ✅ WORKS — `core/canvas::validate` catches both; unit tests cover.

**C55 — `.canvas` files are vault-native** — ✅ WORKS — `commands/canvas` uses `VaultState::resolve` for every path; integration test `canvas_walker_finds_canvas_files_only` confirms the walker discriminates.

### Polish (Phase 13)

**C56 — `Ctrl/Cmd+N` opens new-note prompt** — ⚠️ PARTIAL — handler wired in `page.tsx`, suppressed inside form inputs. **GUI never launched.** Severity: LOW.

**C57 — Last vault + view mode persisted in localStorage** — ⚠️ PARTIAL — code exists, persistence keys defined. **GUI never launched, localStorage never observed in practice.** Severity: LOW.

**C58 — CI runs on push** — ⚠️ PARTIAL — `.github/workflows/ci.yml` is syntactically valid (no schema validation run; assumed correct). **Workflow has never actually executed** in this sandbox — would only verify on push to GitHub. Severity: LOW.

### Honesty markers + cross-cutting

**C59 — No `unwrap()` outside tests** — ❌ **VIOLATED** in two places:
  - `core/hdc/hypervector.rs:24`: `Bernoulli::new(0.5).unwrap()` — unreachable in practice (0.5 is always valid) but technically a violation.
  - `lib.rs:87`: `.expect("error while running tauri application")` — standard Tauri entry pattern but is a non-test panic point.
  Severity: LOW. Either could be replaced with explicit error handling.

**C60 — 98/98 tests pass** — ❌ **NUMBER WRONG.** Actual count: **97 tests** pass. The math in commit messages was off by 1 from Phase 11 onward. All tests do pass; the headline is exaggerated by one. Severity: LOW (honesty marker).

**C61 — `cargo clippy --no-deps -- -D warnings` clean** — ✅ WORKS — re-run, zero warnings.

**C62 — `pnpm typecheck` clean** — ✅ WORKS — re-run, zero errors.

**C63 — `pnpm build` static export succeeds** — ✅ WORKS — verified at 322kB First Load JS.

**C64 — Every `#[tauri::command]` registered** — ✅ WORKS — 40 declared, 40 registered (`diff /tmp/declared.txt /tmp/registered.txt` empty).

**C65 — Real fixture vault, no in-memory mocks** — ✅ WORKS — every integration test starts from `copy_fixture_to_temp()`; inline `fs::write` targets real paths; no `:memory:` SQLite usage anywhere.

**C66 — Word count excludes Markdown syntax** — ✅ WORKS — `audit_misc_claims::audit_word_count_excludes_markdown_syntax` confirms 6 visible words from a doc whose naive whitespace split would over-count.

**C67 — Wiki-link nested-unclosed handled** — ✅ WORKS — `audit_misc_claims::audit_wiki_link_handles_unclosed_nested` proves only `[[Real]]` is captured.

### The big stand-in disclosures

**C68 — Production signing/sync/marketplace deferred** — ✅ HONESTLY DISCLOSED — CLAUDE.md Phase 13 internals lists exactly what's missing and why.

**C69 — Phase 5 ONNX MiniLM stand-in** — 🚨 STAND-IN ACKNOWLEDGED — code is honest (CLAUDE.md "Real-MiniLM swap checklist"); README/commit chatter calls it "semantic search" without the caveat (see C5-CLAIM-FAKE).

**C70 — Phase 7 Leiden + LLM stand-in** — 🚨 STAND-IN ACKNOWLEDGED — LPA in place of Leiden, extractive summary in place of LLM summary. Disclosed in CLAUDE.md.

**C71 — Phase 8 Mamba ONNX stand-in** — 🚨 STAND-IN ACKNOWLEDGED — EMA in place of Mamba. The user-visible property (fixed-memory streaming, recency under repeated steps) genuinely holds, but the SSM family member shipped is much simpler than the spec's headline.

**C72 — Phase 9 Whisper/SigLIP + yt-dlp stand-in** — 🚨 STAND-IN ACKNOWLEDGED — confirmed by audit: `audit_media_claim` shows the encoder is filename-dominated (cosine 0.97 between same-name files with radically different bytes). The Phase 9 Definition of Done — "find AC clicking sound video" — is **architecturally impossible** without the real models.

### Critical gap

**C-NEVER-LAUNCHED — The Aura GUI has never been launched in this sandbox.** — 🚨 — Tauri binary panics at startup with `Failed to initialize GTK` (no display). Every UI-only claim (Source / Live Preview / Reading / Graph / Canvas / AIChat / Agent / Integrations panels; `Mod+S`, `Mod+N`, `Mod+Shift+F` shortcuts; tree refresh on watcher events; embed widget rendering; settings persistence) is **code-verified only**. Type checking and the static export prove the JavaScript bundle compiles; they do not prove anything about runtime behaviour. Severity: HIGH for end-user readiness; LOW for engineering soundness (the code paths are exercised by integration tests on the Rust side).

---

## Severity breakdown

| Severity | Findings |
|----------|----------|
| HIGH | C5-CLAIM-FAKE (semantic search headline is fake); C-NEVER-LAUNCHED (GUI never run) |
| MEDIUM | C4 (path safety holes: null bytes, symlinks); C2 (file watcher untested end-to-end) |
| LOW | C18, C33, C53, C56-C58, C59, C60, C47-cosmetic |

---

## PHASE 5 — Final verdict

**Total claims tested**: 72 + cross-cutting

**✅ Real and working**: 49 — vault model, libsql, FTS5, Bearer-auth MCP, HDC math, LPA detection, SSM math, agent suggestions, canvas IO, idempotent migrations, deterministic tokens, fixture-driven tests.

**⚠️ Partial**: 12 — UI features unverified at runtime; file-watcher e2e not exercised; apply-link not e2e-tested; CI never actually run.

**❌ Broken**: 0 — nothing claimed-and-working flat-out fails.

**🚨 Fake / mocked claims**: 9 — semantic search (acknowledged stand-in, but headline calls it "semantic"), Whisper/SigLIP/yt-dlp (acknowledged), real Mamba (acknowledged), real LLM in GraphRAG (acknowledged), real ONNX MiniLM (acknowledged), the Phase 9 DoD scenario, the headline "98/98 tests" (97 actual), 2× `unwrap()`/`.expect()` outside tests violating Hard Rule #4, and the GUI never-launched gap.

### "If I launched this to paying customers TODAY, what would break first, and how badly?"

**Order of failure, from soonest to latest:**

1. **The GUI might not start at all on Wayland-only or unusual Linux distros.** No one has actually launched the binary anywhere with a display attached in this work. Tauri 2 + GTK on Linux is sensitive to system libraries; the typecheck-and-static-export pipeline catches nothing of this. On macOS / Windows we'd be relying entirely on Tauri 2's bundling working as advertised, with **no signing certificates configured** — so users would hit gatekeeper / SmartScreen warnings on first run. **Severity: project-killing for end users; trivially fixable with one staging machine.**

2. **The first customer who types "ideas about productivity" expecting Obsidian/Notion-level semantic search will be disappointed.** Hash-feature embedding only matches vocabulary, not concepts. The screen says "Semantic" in the search palette; the result set will tell the customer otherwise within five queries. The Real-MiniLM swap is one file and exists as a documented seam — but it has not been done.

3. **A power user who creates 5k+ notes will see the Rust force-directed layout take ~5s at default `iter=250`**. Acceptable as a one-time "compute" but no progress UI, no chunked yielding. The screen will appear frozen.

4. **A vault under git with active branching might confuse the file watcher** when git resets paths in bulk — re-indexing 500 files in one debounced burst could spike CPU and DB writes. Untested at scale.

5. **A malicious extension or a user who accidentally `ln -s /etc/shadow .aura/note.md` could trick the app into reading outside the vault.** Defense-in-depth weakness, not directly remotely exploitable.

6. **The MCP endpoint is solid**: Bearer-token auth, loopback-only bind, path traversal blocked, six tools that wrap real core functions. A Claude Code-style client could use it tomorrow without surprises. This is the one feature most likely to delight a paying customer.

7. **The AIChat "Global Query" panel, when fed a customer's real question, returns concatenated leading paragraphs of community members, not an LLM-generated answer.** Customers will see `## Theme 1 (notes: 3, score: 0.42)` followed by bullet-pointed first-paragraph extracts. The infrastructure is right; the natural-language layer is the LLM swap that hasn't been wired.

**Bottom line:** The Rust core, MCP server, search/indexing/HDC/GraphRAG plumbing are production-quality engineering on the things they actually do. **The user-facing AI claims — "semantic search", "Mamba-style streaming", "find sound by content", "GraphRAG that answers questions" — are stand-ins.** A customer who reads the README in the order it presents itself will hit the gap between "AI-native knowledge engine" and the literal behaviour within minutes of trying it. The code is built so each stand-in swap is a one-file change; until those swaps are done, the project ships as a *very polished local note manager with an MCP endpoint and an honest stand-in scaffold for the AI layer*, not as the AI-native engine the README headline implies.
