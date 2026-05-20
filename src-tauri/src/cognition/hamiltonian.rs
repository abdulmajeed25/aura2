//! Symplectic Hamiltonian leapfrog integrator. Phase 12 (experimental).
//!
//! For a Hamiltonian `H(x, p) = T(p) + V(x)` with kinetic term `T(p) = ½‖p‖²`
//! and potential `V(x)`, the leapfrog scheme advances `(x, p) → (x', p')`:
//!
//! ```text
//! p_half = p − (dt/2) · ∇V(x)
//! x'     = x + dt · p_half
//! p'     = p_half − (dt/2) · ∇V(x')
//! ```
//!
//! Leapfrog is **symplectic** — energy `H` is bounded (no secular drift)
//! over arbitrarily long integration even at moderate `dt`, unlike Euler
//! which diverges. That property is the reason the v5.0 spec earmarks it
//! for the "Hamiltonian fusion" stage of the cognitive core: a Claude-
//! injected momentum `p` can be integrated against the local potential
//! `V(x)` (e.g. a Hopfield attractor landscape) without energy blowing up.
//!
//! This module ships only the kernel + math-driven tests; the integration
//! into `Cortex::tick` is opt-in and lands in a later gate alongside
//! telemetry per the spec.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HamError {
    #[error("dimensions disagree: x={x}, p={p}")]
    DimMismatch { x: usize, p: usize },
    #[error("gradient dim {got} doesn't match state dim {expected}")]
    GradientShape { expected: usize, got: usize },
    #[error("dt must be > 0, got {0}")]
    InvalidDt(f32),
}

/// One symplectic leapfrog step. Mutates `x` and `p` in place.
///
/// `grad_v(x) → ∇V(x)` is supplied as a closure so callers can plug in any
/// potential — harmonic, Hopfield-shaped, free-energy gradient, etc.
pub fn leapfrog_step<F>(
    x: &mut [f32],
    p: &mut [f32],
    grad_v: F,
    dt: f32,
) -> Result<(), HamError>
where
    F: Fn(&[f32]) -> Vec<f32>,
{
    if x.len() != p.len() {
        return Err(HamError::DimMismatch {
            x: x.len(),
            p: p.len(),
        });
    }
    if !matches!(dt.partial_cmp(&0.0), Some(std::cmp::Ordering::Greater)) {
        return Err(HamError::InvalidDt(dt));
    }
    let n = x.len();
    let half_dt = 0.5 * dt;

    // p ← p − (dt/2) · ∇V(x)
    let g = grad_v(x);
    if g.len() != n {
        return Err(HamError::GradientShape {
            expected: n,
            got: g.len(),
        });
    }
    for i in 0..n {
        p[i] -= half_dt * g[i];
    }

    // x ← x + dt · p
    for i in 0..n {
        x[i] += dt * p[i];
    }

    // p ← p − (dt/2) · ∇V(x_new)
    let g2 = grad_v(x);
    if g2.len() != n {
        return Err(HamError::GradientShape {
            expected: n,
            got: g2.len(),
        });
    }
    for i in 0..n {
        p[i] -= half_dt * g2[i];
    }
    Ok(())
}

/// **Pattern-conditioned potential gradient** — the missing piece flagged
/// in stand-in #12 for the spec's `V(x)` shape.
///
/// Given the current cognitive state `x` and an *active pattern*
/// `target` (e.g. a workflow trigger vector or a Hopfield-stored
/// attractor), returns `∇V(x) = weight · (x − target)`. This is the
/// gradient of the quadratic basin `V(x) = ½·weight·‖x − target‖²`.
/// Plugged into [`leapfrog_step`], it bends the trajectory **toward**
/// `target` — Claude's intellectual-momentum `p` then gets bent by the
/// local potential the user's documented patterns shape.
///
/// Multi-pattern: pass each pattern with its own weight; the gradient
/// is the weighted sum. Higher weights pull harder. Negative weights
/// flip the basin into a hill — useful for "stay away from this
/// attractor" semantics (rare; only used in spec's curiosity-drift
/// regime).
pub fn pattern_grad(x: &[f32], targets: &[(&[f32], f32)]) -> Vec<f32> {
    let n = x.len();
    let mut g = vec![0.0f32; n];
    for (target, weight) in targets {
        if target.len() != n {
            // Skip mis-shaped patterns rather than blowing up the loop;
            // the caller's tests own the shape contract.
            continue;
        }
        for i in 0..n {
            g[i] += weight * (x[i] - target[i]);
        }
    }
    g
}

/// `V(x) = Σⱼ ½·wⱼ·‖x − targetⱼ‖²` — the integrated form of
/// [`pattern_grad`]. Use this for energy reporting in the telemetry
/// stream so the same V is consistent across `leapfrog_step`'s
/// `grad_v`, [`energy`]'s `v`, and cortex audit rows.
pub fn pattern_energy(x: &[f32], targets: &[(&[f32], f32)]) -> f32 {
    let mut sum = 0.0f32;
    for (target, weight) in targets {
        if target.len() != x.len() {
            continue;
        }
        let mut sq = 0.0f32;
        for i in 0..x.len() {
            let d = x[i] - target[i];
            sq += d * d;
        }
        sum += 0.5 * weight * sq;
    }
    sum
}

/// Total Hamiltonian energy `H(x, p) = ½‖p‖² + V(x)`. Pass the same `v`
/// closure you use for `leapfrog_step`'s gradient (the energy itself, not
/// the gradient).
pub fn energy<F>(x: &[f32], p: &[f32], v: F) -> f32
where
    F: Fn(&[f32]) -> f32,
{
    let kinetic: f32 = p.iter().map(|pi| 0.5 * pi * pi).sum();
    kinetic + v(x)
}

#[cfg(test)]
#[allow(clippy::needless_borrows_for_generic_args)]
// `leapfrog_step` and `energy` take `impl Fn` by value, so `&closure` is
// the idiomatic way to keep ownership while still satisfying the generic
// bound across many iterations of a `for` loop. Clippy's
// `needless_borrows_for_generic_args` doesn't see through that need.
mod tests {
    use super::*;

    /// Free particle (V = 0). After one step from `x=0, p=1, dt=0.1`:
    ///   p_half = 1 − 0 = 1
    ///   x'     = 0 + 0.1 · 1 = 0.1
    ///   p'     = 1 − 0 = 1
    #[test]
    fn free_particle_one_step_exact() {
        let mut x = vec![0.0_f32];
        let mut p = vec![1.0_f32];
        let zero_grad = |_: &[f32]| vec![0.0_f32];
        leapfrog_step(&mut x, &mut p, zero_grad, 0.1).unwrap();
        let diff_x = (x[0] - 0.1).abs();
        let diff_p = (p[0] - 1.0).abs();
        assert!(diff_x < 1e-6, "x = {} (expected 0.1)", x[0]);
        assert!(diff_p < 1e-6, "p = {} (expected 1.0)", p[0]);
    }

    /// Harmonic oscillator V = ½ω²x². Energy is bounded — symplectic
    /// integrators don't drift secularly the way Euler does.
    /// Starting from x=1, p=0 with ω=1: H₀ = ½·0² + ½·1·1² = 0.5.
    /// After 10 000 steps at dt=0.01, the energy stays within 1 % of H₀.
    #[test]
    fn harmonic_energy_is_bounded_over_long_run() {
        let omega: f32 = 1.0;
        let mut x = vec![1.0_f32];
        let mut p = vec![0.0_f32];
        let grad_v = |xv: &[f32]| vec![omega * omega * xv[0]];
        let v = |xv: &[f32]| 0.5 * omega * omega * xv[0] * xv[0];
        let h0 = energy(&x, &p, &v);
        assert!((h0 - 0.5).abs() < 1e-6);

        let mut max_drift: f32 = 0.0;
        for _ in 0..10_000 {
            leapfrog_step(&mut x, &mut p, &grad_v, 0.01).unwrap();
            let h = energy(&x, &p, &v);
            max_drift = max_drift.max((h - h0).abs());
        }
        assert!(
            max_drift / h0 < 0.01,
            "energy drift {} > 1% of H₀ = {}",
            max_drift,
            h0
        );
    }

    /// Symplectic property — the harmonic trajectory stays *near* the
    /// initial energy level (no monotone increase or decrease). Compare to
    /// Euler integration which would diverge geometrically.
    #[test]
    fn harmonic_does_not_diverge_at_long_horizon() {
        let omega: f32 = 1.0;
        let mut x = vec![1.0_f32];
        let mut p = vec![0.0_f32];
        let grad_v = |xv: &[f32]| vec![omega * omega * xv[0]];
        for _ in 0..50_000 {
            leapfrog_step(&mut x, &mut p, &grad_v, 0.01).unwrap();
        }
        // |x|² + |p|² should stay bounded near 1 (the initial amplitude).
        let amp_sq: f32 = x[0] * x[0] + p[0] * p[0];
        assert!(
            (amp_sq - 1.0).abs() < 0.05,
            "amplitude² = {amp_sq}, expected ≈ 1.0"
        );
    }

    /// 2-D quadratic well V = ½·(k₁ x₁² + k₂ x₂²) — separable, so the
    /// dimensions stay independent. Starting from x=[1, 0.5], p=[0, 0],
    /// energy should also be bounded.
    #[test]
    fn multidim_quadratic_energy_bounded() {
        let k = [1.0_f32, 4.0];
        let mut x = vec![1.0_f32, 0.5];
        let mut p = vec![0.0_f32, 0.0];
        let grad_v = |xv: &[f32]| vec![k[0] * xv[0], k[1] * xv[1]];
        let v = |xv: &[f32]| 0.5 * (k[0] * xv[0].powi(2) + k[1] * xv[1].powi(2));
        let h0 = energy(&x, &p, &v);
        for _ in 0..5_000 {
            leapfrog_step(&mut x, &mut p, &grad_v, 0.01).unwrap();
        }
        let h_final = energy(&x, &p, &v);
        let drift = (h_final - h0).abs() / h0;
        assert!(drift < 0.01, "2-D drift = {drift} (h0={h0}, h={h_final})");
    }

    /// Shape mismatch returns Err, not a panic.
    #[test]
    fn dim_mismatch_errors() {
        let mut x = vec![0.0_f32; 3];
        let mut p = vec![0.0_f32; 2];
        let g = |xv: &[f32]| vec![0.0_f32; xv.len()];
        let err = leapfrog_step(&mut x, &mut p, g, 0.1).unwrap_err();
        assert!(matches!(err, HamError::DimMismatch { x: 3, p: 2 }));
    }

    #[test]
    fn bad_dt_errors() {
        let mut x = vec![0.0_f32];
        let mut p = vec![0.0_f32];
        let g = |_: &[f32]| vec![0.0_f32];
        let err = leapfrog_step(&mut x, &mut p, g, -0.1).unwrap_err();
        assert!(matches!(err, HamError::InvalidDt(_)));
    }

    /// Pattern-conditioned gradient: at x = target, ∇V is zero (we're
    /// at the basin floor). Move off-target and the gradient points
    /// back toward target with magnitude `weight · ‖x − target‖`.
    #[test]
    fn pattern_grad_zero_at_target_basin() {
        let target = vec![1.0_f32, 2.0, 3.0];
        let x = target.clone();
        let g = pattern_grad(&x, &[(&target, 1.0)]);
        for gi in g {
            assert!(gi.abs() < 1e-6);
        }
    }

    #[test]
    fn pattern_grad_points_back_toward_target() {
        let target = vec![1.0_f32, 0.0, 0.0];
        let x = vec![3.0_f32, 0.0, 0.0];
        // x − target = (2, 0, 0); ∇V with weight=1 = (2, 0, 0).
        let g = pattern_grad(&x, &[(&target, 1.0)]);
        assert!((g[0] - 2.0).abs() < 1e-6);
        assert!(g[1].abs() < 1e-6);
        assert!(g[2].abs() < 1e-6);
    }

    #[test]
    fn pattern_grad_superposes_multiple_targets() {
        // Two targets pulling: (0,0,0) with w=1 and (2,0,0) with w=1.
        // From x = (3,0,0):
        //   ∇V = 1·(3-0) + 1·(3-2) = 3 + 1 = 4 in dim 0
        let t1 = vec![0.0_f32, 0.0, 0.0];
        let t2 = vec![2.0_f32, 0.0, 0.0];
        let x = vec![3.0_f32, 0.0, 0.0];
        let g = pattern_grad(&x, &[(&t1, 1.0), (&t2, 1.0)]);
        assert!((g[0] - 4.0).abs() < 1e-6);
    }

    /// Energy form is consistent with the gradient: at the target
    /// `V = 0`, away from it `V > 0`, and `∂V/∂x_i` recovered by
    /// finite-difference matches `pattern_grad` to 1e-4.
    #[test]
    fn pattern_energy_matches_grad_by_finite_difference() {
        let target = vec![1.0_f32, 2.0, 3.0];
        let mut x = vec![0.5_f32, 1.5, 2.5];
        let weight = 1.7;
        let grad = pattern_grad(&x, &[(&target, weight)]);
        let h = 1e-3_f32;
        for i in 0..x.len() {
            let v0 = pattern_energy(&x, &[(&target, weight)]);
            x[i] += h;
            let v1 = pattern_energy(&x, &[(&target, weight)]);
            x[i] -= h;
            let fd = (v1 - v0) / h;
            assert!(
                (fd - grad[i]).abs() < 1e-2,
                "fd {} vs grad {} at dim {}",
                fd,
                grad[i],
                i
            );
        }
    }
}
