# Cognitive Loops — Theory + Code Map

Module map for `src-tauri/src/cognition/` (Phase 11 onwards). **Nothing
in this directory exists in code yet.** This file is the up-front
contract so the math-driven TDD tests can be written before the
implementation.

## Module table

| File | Equation | Role |
|------|----------|------|
| `shared_cortex.rs` | container | Holds `cognitive_state ∈ ℝ^D`, `reservoir_state ∈ ℝ^R`, `synaptic_matrix ∈ ℝ^{N×N}`, `dt`, `tau`, MPSC channels |
| `cans.rs` | `τ dx/dt = -x + σ(W·x + I)` | Wilson-Cowan continuous attractor |
| `lsm.rs` | `r(t+dt) = (1-α)r(t) + α·tanh(W_res·r(t) + u(t))` | Liquid State Machine reservoir |
| `langevin.rs` | `dx = -∇E(x)dt + √(2β⁻¹)·dW_t` | Stochastic curiosity drift |
| `hopfield.rs` | Modern Hopfield update (Ramsauer et al. 2020) | Content-addressed retrieval |
| `hebbian.rs` | `Δw_ij = η · x_i · x_j` (+ STDP timing window) | Synaptic plasticity |
| `neural_ode.rs` | `dh/dt = f_θ(h(t), t)` | Time-continuous weight decay/revive |
| `free_energy.rs` | `F = E_q[log q(θ) − log p(ỹ, θ)]` | Variational free-energy minimisation |
| `curiosity.rs` | learning-progress score | Schmidhuber-style intrinsic reward |
| `hamiltonian.rs` | `H = T(p) + V(x); dx=∂H/∂p·dt; dp=-∂H/∂x·dt` | Symplectic leapfrog fusing LLM momentum with local potential |
| `holographic.rs` | FHRR circular convolution + interference | Holographic interference memory |
| `perpetual_loop.rs` | orchestrator | Background thread, integrates every `dt = 0.01s`, sleeps to cap CPU |
| `reflection_writer.rs` | writer | Materialises high-energy events to `.aura/brain/*.md` |

## CPU budget

`perpetual_loop` must respect `CognitiveConfig.max_cpu_pct = 25.0`.
Implementation: 10 ms `thread::sleep` between integration ticks, plus
exponential backoff if `idle_secs < 30`.

## State persistence

Snapshots written to `cortex_snapshots` table every 30 seconds:
free energy + compressed cognitive state blob. Allows post-mortem
analysis of the cortex without bloating disk.

## Telemetry (Phase 12 experimental modules)

`hamiltonian.rs` and `holographic.rs` are explicitly experimental.
They must instrument:
- Free-energy delta before/after each fusion step.
- Retrieval-quality delta on a held-out probe set.
- Reflection acceptance rate (user-accepted vs dismissed).

If after 30 days of telemetry these modules show no measurable
improvement over the simpler CAN + LSM + Langevin core, they will be
removed. Removal must be a clean module delete; nothing outside
`cognition::hamiltonian` and `cognition::holographic` may import them.

## Reflection output format

`.aura/brain/reflections/YYYY-MM-DD/HHmmss_<slug>.md`:

```markdown
---
trigger: idle | curiosity | contradiction | user_query
energy_before: 0.473
energy_after: 0.291
references:
  - block_id: 01j5...
    path: areas/work/example.md
---

# Insight: ...

(LLM-generated narrative referencing the cited blocks.)
```

User accepts → reflection becomes a normal note + Hebbian reinforcement
between the cited blocks. Dismisses → archived for curiosity to learn
which directions are unfruitful.
