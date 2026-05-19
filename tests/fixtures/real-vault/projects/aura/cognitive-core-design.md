---
title: Aura — Cognitive Core Design
tags: [aura, cognition, design]
status: design
---

# Aura — Cognitive Core Design

Layer-by-layer plan for `src-tauri/src/cognition/`. Source of truth
for the math-driven TDD tests that must land **before** the
implementation.

## Layered loop

```
              ┌─────────────────────────────────────┐
              │  shared_cortex (state container)    │
              └─────────────────────────────────────┘
                              │
   ┌──────────────────┬──────────────────┬──────────────────┐
   ▼                  ▼                  ▼                  ▼
  CAN (Wilson-       LSM (reservoir)   Hopfield          Hebbian/STDP
   Cowan attract.)   r(t+dt)=...       (modern)          Δw=η·x·y
   τ dx/dt = ...
   ▲                  ▲
   │                  │
   Langevin noise    user input via
   √(2β⁻¹)·dW_t      MPSC channel
```

## Initial state size

- `cognitive_state ∈ ℝ^512` (matching the unified multimodal space).
- `reservoir_state ∈ ℝ^2048`.
- `synaptic_matrix` block-sparse, capped at 50k non-zero entries to
  fit the RAM budget (≤ 350 MB with full loops running).

## Why a unified `D = 512` for cognition while HDC uses `D = 10_000`

HDC needs the high dimension for binding capacity (Plate's bound:
~`D / (4·log K)` independent role-filler pairs). The cortex doesn't
bind; it integrates. 512 is enough for an attractor manifold and
keeps the integration step well under the 5 ms target.

## Test contract (write these first)

1. `cans::wilson_cowan_step` — 100-step integration of a 2-attractor
   landscape converges to one attractor (hand-computed end-state).
2. `lsm::step` — repeated zero input drives the reservoir to a fixed
   point; non-zero input perturbs it predictably.
3. `langevin::gaussian_noise` — sample mean ≈ 0, sample variance ≈ 1
   for n ≥ 10_000.
4. `hebbian::reinforce` — co-activated indices have higher weights
   after N updates; non-co-activated ones don't.
5. `free_energy::compute` — monotone non-increasing across a sequence
   of reflection cycles on a synthetic vault.

## Crosslinks

- The HDC algebra these loops feed off lives in
  [[resources/papers/vsa-foundations]].
- Mamba as the SSM backbone (Phase 8): [[resources/papers/mamba-state-space]].
- Free-energy formalism: [[resources/papers/free-energy-principle]].
