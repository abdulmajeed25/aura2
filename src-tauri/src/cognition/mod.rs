//! Cognitive core — v5.0 Phase 11.
//!
//! Numerical kernels for Aura's subconscious cortex. Each submodule is a
//! standalone, hand-verified kernel; the perpetual-loop orchestrator that
//! threads them together (`perpetual_loop`) is a later phase.
//!
//! Kernels in this commit:
//! - [`langevin`] — Gaussian noise sampling for the stochastic drift term.
//! - [`cans`] — Wilson-Cowan continuous attractor step.
//! - [`lsm`] — Liquid State Machine reservoir update.
//! - [`hebbian`] — Δw = η · x · y synaptic update.
//! - [`shared_cortex`] — state container (no logic).
//!
//! Hard rules followed:
//! - Math-driven TDD: every kernel ships with hand-computed expected outputs.
//! - No `unwrap()` outside tests. Constructors that can fail return `Result`.
//! - No mock data — kernels are exercised on the real numerical contract,
//!   not stand-in inputs.
//!
//! Status against `docs/STAND_IN_REGISTRY.md`:
//! - Stand-in #11 (Cognitive core) moves 🔴 → 🟡 (kernels in, perpetual
//!   loop + Hopfield + free-energy + curiosity still pending).

pub mod cans;
pub mod cortex;
pub mod curiosity;
pub mod free_energy;
pub mod hamiltonian;
pub mod hebbian;
pub mod holographic;
pub mod hopfield;
pub mod langevin;
pub mod lsm;
pub mod perpetual_loop;
pub mod reflection_writer;
pub mod shared_cortex;
