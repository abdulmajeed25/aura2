# Aura Cognitive OS — CLAUDE.md

Re-read at the start of every session. Reference [`AURA_V5_MASTER_PROMPT.md`](./AURA_V5_MASTER_PROMPT.md) for the full 42-week spec; this file is the 2-page orientation.

## What this is

Local-first, AI-native knowledge engine **plus** a perpetual subconscious cortex (CAN + LSM + Langevin + Hopfield + Hebbian + Free Energy). Plain `.md` on disk. Tauri 2 + Rust + Next.js 15 + CodeMirror 6 + LanceDB + Tantivy + ONNX Runtime.

## Hard rules (non-negotiable)

1. **Local-First Absolute.** Authoritative data = plain Markdown on disk.
2. **Privacy by Architecture.** Every cognitive loop runs locally; nothing leaves the device without per-operation consent.
3. **No mock data.** Tests run against `tests/fixtures/sample-vault/` (legacy, 3 notes, 97 audited tests) and `tests/fixtures/real-vault/` (v5.0, ≥25 notes, mixed Arabic + English).
4. **No `unwrap()` outside tests.** Production paths return `Result` and handle errors. The two existing violations were fixed in Phase 0 of v5.0.
5. **Vertical completion per module:** DB → core → command → TS wrapper → UI → integration test against real files.
6. **Math-driven TDD** for every numerical kernel (HDC ops, CAN integration, LSM update, Langevin noise, Hebbian, Hamiltonian leapfrog).
7. **STANDIN: marker** for any module shipping with a stand-in. Listed in [`docs/STAND_IN_REGISTRY.md`](./docs/STAND_IN_REGISTRY.md) with a Real-X swap checklist.
8. **Phase gates are sacred.** End every phase with: file paths created/modified, reproduce commands, expected outputs, failures encountered, then `STOP — request "continue"`.
9. **MCP hardened** per Claw Chain CVE lessons (Part 14 of master): OAuth Resource Server, per-request auth, owner from token only, canonicalize paths, no shell heredocs, atomic check-then-use, token rotation on restart.
10. **Honest disclosure** in commit messages and README. Don't write "semantic search" for a feature that's actually lexical hash-feature matching.

## Repo layout

```
src-tauri/              Rust core (Tauri 2)
├── src/commands/       #[tauri::command] handlers
├── src/core/           vault, markdown_parser, file_watcher, link_resolver,
│                       graph_engine, embeddings, hdc, ssm, search, graph_rag,
│                       multimedia, canvas, agent
├── src/cognition/      [v5.0 NEW] CAN/LSM/Langevin/Hopfield/Hebbian/Hamiltonian/Holographic
├── src/reasoning/      [v5.0 NEW] VSA inference, ILP, Z3 bridge, self-modifier
├── src/protocols/      MCP server, control port WebSocket, auth, permissions, audit
├── src/orchestration/  [v5.0 NEW] workflow state machine, agents, resonance triggers
├── src/ai/providers/   [v5.0 NEW] Anthropic / OpenAI / Ollama traits
├── src/ai/retrieval/   [v5.0 NEW] contextual, hybrid RRF, reranker, compressor
└── src/db/             LibSQL + LanceDB + Tantivy

src/                    Next.js App Router (static export)
├── app/                page.tsx
├── components/         editor / sidebar / graph / canvas / ai / cognition /
│                       reasoning / search / settings
├── lib/tauri/          typed wrappers around invoke()
├── lib/store/          Zustand: vaultStore, editorStore
└── types/

docs/                   STAND_IN_REGISTRY, HDC_VSA_MATHEMATICS, COGNITIVE_LOOPS,
                        MCP_PROTOCOL, CONTROL_PORT_API, PHASE_GATES

tests/fixtures/sample-vault/   Legacy 3-note fixture (audited test suite)
tests/fixtures/real-vault/     v5.0 25+ note fixture (Arabic+English)
```

## State model

- **Rust:** `AppState` (in `lib.rs`) holds `Arc<Mutex<Option<VaultState>>>`, `Arc<Mutex<Option<StreamingState>>>`, `Arc<Mutex<Option<McpServerHandle>>>`. v5.0 adds `Arc<RwLock<SharedCortex>>` for the perpetual cognitive loop.
- **Frontend:** Zustand stores (`vaultStore`, `editorStore`). v5.0 will add `cortexStore`, `reflectionStore`, `factsStore`.
- **Event channel:** `vault://changed` (filesystem), `cortex://snapshot` (free-energy + dominant attractor), `reflection://written`.

## Database

- **LibSQL** `<vault>/.aura/aura.db`. Migrations embedded via `include_str!`, tracked in `_aura_migrations`.
- **LanceDB** `<vault>/.aura/vectors/`. Block embeddings, hypervectors, community summaries, multimedia embeddings.
- **Tantivy** `<vault>/.aura/tantivy/`. BM25 + Inverted Index for keyword + autocomplete.
- v5.0 adds tables: `cortex_snapshots`, `synaptic_weights`, `reflections`, `derived_facts`, `curiosity_questions`, `workflows`, `workflow_runs`, `agent_permissions`, `audit_log`.

## Path safety

`VaultState::resolve(rel)` rejects absolute paths, `..` components, and (per audit findings) **must also** reject null bytes and canonicalize through symlinks before reads. The current implementation rejects abs paths + `..` but accepts `\0`, bare `.`, and follows symlinks unconditionally — Phase 1 cleanup item from the audit.

## Run / verify

```bash
cd src-tauri
cargo clippy --no-deps -- -D warnings   # treats warnings as errors
cargo test                              # currently 97 tests pass

cd ..
pnpm install
pnpm typecheck
pnpm build                              # static export → out/
pnpm tauri dev                          # GUI — requires a display
```

Sandbox caveat: this build env has no display, no GPU, no HF network, no Anthropic API key, no yt-dlp/ffmpeg. GUI launches and ONNX model downloads must happen on the user's own machine.

## Tauri command registry

Every `#[tauri::command]` MUST be added to `tauri::generate_handler![…]` in `src-tauri/src/lib.rs`. Forgetting this is a clean `cargo build` but a 500 at runtime. Current count: 40 commands declared, 40 registered (verified).

## Current state (post-v3, pre-v5.0)

Phases 1-13 from v3 shipped. **TRUTH_AUDIT.md** documents the gap between claims and reality: 49 ✅, 12 ⚠️, 0 ❌, 9 🚨 stand-ins. v5.0 explicitly swaps those stand-ins phase-by-phase. See [`docs/STAND_IN_REGISTRY.md`](./docs/STAND_IN_REGISTRY.md).

## v5.0 phase roadmap (42 weeks)

| Phase | Adds | Status |
|------:|------|:------:|
| 0 | CLAUDE.md update, stand-in registry, fixture expansion, unwrap fixes | ✅ this commit |
| 1 | Path-safety hardening (null bytes, symlink canonicalization) | pending |
| 5 | Real ONNX MiniLM + Anthropic Contextual Retrieval + Hybrid RRF via Tantivy | pending |
| 6 | Extend existing HDC with FHRR (complex-valued, FFT-based) branch | pending |
| 7 | Real Leiden (4-level hierarchy) + Claude-summarised communities | pending |
| 8 | Mamba ONNX path A + Phi-3 path C (EMA stays as path B) | pending |
| 9 | Whisper/SigLIP/yt-dlp real path | pending |
| 10 | rmcp 0.12 + OAuth + Claw-Chain hardening | pending |
| 11 | **Cognitive Core: CAN + LSM + Langevin + Hebbian + Hopfield + Free Energy + Curiosity** | pending |
| 12 | **Hamiltonian fusion + Holographic memory** (experimental, telemetry-instrumented) | pending |
| 13 | **Neuro-symbolic: VSA + ILP + Z3 + Gödel-style prompt self-modifier** | pending |
| 14 | Mem0 + Letta integration | pending |
| 15 | LangGraph + Rust workflows + Anthropic Skills + resonance triggers | pending |
| 16 | Infinite Canvas drag-from-tree | pending |
| 17 | Polish, signing, sync, marketplace | pending |

Each phase ends at a STOP gate with tangible demo + reproduce commands.
