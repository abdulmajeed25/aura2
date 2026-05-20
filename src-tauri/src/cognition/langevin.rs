//! Langevin noise sampling.
//!
//! The cortex's stochastic drift term is `√(2β⁻¹) · dW_t`, where `dW_t` is
//! white Gaussian noise. This module produces deterministic Gaussian samples
//! via Box-Muller from a `ChaCha8` stream so a given seed always replays the
//! same trajectory — essential for reproducible cognition tests.
//!
//! Math contract (verified in tests):
//!
//! 1. With seed `s`, [`Sampler::new(s)`] then `n` calls to `sample` produces
//!    a sequence whose sample mean → 0 and sample variance → 1 as `n → ∞`.
//! 2. Independence: cross-correlation of two seeds is at chance.
//! 3. The drift form `step(state, beta, dt)` adds noise scaled by
//!    `√(2 · dt / β)` to each coordinate.

use rand::Rng;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Deterministic Gaussian source. `next_uniform_pair` is the underlying
/// `(u1, u2)` draw; `sample` returns the Box-Muller pair cached one at a time.
pub struct Sampler {
    rng: ChaCha8Rng,
    cached: Option<f32>,
}

impl Sampler {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            cached: None,
        }
    }

    /// One draw from `N(0, 1)`. Box-Muller transform.
    pub fn sample(&mut self) -> f32 {
        if let Some(z) = self.cached.take() {
            return z;
        }
        // Both u1 ∈ (0, 1] so ln(u1) is finite.
        let u1: f32 = loop {
            let v: f32 = self.rng.gen();
            if v > f32::EPSILON {
                break v;
            }
        };
        let u2: f32 = self.rng.gen();
        let r = (-2.0_f32 * u1.ln()).sqrt();
        let theta = 2.0_f32 * std::f32::consts::PI * u2;
        let z0 = r * theta.cos();
        let z1 = r * theta.sin();
        self.cached = Some(z1);
        z0
    }

    /// Apply Langevin drift to a state vector in place.
    /// `dx_i = √(2 · dt / β) · ξ_i` where `ξ_i ∼ N(0, 1)`.
    pub fn drift_in_place(&mut self, state: &mut [f32], beta: f32, dt: f32) {
        let scale = (2.0 * dt / beta).sqrt();
        for s in state.iter_mut() {
            *s += scale * self.sample();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-verified contract: 10_000 samples from chacha8(seed=42).
    /// Theoretical mean 0, variance 1; for n=10_000 the standard error of
    /// the mean is 1/√n = 0.01, so |mean| < 0.05 is well within 5σ.
    #[test]
    fn sample_mean_and_variance_match_standard_normal() {
        let mut sampler = Sampler::new(42);
        let n = 10_000;
        let mut samples = Vec::with_capacity(n);
        for _ in 0..n {
            samples.push(sampler.sample());
        }
        let mean: f32 = samples.iter().sum::<f32>() / n as f32;
        let var: f32 =
            samples.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n as f32;

        assert!(
            mean.abs() < 0.05,
            "sample mean {mean} too far from 0 (n={n})"
        );
        assert!(
            (var - 1.0).abs() < 0.05,
            "sample variance {var} too far from 1 (n={n})"
        );
    }

    /// Two different seeds produce uncorrelated streams.
    #[test]
    fn distinct_seeds_are_uncorrelated() {
        let mut a = Sampler::new(1);
        let mut b = Sampler::new(2);
        let n = 4_000;
        let mut sum_xy = 0.0_f32;
        let mut sum_x = 0.0_f32;
        let mut sum_y = 0.0_f32;
        let mut sum_xx = 0.0_f32;
        let mut sum_yy = 0.0_f32;
        for _ in 0..n {
            let x = a.sample();
            let y = b.sample();
            sum_xy += x * y;
            sum_x += x;
            sum_y += y;
            sum_xx += x * x;
            sum_yy += y * y;
        }
        let nf = n as f32;
        let cov = (sum_xy - sum_x * sum_y / nf) / nf;
        let var_x = (sum_xx - sum_x * sum_x / nf) / nf;
        let var_y = (sum_yy - sum_y * sum_y / nf) / nf;
        let rho = cov / (var_x.sqrt() * var_y.sqrt());
        assert!(
            rho.abs() < 0.05,
            "correlation {rho} between seeds 1 and 2 not at chance"
        );
    }

    /// Same seed twice produces the identical sequence (determinism).
    #[test]
    fn replays_with_same_seed() {
        let mut a = Sampler::new(7);
        let mut b = Sampler::new(7);
        for _ in 0..50 {
            assert_eq!(a.sample().to_bits(), b.sample().to_bits());
        }
    }

    /// `drift_in_place` adds noise with the right amplitude:
    /// `Var(Δ_i) = 2·dt/β`. With dt=0.01, β=1, expected per-coord var = 0.02.
    #[test]
    fn drift_amplitude_matches_formula() {
        let dim = 200;
        let n_trials = 500;
        let dt = 0.01_f32;
        let beta = 1.0_f32;
        let expected_var = 2.0 * dt / beta; // 0.02
        let mut sampler = Sampler::new(123);

        let mut variances = Vec::with_capacity(n_trials);
        for _ in 0..n_trials {
            let mut state = vec![0.0_f32; dim];
            sampler.drift_in_place(&mut state, beta, dt);
            let mean: f32 = state.iter().sum::<f32>() / dim as f32;
            let var: f32 =
                state.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / dim as f32;
            variances.push(var);
        }
        let avg_var: f32 = variances.iter().sum::<f32>() / n_trials as f32;
        let rel_err = ((avg_var - expected_var) / expected_var).abs();
        assert!(
            rel_err < 0.05,
            "avg variance {avg_var} vs expected {expected_var} (rel err {rel_err})"
        );
    }
}
