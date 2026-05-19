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
| 1 | Embeddings | `all-MiniLM-L6-v2` ONNX (English) + `BGE-M3` (Arabic/multilingual) | **Phase 5a in (real encoder shipped, active-swap follow-on pending).** `core/embeddings_onnx/`: real `OnnxMiniLm` running via pure-Rust `tract-onnx` (no native libs) + `tokenizers`. SHA-256-verified download-on-first-run from GitHub mirror of the upstream Apache-2.0 model into `<vault>/.aura/models/all-MiniLM-L6-v2/`. Tauri commands `embeddings_model_status` + `download_embeddings_model` (with `embeddings://download-progress` event). End-to-end inference verified in the build sandbox: `"I love cats"` vs `"Felines are wonderful"` similarity is +0.10 above the off-topic baseline — the falsifying gap the audit identified is closed. **Still pending:** make `OnnxMiniLm` the active encoder during indexing (6 call sites in `vault.rs`, `search.rs`, `ssm.rs`, `graph_rag/query_engine.rs`, `multimedia/mod.rs`, `related.rs` to thread an `Arc<dyn TextEncoder>` through `AppState`). BGE-M3 (Arabic) is a separate later gate. | 🟡 | 5 | Real semantic encoder + download mechanism shipped, but `HashEmbedder` is still the default during indexing until the active-encoder swap lands. |
| 2 | Communities | Leiden, 4-level hierarchy | LPA single-level (`core/graph_rag/community_detector.rs`) | 🟡 | 7 | No hierarchical zoom-out. Bridge edges can flip the partition under different seeds. |
| 3 | Community summaries | Claude Haiku 4.5 with prompt caching | Extractive concatenation of member titles + leading paragraphs | 🟡 | 7 | "Themes" surfaced as bullet lists of file titles, not natural-language synthesis. |
| 4 | GraphRAG answer | Claude Sonnet 4.6 with cited communities | Concatenated extracts assembled as a "context payload" | 🟡 | 7 | The Global Query panel returns `## Theme 1 (score 0.42)\n• Note A: lead…` not a real answer. |
| 5 | SSM | Mamba-130M INT8 ONNX **OR** Phi-3-mini fallback | EMA streaming, α=0.82, 384-dim hidden | 🟡 | 8 | "Continuous mode" preserves topic but isn't the state-space model the spec headlines. |
| 6 | Media encoder | Whisper-tiny + SigLIP-small ONNX (audio + vision) | 0.85·text(description) + 0.15·byte_fingerprint | 🟡 | 9 | Audit proved encoder is filename-dominated (cosine 0.97 between same-name files with radically different bytes). "Find AC clicking sound" cannot work. |
| 7 | URL ingest | yt-dlp + ffmpeg pipeline | Tool probe only — reports binaries absent, ingestion not wired | 🔴 | 9 | UI greys out URL paste; only local media files can be indexed. |
| 8 | `unwrap()` violations | Proper `Result` handling | **FIXED in Phase 0** — `core/hdc/hypervector.rs:24` now uses raw `rng.gen::<u32>()`; `lib.rs::run()` end now uses `unwrap_or_else` + `eprintln!` + `exit(1)` | 🟢 | 0 | n/a — closed. |
| 9 | GUI launch | Launch + verify panels render | Never launched in the build sandbox (no display). Code-verified only via typecheck + static export. | 🟡 | 1 | Every UI claim (modes, shortcuts, settings persistence, watcher refresh) is unverified at runtime in this environment. |
| 10 | Path safety | Reject null bytes + canonicalize symlinks before reads | **FIXED in Phase 1** — `core/vault.rs::resolve` now rejects `\0`, bare `.`/`./`/`.\\`, canonicalises through symlinks, and verifies the canonical path stays under the canonical vault root. Inner symlinks (those resolving inside the vault) still work. `C:\Users` on Unix is explicitly accepted as a literal filename (portability hazard, not a security one). | 🟢 | 1 | n/a — closed. |
| 11 | Cognitive core | CAN + LSM + Langevin + Hopfield + Hebbian + Free Energy + Curiosity + perpetual loop + Tauri events | **DONE (Phase 11a + 11b + 11c)** — `src/cognition/`: 10 modules (`langevin`, `cans`, `lsm`, `hebbian`, `hopfield`, `free_energy`, `curiosity`, `cortex`, `perpetual_loop`, `shared_cortex`) with 48 math-driven TDD tests. Tokio perpetual loop running at configurable `dt_ms` heartbeat, drains MPSC observations, emits `CortexSnapshot { tick, free_energy, curiosity, dominant_index }`. Tauri-side: 4 commands (`start_cortex`, `stop_cortex`, `send_observation`, `cortex_status`) plus a `cortex://snapshot` event forwarder. Reflection writing (`reflection://written`) and the LLM-narrated synthesis step are scheduled for a later phase (15 — agent orchestration). | 🟢 | 11 | n/a — closed for kernels + perpetual loop. Reflection-write stage is a Phase 15 follow-on. |
| 12 | Hamiltonian fusion | Symplectic leapfrog over LLM-injected `p` + local potential `V(x)` | Not started | 🔴 | 12 | Experimental; ships with telemetry to decide retention. |
| 13 | Holographic memory | FHRR via `rustfft` (complex unit-modulus, circular convolution) | Not started | 🔴 | 12 | Optional FHRR columns in LanceDB schema; bipolar HV is the default path. |
| 14 | Neuro-symbolic | VSA inference + ILP engine + Z3 + DSPy-style prompt self-modifier | Not started | 🔴 | 13 | No derived facts panel, no proof tree, no curated rules. |
| 15 | Memory layer | Mem0 ADD/UPDATE/DELETE/NOOP + Letta sleeptime | Not started | 🔴 | 14 | No fact consolidation across sessions; reflections still per-session. |
| 16 | Agent orchestration | LangGraph sidecar OR Rust workflow runtime + Anthropic Skills + resonance triggers | Not started; the existing v3 agent workspace covers only link-suggestion + orphan-detection | 🔴 | 15 | No multi-step workflows; no Skill loader. |
| 17 | Prompt caching | Anthropic prompt caching API on every Claude call | No Claude calls wired (no API key in sandbox); cache integration deferred | 🔴 | 5 + 7 | All "AI" features today return deterministic stand-in output. |
| 18 | LLMLingua-2 compression | Python sidecar OR local heuristic compressor | Not started | 🔴 | 12 | Retrieval prompts not compressed; raw chunks sent. (When Claude is wired in Phase 5.) |
| 19 | Hardened MCP | OAuth Resource Server + RFC 8707 + Claw-Chain mitigations | Bearer-token + 127.0.0.1 + path-traversal block. Auth + bind are correct; OAuth Resource Server pattern, RFC 8707 audience binding, atomic check-then-use, and request-body size cap are not yet in place. | 🟡 | 10 | A malicious local process with token can still misuse — but token is required, so blast radius is contained. |

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
