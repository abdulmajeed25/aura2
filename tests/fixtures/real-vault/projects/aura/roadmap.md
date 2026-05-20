---
title: Aura — Roadmap
tags: [aura, roadmap, project]
---

# Aura — Roadmap

Working roadmap for the v5.0 cognitive OS. Phase status mirrors
`docs/STAND_IN_REGISTRY.md`.

## Done (v3)
- Phases 1–13 from the v3 spec.
- 97 tests passing; 49 ✅ / 12 ⚠️ / 9 🚨 stand-ins per TRUTH_AUDIT.md.

## Phase 0 (v5.0) — current
- Closing the two `unwrap()`/`expect()` Hard Rule #4 violations.
- Stand-in registry seeded with 19 entries.
- This fixture (real-vault) lands here.

## Phase 1 — path safety hardening
Reject null bytes at the API boundary; canonicalize symlinks; cover
with `audit_path_safety` tests that currently fail.

## Phase 5 — real semantic
Swap [[projects/aura/cognitive-core-design|HashEmbedder]] for ONNX
MiniLM, add Anthropic Contextual Retrieval, hybrid RRF via Tantivy.

## Phase 11 — cognitive core
The big one. CAN + LSM + Langevin + Hopfield + Hebbian + Free Energy
+ Curiosity, all running in `cognition/perpetual_loop.rs`. See
[[areas/work/cognitive-architecture-notes#^system1-loop|the System 1 loop]].

## Phase 12 — experimental fusion
Hamiltonian leapfrog + holographic FHRR memory. Telemetry-instrumented.
Removable if 30-day numbers don't justify it.
