# Stand-in Registry

Every module that ships with a stand-in must appear here with: (a) the
real target, (b) the user-visible gap the stand-in creates, (c) the
phase where the swap is scheduled, and (d) the **swap checklist** so
the replacement is a one-file change.

Marked in code with the comment `// STANDIN: <real target>`. Grep for
that marker in `src-tauri/src` to find every live stand-in.

## Status legend

- 🟢 **REAL** — full implementation, no stand-in.
- 🟡 **STANDIN** — working code, but the named feature isn't fully delivered.
- 🔴 **MISSING** — neither real nor stand-in; phase not yet started.

## Inventory

| # | Module | Real target | Current | Status | Phase | User-visible gap |
|--:|--------|-------------|---------|--------|------:|------------------|
| 1 | Embeddings | `all-MiniLM-L6-v2` ONNX (English) + multilingual encoder (100+ languages incl. Arabic) | **Phase 5a + 5a-ii DONE for English; 5d ships the multilingual seam.** `core/embeddings_onnx/`: multi-model registry (`MODELS`) with two entries — `all-MiniLM-L6-v2` (real, tested, +0.10 paraphrase margin in sandbox) and `multilingual-e5-small` (registry + download + `OnnxMultilingualE5` embedder class + `pick_encoder` priority — but `tract 0.21::into_optimized()` rejects the spectra-e5 quantized ONNX with `Failed analyse for node #564 "/Unsqueeze" AddDims`, so the default download URL produces a model that fails to load at runtime). Tauri commands `embeddings_model_status` (lists every registered model with installed/missing + `active_when_reopened`), `download_embeddings_model(model_name)` (defaults to `"multilingual-e5-small"`). `VaultState::open::pick_encoder` priority: multilingual-e5 → MiniLM → HashEmbedder, with `tracing::info!`/`warn!` lines making every fallback visible. **Workaround for Arabic users today:** drop a tract-compatible (non-quantized) multilingual ONNX into `<vault>/.aura/models/multilingual-e5-small/` — the loader picks it up automatically. | 🟡 | 5 | English works end-to-end via the existing `OnnxMiniLm` path. Multilingual seam is complete but the default download URL has a known tract incompatibility (3 cached-model E5 tests are `#[ignore]` with the tract issue documented). |
| 2 | Communities | Leiden, 4-level hierarchy | **Phase 7a — Leiden in.** `core/graph_rag/leiden.rs` implements full Leiden (local-moving + refinement + aggregation) with modularity tracking per level. `rebuild_graph_rag` uses the coarsest partition the algorithm produced and persists it; intermediate hierarchy levels are computed and exposed via `LeidenResult.levels` for future schema work. 8 math-driven TDD tests including a head-to-head: Leiden's modularity ≥ LPA's on the canonical two-cliques-plus-bridge graph. The old LPA stays available at `community_detector::detect_communities` for backward compat / comparison. | 🟢 | 7 | n/a — closed for partitioning. The 4-level hierarchy persistence (DB schema for multi-level community storage) is a follow-on. |
| 3 | Community summaries | Claude Haiku 4.5 with prompt caching | Extractive concatenation of member titles + leading paragraphs | 🟡 | 7 | "Themes" surfaced as bullet lists of file titles, not natural-language synthesis. |
| 4 | GraphRAG answer | Claude Sonnet 4.6 with cited communities | Concatenated extracts assembled as a "context payload" | 🟡 | 7 | The Global Query panel returns `## Theme 1 (score 0.42)\n• Note A: lead…` not a real answer. |
| 5 | SSM | Mamba-130M INT8 ONNX **OR** Phi-3-mini fallback | EMA streaming, α=0.82, 384-dim hidden | 🟡 | 8 | "Continuous mode" preserves topic but isn't the state-space model the spec headlines. |
| 6 | Media encoder | Whisper-tiny + SigLIP-small ONNX (audio + vision) | 0.85·text(description) + 0.15·byte_fingerprint | 🟡 | 9 | Audit proved encoder is filename-dominated (cosine 0.97 between same-name files with radically different bytes). "Find AC clicking sound" cannot work. |
| 7 | URL ingest | yt-dlp + ffmpeg pipeline | **Phase 9(a) DONE.** `core/multimedia/url_ingest.rs` shells out to `yt-dlp` (downloads + metadata) and `ffprobe` (duration + container). Tauri command `ingest_url(url, trust_self_signed?)` downloads into `<vault>/Media/inbox/` then runs the existing local-media ingestion. URL validation rejects `file://`, `ftp://`, javascript:, null bytes. End-to-end verified in the build sandbox: 5.5 MB Big Buck Bunny clip from a GitHub raw URL, `duration_ms ≈ 60_000`. | 🟢 | 9 | n/a — closed. User installs `yt-dlp` + `ffmpeg` once (pip / apt); the UI's `media_tools_status` already greys out the URL-paste button when they're absent. |
| 8 | `unwrap()` violations | Proper `Result` handling | **FIXED in Phase 0** — `core/hdc/hypervector.rs:24` now uses raw `rng.gen::<u32>()`; `lib.rs::run()` end now uses `unwrap_or_else` + `eprintln!` + `exit(1)` | 🟢 | 0 | n/a — closed. |
| 9 | GUI launch | Launch + verify panels render | Never launched in the build sandbox (no display). Code-verified only via typecheck + static export. | 🟡 | 1 | Every UI claim (modes, shortcuts, settings persistence, watcher refresh) is unverified at runtime in this environment. |
| 10 | Path safety | Reject null bytes + canonicalize symlinks before reads | **FIXED in Phase 1** — `core/vault.rs::resolve` now rejects `\0`, bare `.`/`./`/`.\\`, canonicalises through symlinks, and verifies the canonical path stays under the canonical vault root. Inner symlinks (those resolving inside the vault) still work. `C:\Users` on Unix is explicitly accepted as a literal filename (portability hazard, not a security one). | 🟢 | 1 | n/a — closed. |
| 11 | Cognitive core | CAN + LSM + Langevin + Hopfield + Hebbian + Free Energy + Curiosity + perpetual loop + Tauri events + reflection writer | **DONE (Phase 11a + 11b + 11c + 15a starter)** — `src/cognition/`: 11 modules including `reflection_writer` (template-only reflections fired on F spike or curiosity dip, atomic `.md` writes under `<vault>/.aura/brain/reflections/YYYY-MM-DD/`). 54 math-driven TDD tests total. Tokio perpetual loop emits `cortex://snapshot`; the start-cortex Tauri command spawns a forwarder that *also* feeds the writer and emits `reflection://written` events. LLM-narrated synthesis (the "what did the cortex actually notice" body text) lands in Phase 15b alongside Claude bindings. | 🟢 | 11 | n/a — closed. Template reflections fire today; LLM narration in Phase 15b. |
| 12 | Hamiltonian fusion | Symplectic leapfrog over LLM-injected `p` + local potential `V(x)` | **Phase 12 kernel in.** `cognition/hamiltonian.rs`: `leapfrog_step(x, p, grad_v, dt)` runs the symplectic three-stage update; `energy(x, p, V)` reports `H = ½‖p‖² + V(x)`. 6 math-driven TDD tests including a 50 000-step harmonic-oscillator run that proves the amplitude (and hence energy) stays bounded — exactly the property Euler can't promise. Integration into `Cortex::tick` and the telemetry harness (the spec's 30-day retention experiment) are a future gate. | 🟡 | 12 | Kernel exists and is proven symplectic; the spec's "Claude-injected momentum + Hopfield potential" fusion in the cortex loop is the next step. |
| 13 | Holographic memory | FHRR via `rustfft` (complex unit-modulus, circular convolution) | **Phase 12 kernel in.** `cognition/holographic.rs`: `FhrrVec` with element-wise complex `bind` / `unbind` / `bundle` / `similarity`. No FFT dep — we represent vectors in the frequency domain directly, so circular convolution reduces to element-wise multiplication and a local `Cmplx { re, im }` struct does the algebra. 8 math-driven TDD tests including a role-filler retrieval proof: `unbind(bind(K1, V1) ⊞ bind(K2, V2), K1)` is closer to V1 than to V2 by ≥0.3 at D=2048. The LanceDB schema columns + the decision to swap or augment bipolar HDC are deferred to the post-telemetry gate. | 🟡 | 12 | FHRR algebra is in code and tested; storage schema + cortex integration come after telemetry. |
| 14 | Neuro-symbolic | VSA inference + ILP engine + Z3 + DSPy-style prompt self-modifier | **Phase 14(a) — VSA + Z3 in.** New `src/reasoning/` module: `vsa_inference.rs` implements bipolar-HDC key-value memory queries (`recover_value`, `recover_key`, `pair_score`) with nearest-neighbour clean-up against a stored vocabulary — 5 hand-verified TDD tests including a 6-pair capacity test at HV_DIM=10_000. `z3_bridge.rs` spawns `sidecars/z3_sidecar.py` (Python `z3-solver` installed via pip) and ships an SMT-LIB-over-stdin/stdout JSON protocol — 4 live tests including SAT recovery (x ∈ {8, 9} for `x>0 ∧ x<10 ∧ x²>50`) and unsat detection. **Still pending:** ILP rule-induction engine and the DSPy-style prompt self-modifier (the latter needs an LLM). | 🟡 | 13 | VSA queries + Z3 bridge work end-to-end; the ILP engine + LLM-driven prompt self-modifier come later. |
| 15 | Memory layer | Mem0 ADD/UPDATE/DELETE/NOOP + Letta sleeptime | Not started | 🔴 | 14 | No fact consolidation across sessions; reflections still per-session. |
| 16 | Agent orchestration | LangGraph sidecar OR Rust workflow runtime + Anthropic Skills + resonance triggers | **Phase 16(a) — loaders + resonance logic in.** New `src/orchestration/` module: `skills` parses Anthropic-Skills directories (`SKILL.md` with YAML frontmatter; aux files enumerated; malformed skills are skipped-with-reason, not load-aborting). `workflows` parses workflow JSON (`{name, description, trigger_vector?, trigger_threshold, steps}`) and exposes `resonant(state)` — cosine of cortex state vs each workflow's trigger vector with a threshold. 11 hand-verified TDD tests. Tauri commands `list_skills` + `list_workflows`. **Pending:** the executor that walks `steps` (dispatches to search / llm / write_note / shell) — needs the LLM provider trait wired (API-key gate). | 🟡 | 15 | Loaders + resonance ready; executor + LLM invocation come once an API key is available. |
| 17 | Prompt caching | Anthropic prompt caching API on every Claude call | No Claude calls wired (no API key in sandbox); cache integration deferred | 🔴 | 5 + 7 | All "AI" features today return deterministic stand-in output. |
| 18 | LLMLingua-2 compression | Python sidecar OR local heuristic compressor | Not started | 🔴 | 12 | Retrieval prompts not compressed; raw chunks sent. (When Claude is wired in Phase 5.) |
| 19 | Hardened MCP | OAuth Resource Server + RFC 8707 + Claw-Chain mitigations | **Phase 19 hardening DONE.** Bearer + 127.0.0.1 + path-traversal block stay. New in this gate: (1) `DefaultBodyLimit::max(10 MB)` so a malicious local process can't OOM the server with a multi-GB payload (413 or BrokenPipe outcome). (2) `WWW-Authenticate: Bearer realm="aura", resource="http://127.0.0.1:<port>/mcp"` on 401 (RFC 6750 + RFC 8707-spirit audience indicator). (3) Constant-time token comparison (XOR-fold), so a probing process can't bisect the token via timing. (4) DNS-rebinding mitigation via Host header check — only `127.0.0.1` / `localhost` / `::1` accepted. (5) Atomic check-then-use on `tool_read_note` / `tool_write_note` via `O_NOFOLLOW` on the final path component (Unix), closing the post-`resolve()` symlink-swap window. Token rotation on restart was already in place since v3 (`new_token()` called fresh each `start_server`). | 🟢 | 10 | n/a — closed. Full RFC 8707 (token issuance via OAuth authorization-server flow) is not implemented because the threat model is local Bearer-on-loopback, not third-party delegation. |

## Honesty disclosure

When a phase has multiple stand-ins (e.g., Phase 5 ships HashEmbedder
**and** has no Claude API key), commit messages and the UI MUST NOT
describe the feature in language that implies the real target is
running. Use phrasing like:

- 🟢 "Semantic search via all-MiniLM-L6-v2 ONNX." (only when REAL)
- 🟡 "Lexical hash-feature search (STANDIN: real MiniLM in Phase 5)." (when stand-in)
- 🔴 "Semantic search not yet shipped." (when missing)

## Swap checklist template

Each module's swap checklist lives in CLAUDE.md as a "Real-X swap checklist"
section. The pattern:

1. Add the real dependency to `Cargo.toml` (or sidecar `requirements.txt`).
2. Vendor model files into `models/` with checksums in `models/manifest.json`.
3. Implement the trait method that the stand-in already exposes.
4. Replace the one constructor call in core (e.g., `HashEmbedder::new()` → `OnnxMiniLm::new()`).
5. Re-run `reindex_vault`.
6. Update this registry: status 🟡 → 🟢.
