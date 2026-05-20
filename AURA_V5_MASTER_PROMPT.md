# AURA COGNITIVE OS — Master Implementation Prompt v5.0 (UNIFIED FINAL)

**Owner**: Abdulmajeed Talal Almutairi
**Date**: May 2026
**Status**: Authoritative build specification. Supersedes all prior drafts.

> Capture note (2026-05-20): pasted by the owner during the autonomous
> batch session. Earlier sections (PARTs 1–5 and part of PART 6) were
> abbreviated by the clipboard; the high-signal parts that drive
> implementation (PARTs 7–21) are intact. Re-paste a clean version when
> next available; until then this file is the working copy.

---

## PART 0 — Directive to Claude Code

You will build **Aura Cognitive OS** end-to-end. This document fuses
*every* technique, equation, library, and architectural pattern
discussed across the full design conversation — both the
production-proven layer (BM25, vector search, GraphRAG, MCP, hardened
local-first plumbing) and the research-grade cognitive layer
(continuous attractor networks, liquid state machines, Langevin
dynamics, Hopfield memory, Hebbian learning, neural ODEs, free-energy
minimization, Hamiltonian phase-space fusion, FHRR holographic
memory). Every numerical kernel must land with math-driven TDD; every
stand-in must be registered in `docs/STAND_IN_REGISTRY.md` with a swap
checklist; every phase ends at an explicit STOP gate.

---

## PARTS 1 – 5 — (placeholder)

Foundational sections: Tauri 2 + Next.js 15 scaffold, vault model,
markdown block-addressing system, wiki-link grammar, FTS5 + LibSQL +
Tantivy + LanceDB layout, file-watcher discipline. See the v3 repo for
the implementation that already shipped; the v5 deltas listed in
[`CLAUDE.md`](./CLAUDE.md) are authoritative until this section is
re-pasted in full.

---

## PART 6 — HDC Engine

(Partial capture; full description in code and `docs/HDC_VSA_MATHEMATICS.md`.)

Neighbourhood encoding mixes a note's own bag-of-bigrams hash signature
with the signatures of its REF/EMBED/MEMBER neighbours, each permuted
by relation:

```
note_hv = bundle(
    self_hv,
    permute(edge_hv_out_1, 1),
    permute(edge_hv_out_2, 1),
    ...
    permute(edge_hv_in_1, 2),
    permute(edge_hv_in_2, 2),
    ...
)
```

### 6.4 Unit tests (math-driven TDD)

```rust
#[test]
fn bind_is_self_inverse_for_bipolar() {
    let a = Hypervector::from_token("alpha");
    let b = Hypervector::from_token("beta");
    let bound = a.bind(&b);
    let unbound = bound.bind(&b);   // bind == unbind for bipolar
    assert!(a.similarity(&unbound) > 0.99);
}

#[test]
fn shared_neighbours_pull_disjoint_text_together() {
    // ALPHA  "alpha bravo charlie"     --[REF]-> HUB
    // OMEGA  "xylophone yankee zulu"   --[REF]-> HUB
    // ORPHAN "rosebud sunflower"       no edges
    let alpha  = encode_note_with_neighborhood(/* ... */);
    let omega  = encode_note_with_neighborhood(/* ... */);
    let orphan = encode_note_with_neighborhood(/* ... */);
    let s_aa = alpha.similarity(&omega);   // > 0.5 even with disjoint vocab
    let s_ao = alpha.similarity(&orphan);  // ≈ 0
    assert!(s_aa > 0.5);
    assert!(s_ao.abs() < 0.1);
}
```

(This is the property the current Aura codebase already proves: 0.6340
vs −0.0062 for siblings vs orphan.)

---

## PART 7 — SSM / Mamba Streaming

### 7.1 Two-path strategy

**Path A (preferred when available)**: Mamba-130M INT8 ONNX via the
`ort` crate. Attempt download/conversion; if it works, use it. Maintain
hidden state `(d_state=16, d_model=768)` across calls.

**Path B (always-on fallback)**: EMA streaming approximation (already
in Aura codebase). 384-dim hidden vector, α=0.82. Provides fixed-memory
streaming with recency bias. Marked `STANDIN: real Mamba expected`.

**Path C (alternative SSM)**: Phi-3-mini or Gemma 3 2B ONNX as a small
reasoning model. Used when local LLM reasoning is needed (not the same
as SSM streaming, but solves the same user-facing goal).

The system loads whichever paths are available at runtime and exposes
capability flags via the integrations panel.

### 7.2 Pipeline

```
[user query] → [text embedding] → [hybrid RRF search] → [community routing]
            → [HDC neighborhood encoding] → [SSM/EMA stream update]
            → [LLMLingua-2 compression] → [Claude with prompt caching]
            → [response] → [Hebbian weight reinforcement on used blocks]
            → [reflection scheduled]
```

---

## PART 8 — GraphRAG: Real Leiden + LightRAG Bridge

### 8.1 Leiden algorithm (custom Rust impl)

Implement Leiden directly via `petgraph` + custom modularity optimizer
with three phases (local move, refinement, aggregation). Produce a
**4-level hierarchy** by varying the resolution parameter γ ∈
{2.0, 1.0, 0.5, 0.25}.

### 8.2 LightRAG bridge

For dual-level (low-level entity + high-level theme) retrieval,
optionally spawn LightRAG as a sidecar (HTTP). If `lightrag.enabled =
false` in config, use the in-Rust Leiden + extractive summary path.

### 8.3 Summarization

For each community at each level:

1. If members ≤ 20: send all blocks to Claude Haiku 4.5 with prompt
   caching on the system prompt → ~200-word summary.
2. If members > 20: chunk into groups of 15 → summarize each →
   meta-summarize.

The summary becomes a new "block" embedded into LanceDB and added to
the community graph.

### 8.4 Query dispatch

```rust
pub enum QueryMode {
    Local,    // specific question → top-K blocks via hybrid RRF
    Global,   // thematic question → community summaries
    Drift,    // both → fuse via HDC bundling
}
```

The dispatch is decided by a small classifier prompt sent to Claude
Haiku (with prompt caching) at the start of each query.

---

## PART 9 — Memory Layer: Mem0 + Letta + Aura-native

Three memory subsystems coexist.

### 9.1 Mem0-style fact memory (Rust-native)

ADD / UPDATE / DELETE / NOOP pattern. Implement directly in Rust:

- Every interaction with Claude → extract candidate facts via a
  structured prompt.
- Diff against `derived_facts` and `synaptic_weights` tables.
- Decide action by similarity threshold.

### 9.2 Letta-style sleeptime agent (sidecar OR Rust port)

- **Sidecar mode**: spawn Python `letta` process, register Aura as a
  memory provider.
- **Rust port** (preferred for distribution): implement
  `cognition/perpetual_loop.rs` to fire reflection prompts every N idle
  minutes, write results to `.aura/brain/reflections/YYYY-MM-DD/*.md`.

### 9.3 Aura-native cortex (THE core innovation)

The Cortex (Part 10) IS Aura's distinctive memory. Mem0 and Letta are
*complementary*, not substitutes.

---

## PART 10 — THE COGNITIVE CORE (Subconscious / System 1)

This is the part nobody else has built. We are building it.

### 10.1 Module map

| File | Equation | Role |
|---|---|---|
| `shared_cortex.rs` | container | Holds `cognitive_state ∈ ℝ^D`, `reservoir_state ∈ ℝ^R`, `synaptic_matrix ∈ ℝ^{N×N}`, `dt`, `tau`, MPSC channels |
| `cans.rs` | `τ dx/dt = -x + σ(W·x + I)` | Wilson-Cowan continuous attractor |
| `lsm.rs` | `r(t+dt) = (1-α)r(t) + α·tanh(W_res·r(t) + u(t))` | Liquid State Machine reservoir |
| `langevin.rs` | `dx = -∇E(x)dt + √(2β⁻¹)·dW_t` | Stochastic curiosity drift |
| `hopfield.rs` | Modern Hopfield update (Ramsauer et al. 2020) | Content-addressed retrieval |
| `hebbian.rs` | `Δw_ij = η · x_i · x_j` (+ STDP timing window) | Synaptic plasticity |
| `neural_ode.rs` | `dh/dt = f_θ(h(t), t)` | Time-continuous weight decay/revive |
| `free_energy.rs` | `F = E_q[log q(θ) - log p(ỹ, θ)]` | Variational free-energy minimization |
| `curiosity.rs` | learning-progress score | Schmidhuber-style intrinsic reward |
| `hamiltonian.rs` | `H = T(p) + V(x); dx=∂H/∂p·dt; dp=-∂H/∂x·dt` | Symplectic leapfrog integrator fusing LLM momentum with local potential |
| `holographic.rs` | FHRR circular convolution + interference | Holographic interference memory |
| `perpetual_loop.rs` | orchestrator | Spawns background thread, integrates every `dt = 0.01s`, sleeps to cap CPU |
| `reflection_writer.rs` | writer | Materializes high-energy events to `.aura/brain/*.md` |

### 10.2 Perpetual integration loop (skeleton)

```rust
// src-tauri/src/cognition/perpetual_loop.rs

use std::thread;
use std::time::Duration;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct CognitiveCore {
    pub state: Arc<RwLock<SharedCortex>>,
    pub claude: Arc<dyn AIProvider>,
    pub vault: Arc<VaultDb>,
    pub config: CognitiveConfig,
}

pub struct CognitiveConfig {
    pub dt: f32,             // 0.01
    pub tau: f32,            // 0.1
    pub alpha_lsm: f32,      // 0.2
    pub beta_langevin: f32,  // 0.5
    pub hebb_lr: f32,        // 0.01
    pub reflection_idle_secs: u64,  // 300
    pub max_cpu_pct: f32,    // 25.0
}

impl CognitiveCore {
    pub fn start_perpetual_loop(self: Arc<Self>) {
        thread::Builder::new()
            .name("aura-cortex".into())
            .spawn(move || {
                tracing::info!("Aura Cognitive Core: subconscious loop starting");
                let mut last_reflection = std::time::Instant::now();
                loop {
                    // 1. inject Langevin noise (curiosity drift)
                    let noise = langevin::gaussian_noise(
                        self.state.read().cognitive_state.len(),
                        self.config.beta_langevin,
                    );
                    // 2. LSM reservoir update (echoes of recent activity)
                    let input = self.state.read().pull_pending_input();  // from MPSC
                    {
                        let mut s = self.state.write();
                        lsm::step(&mut s.reservoir_state, &s.synaptic_matrix,
                                  &input, self.config.alpha_lsm);
                    }
                    // 3. Wilson-Cowan CAN step (attractor dynamics)
                    {
                        let mut s = self.state.write();
                        cans::wilson_cowan_step(&mut s.cognitive_state,
                                                &s.synaptic_matrix,
                                                self.config.dt, self.config.tau);
                        // 4. add Langevin noise scaled by dt
                        s.cognitive_state += &(noise * self.config.dt);
                    }
                    // 5. periodic Free Energy minimization + Hebbian update
                    if last_reflection.elapsed().as_secs() >= self.config.reflection_idle_secs {
                        if system_is_idle() {
                            self.run_reflection_cycle().ok();
                            last_reflection = std::time::Instant::now();
                        }
                    }
                    // 6. push state to MCP listeners + persist snapshot every 30s
                    self.maybe_snapshot();
                    // CPU cap: sleep 10ms between ticks
                    thread::sleep(Duration::from_millis(10));
                }
            })
            .expect("cortex thread");
    }

    fn run_reflection_cycle(&self) -> anyhow::Result<()> {
        // a) measure free energy → identify contradictions
        let energy = free_energy::compute(&self.state.read());
        // b) Hopfield-style content-addressed retrieval of relevant memories
        let memories = hopfield::retrieve(&self.state.read(), /* top_k = */ 5);
        // c) curiosity: detect knowledge gaps via GraphRAG community holes
        let gap = curiosity::find_gap(&self.vault)?;
        // d) build reflection prompt for Claude (with prompt caching on system context)
        let prompt = reflection_prompt(energy, &memories, &gap);
        // e) call Claude → store insight as reflection block
        let insight = self.claude.generate(&prompt)?;
        let reflection_id = reflection_writer::persist(&self.vault, &insight, gap)?;
        // f) reinforce Hebbian weights between memories cited in the reflection
        hebbian::reinforce(&mut self.state.write(), &memories, self.config.hebb_lr);
        Ok(())
    }
}
```

### 10.3 Hamiltonian phase-space fusion (the experimental fusion layer)

The Hamiltonian `H(x, p) = T(p) + V(x)` where:

- `V(x)` = potential energy from local synaptic matrix (purely Rust, deterministic)
- `T(p)` = kinetic energy seeded by Claude's response embeddings via MCP

Leapfrog symplectic integration step:

```
p(t+dt/2) = p(t) − (dt/2) · ∇V(x(t))
x(t+dt)   = x(t) + dt · ∇T(p(t+dt/2))
p(t+dt)   = p(t+dt/2) − (dt/2) · ∇V(x(t+dt))
```

Claude's "intellectual momentum" is injected as `p` whenever a Claude
response arrives via MCP — its embedding is projected into the same
D-dim space as the cognitive state. The local Rust loop then bends
Claude's trajectory along the manifold defined by `V(x)` (which encodes
the user's documented knowledge structure).

**Honest disclosure**: this is a research-grade construct. We will
build it, instrument it heavily, and **measure whether it improves any
downstream metric** (retrieval quality, reflection coherence, agent
task success). If after 30 days of telemetry it shows no measurable
improvement, we strip it and keep the simpler CAN+LSM+Langevin core.
The code path stays under `cognition::hamiltonian` so the experiment is
contained.

### 10.4 Holographic interference memory (FHRR branch)

Implement `holographic.rs` using complex unit-modulus FHRR vectors via
`rustfft`:

- Binding = element-wise complex multiplication
- Bundling = mean + renormalization
- Lookup = circular correlation (FFT, multiply, IFFT)

Store an optional FHRR representation alongside bipolar HV in LanceDB
(`fhrr_real`, `fhrr_imag` columns). Use FHRR for similarity-by-
interference experiments; use bipolar for fast bit-packed similarity by
default.

### 10.5 Reflection output format

Every reflection appears in `.aura/brain/reflections/YYYY-MM-DD/HHmmss_<slug>.md`:

```markdown
---
trigger: idle | curiosity | contradiction | user_query
energy_before: 0.473
energy_after: 0.291
references:
  - block_id: 01j5...
    path: areas/work/sovereign-protocol.md
  - block_id: 01k2...
    path: projects/taminat/billing.md
---
# Insight: Settlement engine in taminat mirrors Sovereign smart-contract escrow

While integrating today's tow-truck billing changes, the cortex noticed that
your settlement-split SystemSetting design ([[taminat/billing#^settlement-split]])
follows the same algebraic pattern as Sovereign Protocol's escrow primitive
([[sovereign-protocol/escrow#^split-rule]]). Both treat fees as
`(provider_share, platform_share, regulator_share)` triples with invariant
`sum == 100`.

Suggestion: extract a shared `FeeSplit` abstraction into a vendored crate
both projects depend on. Reduces drift, single source of truth for
Shariah-compliance audits.
```

The user sees these in a "Reflection Feed" panel and can accept (turn
into a normal note + Hebbian reinforcement) or dismiss (still archived,
used to teach curiosity which directions are unfruitful).

---

## PART 11 — NEURO-SYMBOLIC REASONING (System 2 deductive layer)

### 11.1 VSA algebraic inference (`vsa_inference.rs`)

Encode rules as bound triples:

```
"Smart contracts follow Sovereign Protocol"
→ rule_hv = SMART_CONTRACT ⊙ FOLLOWS ⊙ permute(SOVEREIGN_PROTOCOL, 1)
```

Given new fact `NEW_CONTRACT = SMART_CONTRACT`, derive:

```
predicted_target = unbind(unbind(rule_hv, NEW_CONTRACT), FOLLOWS)
                 = permute(SOVEREIGN_PROTOCOL, 1)
unpermuted       = permute_inverse(predicted_target, 1)
similarity_to_known = cos_sim(unpermuted, SOVEREIGN_PROTOCOL_hv)
```

If similarity > 0.7 → store as **derived fact** with `confidence = similarity`.

### 11.2 ILP engine (`ilp_engine.rs`)

Parse First-Order Logic facts from explicit user markup:

```markdown
---
facts:
  - Parent(Aura, MCPServer)
  - Requires(MCPServer, OAuth)
---
```

Forward-chain new facts via Datalog-style evaluation. New facts:

```
∀x,y,z. Parent(x,y) ∧ Requires(y,z) ⊢ Requires(x,z)
→ Requires(Aura, OAuth)
```

Store in `derived_facts` with derivation trace.

### 11.3 Theorem prover bridge (`theorem_prover.rs`)

Use Z3 via the `z3` crate for harder proofs. Example: user notes
contain inequalities and constraints; Z3 verifies whether a proposed
contract clause is consistent with all prior constraints. Returns
SAT/UNSAT + counterexample model.

### 11.4 Self-modifier (Gödel-style via DSPy/TextGrad-equivalent in Rust)

**Not** self-rewriting Rust source. Instead:

- Aura keeps a library of **prompts** used internally (summarization,
  reflection, classification).
- After each use, score the output quality (length, downstream user
  acceptance, energy reduction).
- Periodically run a prompt-optimization loop:
  1. Generate variations of the prompt (via Claude).
  2. A/B test on cached representative inputs.
  3. Promote winners; archive losers.

This realizes the *behavioral* effect of a Gödel Machine
(self-improving system) without the dangerous "rewrite your own
compiler" failure mode. All prompts versioned in `.aura/prompts/` so
the user can audit.

---

## PART 12 — RETRIEVAL: Anthropic Contextual + Hybrid RRF + LLMLingua-2

### 12.1 Ingest pipeline

For each new/modified block:

1. **Chunk** at semantic boundaries (paragraphs, code fences, list items).
2. **Contextualize**: send (whole file + chunk) → Claude Haiku 4.5
   with `prompt_caching` on the whole file → returns a 50-word prefix
   describing how the chunk fits in the file.
3. **Embed** the contextualized chunk via `all-MiniLM-L6-v2` (English)
   or `BGE-M3` (Arabic/multilingual).
4. **Index** in BM25 (Tantivy, with the same contextual prefix) and LanceDB.
5. **Encode HV** and store in LanceDB.
6. **Update synaptic weights** via Hebbian rule for blocks recently co-accessed.

### 12.2 Query pipeline

1. **Classify** query mode (local/global/drift) via cached Claude Haiku call.
2. **Hybrid RRF**:
   - BM25 top-K (Tantivy)
   - Vector top-K (LanceDB)
   - RRF fusion with k=60, weights 1:4 BM25:vector (per Anthropic cookbook).
3. **Rerank** top-150 via Cohere rerank-multilingual-v3 (cloud) OR
   local cross-encoder (ms-marco-MiniLM).
4. **Compress** the top-20 chunks via LLMLingua-2 sidecar (or local
   approximation): 2–5× compression.
5. **Assemble prompt** with prompt caching on system prompt + recent conversation.
6. **Stream** response from Claude Sonnet 4.6.
7. **Reinforce** Hebbian weights on cited blocks.
8. **Schedule** a reflection on the query (asynchronously, sleep-time).

### 12.3 LLMLingua-2 integration

Run as Python sidecar (recommended) — accepts text over HTTP, returns
compressed text. Aura calls it before every LLM request and records the
compression ratio in the audit log.

Fallback: a Rust-native heuristic compressor that strips Markdown
formatting characters, deduplicates whitespace, and removes very-low-
IDF tokens (BM25 statistics from Tantivy). Compression ~1.5×, much less
than LLMLingua, but zero-dependency.

---

## PART 13 — MULTIMEDIA layer (real this time)

### 13.1 Models (auto-download with checksums)

| Modality | Model | Size | Format |
|---|---|---|---|
| Text | all-MiniLM-L6-v2 OR BGE-M3 | 90 MB / 2.3 GB | ONNX |
| Audio | whisper-tiny encoder layer | 75 MB | ONNX |
| Vision | siglip-small | 200 MB | ONNX |

`models/manifest.json` lists URL, SHA-256, and license for each. First
use triggers download with a progress UI.

### 13.2 YouTube pipeline

1. `yt-dlp --dump-json <url>` → metadata (title, channel, duration).
2. `yt-dlp -x --audio-format wav -q 5 <url>` → 8 kHz audio.
3. `yt-dlp -f "worst[height<=360]" <url>` + `ffmpeg -vf fps=1 ...` →
   keyframes at 1 fps.
4. Audio → Whisper encoder → 512-dim per second → mean-pooled or
   stored per-segment.
5. Each keyframe → SigLIP → 512-dim.
6. Bind each frame embedding with `FRAME_<i>` positional HV; bundle all.
7. Final video HV = bundle(audio_hv, all_frame_hvs).
8. Store in `multimedia_embeddings` with timestamps.

User-facing search: "find the video about clicking AC sound" → query
embedding → cosine over `unified_embedding` → returns video + most-
similar timestamp.

### 13.3 Legal note

The user can index only what they're entitled to (their own uploads,
public domain, fair-use research). Aura does not redistribute fetched
content; it only stores embeddings + local cache that the user controls.

---

## PART 14 — MCP Server & Aura Control Port

### 14.1 MCP server (HTTP, OAuth Resource Server pattern)

`rmcp` crate with `#[tool]` macros. Tools exposed:

| Tool | Description |
|---|---|
| `aura_search` | Hybrid RRF search; returns block refs + snippets |
| `aura_read_note` | Read full file content by path or ID |
| `aura_write_block` | Create/update block (requires `Write` permission) |
| `aura_create_link` | Create wiki-link between blocks |
| `aura_graph_rag_query` | Hierarchical GraphRAG query |
| `aura_get_backlinks` | Backlinks for a file/block |
| `aura_list_orphans` | Notes with no incoming/outgoing links |
| `aura_suggest_links` | HDC-similarity-based link suggestions |
| `aura_run_workflow` | Trigger a registered workflow (dry-run by default) |
| `aura_get_reflection_feed` | Read recent reflections |
| `aura_query_facts` | Query derived facts (VSA/ILP/Z3) |
| `aura_inject_cognitive_input` | Push input to LSM reservoir (advanced, `CognitiveAccess`) |
| `aura_cortex_status` | Read current free energy, dominant attractor (telemetry) |

All endpoints bind `127.0.0.1`. Every request: OAuth Bearer token
verified per-request, owner status derived from token only, path
canonicalized, audit-logged.

### 14.2 Control port (WebSocket JSON-RPC)

Lower-level, fine-grained API for agents that need to subscribe to
cognitive events (e.g., react when free energy spikes). Same auth /
permission rules.

### 14.3 Permission system

```rust
pub enum Permission {
    Read,
    Write,
    CreateFiles,
    DeleteFiles,
    Reorganize,
    Query,
    CognitiveAccess,        // read cortex state
    CognitiveWrite,         // inject input to cortex (rare, scoped)
    RunWorkflow,
    ManagePermissions,
}

pub struct AgentPermissions {
    agent_id: String,
    agent_name: String,
    permissions: HashSet<Permission>,
    scope: PermissionScope,
    expires_at: Option<i64>,
    max_operations_per_hour: Option<u32>,
}
```

### 14.4 Security hardening (Claw-Chain lessons)

| Vulnerability class | Mitigation in Aura |
|---|---|
| TOCTOU file ops | Atomic `open + fstat` pattern; never resolve path twice |
| Symlink escape | `canonicalize` + verify still within vault root |
| Heredoc env-expansion | No shell heredocs anywhere; all commands invoked with explicit argv |
| Client-controlled ownership | Owner derived **only** from authenticated token |
| Stale bearer tokens | Token rotated on every restart + revocation list |
| Null-byte paths | Reject at API boundary |
| Localhost ≠ trusted | All endpoints require auth even on 127.0.0.1 |
| Unbounded request bodies | 10 MB body limit on all endpoints |

---

## PART 15 — AGENT ORCHESTRATION

### 15.1 LangGraph sidecar mode (preferred for complex agents)

Spawn Python LangGraph service. Aura exposes a single
`aura_run_workflow` MCP tool; the LangGraph graph internally calls back
through MCP for any vault operations. Checkpoints stored in
`.aura/agent-state/`.

### 15.2 Rust-native mode (lightweight workflows)

`orchestration/workflow.rs` implements a state machine with:

- Node = (name, function, retry policy, timeout)
- Edges = conditional transitions
- Checkpointing every node
- Dry-run mode produces a diff without applying it
- Rollback via reverse-applying the diff

### 15.3 Resonance-based triggers

Workflow has a stored `trigger_vector` (HDC signature). The cognitive
core's `cognitive_state` is dotted with each enabled workflow's
`trigger_vector` every N seconds. When `cos_sim > threshold` and
`last_fired_at` is sufficiently old, the workflow auto-suggests itself
to the user (or auto-fires if `auto_fire = true` and the workflow has
`RunWorkflow` permission + dry-run + user-confirmed past success).

Example workflows shipped:

1. **`marketing-plan-from-cluster`**: when cognitive state aligns with
   "marketing" + "summer season" clusters, propose drafting a marketing
   plan based on recently active notes.
2. **`contract-from-template`**: trigger on alignment with "contract
   draft" + active counter-party note.
3. **`reflection-to-blog-post`**: on alignment with a chain of recent
   reflections, propose merging into a blog draft.

### 15.4 Anthropic Skills

Skills loaded from `.aura/skills/<skill-name>/SKILL.md` + supporting
scripts. Aura registers them with Claude via the Skills API. Initial
skills shipped:

- `marketing-plan-skill`
- `contract-draft-skill`
- `content-pipeline-skill`

---

## PART 16 — UI / UX

### 16.1 Layout (CodeMirror 6 + PixiJS + Three.js)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ [Aura ⚡] [Vault]                            [⚙] [🔍] [🧠] [👤]                │
├──────────┬─────────────────────────────────────────┬─────────────────────────┤
│ Files    │             Editor Pane                  │  Right Pane            │
│ Tags     │  ┌──────────────────────────────────────┐│  ┌──────────────────┐  │
│ Agents   │  │ # Title                              ││  │ Backlinks        │  │
│ Cortex   │  │ [[wiki]] ![[embed]] $math$ {{id}}    ││  │ Outline          │  │
│ Reflect  │  └──────────────────────────────────────┘│  │ AI Chat          │  │
│          │  [Source | Live Preview | Reading]       │  │ Cortex Monitor   │  │
├──────────┴─────────────────────────────────────────┴┤  │ Reflection Feed  │  │
│ Status: 1,247 notes • 8,392 blocks • Energy: 0.34 ↓ │  │ Curiosity        │  │
│         Indexed 100% • MCP: live • Cortex: running  │  └──────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────┘
```

New panels distinctive to Aura:

- **Cortex Monitor**: real-time line chart of free energy, current
  dominant attractor (top-3 tokens), reservoir activity heatmap.
- **Reflection Feed**: scroll of recent insights with accept/dismiss
  buttons.
- **Curiosity**: open questions the system has generated; clicking one
  opens a draft note pre-populated with the question.
- **Deduction Feed**: derived facts from VSA/ILP/Z3 with provenance.

### 16.2 Command palette (`Cmd+K`)

- File/block/tag search
- Run workflow
- Inject cognitive input ("seed the cortex with: ...")
- Query derived facts
- Show audit log

### 16.3 Themes

- `Aura Dark` (default): bg `#0a0a0f`, text `#e4e4e7`, accent `#818cf8`
- `Aura Light`: bg `#fafafa`, text `#18181b`, accent `#6366f1`
- `Aura Cortex`: minimal chrome to highlight visualizations

---

## PART 17 — IMPLEMENTATION ROADMAP (phase-by-phase, with Vertical Completion)

> Each phase ends with `STOP — request "continue"`. Demo report required.

### Phase 0 — Bootstrap & CLAUDE.md (Day 1–3)
- Initialize Tauri 2 + Next.js 15 + Cargo workspace
- Write `CLAUDE.md` summarizing this document in ≤ 2 pages
- Create `tests/fixtures/real-vault/` with 50 starter notes
- CI workflow runs on push

### Phase 1 — Foundation (Week 1–2)
- Vault open/close, file CRUD, file watcher
- Markdown parser, FTS5, LibSQL migrations
- File explorer + CodeMirror editor (no extensions yet)
- **STOP**

### Phase 2 — Block System & Wiki-Links (Week 3–4)
- UUIDv7 block IDs, `^anchor` markers
- All 4 wiki-link variants + auto-healing
- Backlinks, Outline, autocomplete
- **STOP**

### Phase 3 — Live Preview & Embeds (Week 5)
- CodeMirror live preview, transclusion extension
- Reading view with `marked`
- **STOP**

### Phase 4 — Graph View (Week 6–7)
- Fruchterman-Reingold in Rust + Rayon
- PixiJS rendering, 10k → 100k node stress test
- **STOP**

### Phase 5 — Vector Search + Contextual Retrieval (Week 8–10)
- LanceDB + Tantivy
- `all-MiniLM-L6-v2` ONNX via `ort`
- Anthropic Contextual Retrieval ingest pipeline
- Hybrid RRF query
- **STOP** ← swap the existing hash-feature embedder seam

### Phase 6 — HDC Engine (Week 11–12)
- `hypervector.rs`, `graph_encoder.rs` (already partly built — extend)
- LanceDB packed-byte storage
- FHRR research branch
- "Find Related" UI
- **STOP**

### Phase 7 — Real Leiden + GraphRAG + LightRAG Bridge (Week 13–15)
- Custom Leiden implementation in Rust
- 4-level hierarchical summaries (Claude Haiku + prompt caching)
- LightRAG sidecar optional
- AIChat "Global Query" mode
- **STOP** ← swap the existing LPA stand-in

### Phase 8 — SSM / Mamba (Week 16–17)
- Attempt Mamba-130M ONNX; if fails, document and keep EMA stand-in
- Phi-3-mini fallback path
- Continuous Mode in AIChat
- **STOP**

### Phase 9 — Real Multimedia (Week 18–19)
- yt-dlp + ffmpeg integration
- Whisper-tiny + SigLIP-small ONNX
- Unified 512-dim encoder
- Drag-drop + URL paste
- **STOP** ← swap the existing filename-fingerprint stand-in

### Phase 10 — MCP Server + Control Port + Hardening (Week 20–21)
- `rmcp` 0.12 server
- All 13 tools wired
- OAuth Resource Server, full Claw-Chain mitigations
- Settings → Integrations UI
- **STOP**

### Phase 11 — THE COGNITIVE CORE (Week 22–26) ★ THE INNOVATION ★
- `cognition/` module: CAN + LSM + Langevin + Hopfield + Hebbian +
  Neural ODE + Free Energy + Curiosity
- `perpetual_loop.rs` running as background thread with CPU cap
- Cortex Monitor + Reflection Feed UI
- Reflection writer materializes `.aura/brain/*.md`
- Math-driven TDD: every kernel has hand-verified test
- **STOP**

### Phase 12 — Hamiltonian Fusion + Holographic Memory (Week 27–28)
- `hamiltonian.rs` symplectic leapfrog
- `holographic.rs` FHRR via rustfft
- Instrument with telemetry: does it improve retrieval / reflection /
  agent metrics?
- **STOP** with 30-day measurement window

### Phase 13 — Neuro-Symbolic Reasoning (Week 29–31)
- VSA inference, ILP engine, Z3 bridge, prompt self-modifier
- Deduction Feed, Proof Tree, KB Editor UI
- **STOP**

### Phase 14 — Memory Layer (Mem0 + Letta integration) (Week 32–33)
- Mem0-style fact memory in Rust
- Letta sidecar OR Rust port of sleeptime
- Reflection scheduling consolidated
- **STOP**

### Phase 15 — Agent Workspace + Workflows (Week 34–36)
- LangGraph sidecar
- Rust-native workflow runtime
- 3 shipped skills + 3 shipped workflows
- Resonance triggers
- Diff preview + undo
- **STOP**

### Phase 16 — Infinite Canvas (Week 37–38)
- PixiJS canvas
- Drag block ↔ note bidirectional binding
- **STOP**

### Phase 17 — Polish, Audit, Distribute (Week 39–42)
- E2E tests
- Encrypted sync (age envelopes)
- Auto-update
- Code signing macOS / Windows / Linux
- Landing page + plugin marketplace v1

---

## PART 18 — CLAUDE CODE EXECUTION RULES (read every session)

1. **Re-read `CLAUDE.md` and this document at session start.**
2. **Never declare a technique impossible without writing a falsifying
   test first.** If Mamba ONNX fails to convert, document the exact
   error and propose two fallbacks; do NOT delete the Mamba module.
3. **Vertical Completion**: DB → core → API → wrapper → UI → real-data
   validation per feature.
4. **Forbidden**: mock data, ignored errors, `unwrap()` outside tests,
   hardcoded paths, half-wired features.
5. **Math-Driven TDD** for every numerical kernel. Test first, with
   hand-computed expected values.
6. **Audit `tests/fixtures/real-vault/` after every phase**: indices
   clean, no stale rows, no orphan embeddings.
7. **Run after every significant change**: `cargo check`,
   `cargo clippy --no-deps -- -D warnings`, `pnpm typecheck`,
   `pnpm build`.
8. **Parallel subagents** for independent backend / frontend work.
9. **Honest disclosure**: any stand-in is marked `STANDIN:` in code AND
   `docs/STAND_IN_REGISTRY.md`. Swap checklist required.
10. **Phase gates are sacred**: at the end of each phase, present
    - exact file paths created/modified
    - commands to reproduce the demo
    - expected output (terminal screenshot described in text)
    - any failures encountered and how handled
    - one-line ask: "STOP — request continue"
11. **Telemetry-first for experimental modules** (Hamiltonian,
    Holographic, Free Energy): instrument before the user has to ask.
    Energy traces, similarity diffs, reflection acceptance rates — all
    logged in `audit_log`.
12. **Honor Abdulmajeed's rhythm**: single-character "c" / "ok" /
    "continue" advances a phase. Speak Najdi Arabic when explaining to
    him; keep code and identifiers English.

---

## PART 19 — Pre-flight questions (ask before writing code)

1. **Target OS** for first build? (macOS / Windows / Linux — pick one
   to optimize first)
2. **GPU available?** Determines whether SigLIP/Whisper run on
   CPU-only or hardware-accelerated.
3. **Use existing Aura codebase as starting point?** (the project with
   97/97 tests, 49 working claims, 9 stand-ins) — answer is presumably
   YES; this prompt then proceeds module by module to replace stand-ins
   and add the cognitive core.
4. **Letta**: sidecar Python service or Rust port?
5. **LightRAG**: enable sidecar or use in-Rust Leiden + extractive only?
6. **Cohere reranker**: cloud key available, or use local cross-encoder?

Answer all 6, then begin Phase 0.

---

## PART 20 — Stand-in registry (initial)

These are the 9 stand-ins from the current Aura codebase that will be
swapped phase-by-phase:

| Module | Current | Target | Phase |
|---|---|---|---|
| Embeddings | 384-dim hash-feature | `all-MiniLM-L6-v2` ONNX (English) + BGE-M3 (Arabic) | 5 |
| Communities | LPA (single-level) | Leiden (4-level hierarchy) | 7 |
| Community summaries | Extractive concatenation | Claude Haiku 4.5 LLM summaries with prompt caching | 7 |
| GraphRAG answer | Concatenated extracts | Claude Sonnet 4.6 with cited communities | 7 |
| SSM | EMA (α=0.82) | Mamba-130M ONNX or Phi-3-mini fallback | 8 |
| Media encoder | Byte-fingerprint (filename-dominated) | Whisper-tiny + SigLIP-small ONNX | 9 |
| URL ingest | none | yt-dlp + ffmpeg pipeline | 9 |
| `unwrap()` violations | 2 occurrences | Proper `Result` handling | 0 |
| GUI launch | Never launched on display | Launch + verify all panels render | 1 |

---

## PART 21 — END

Execute Phase 0 now. Begin by reading the existing Aura codebase, write
the updated `CLAUDE.md` and `docs/STAND_IN_REGISTRY.md`, ask the 6
pre-flight questions, then move into Phase 1.

The Cortex will wake. Let's build it.

— End of Master Prompt v5.0 Unified —
