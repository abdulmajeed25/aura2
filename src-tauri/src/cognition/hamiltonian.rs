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
}
