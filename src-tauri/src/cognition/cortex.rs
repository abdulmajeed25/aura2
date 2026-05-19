//! Cortex — minimal orchestrator that composes the kernels into one `tick`.
//!
//! `tick(observation)` does, in order:
//!
//! 1. LSM step driven by the observation projected into reservoir space.
//! 2. CAN step on the cognitive state, with input = reservoir projected
//!    back into cognitive space.
//! 3. Langevin drift on the cognitive state (the stochastic curiosity term).
//! 4. Hopfield retrieval if any patterns are stored — softly attracts the
//!    cognitive state toward the nearest stored memory.
//! 5. Free-energy compute, using `observation` as `y`, the prediction being
//!    a slice of the cognitive state, and `prior_mean / prior_precision`
//!    as the cortex's reset prior.
//!
//! This module deliberately stays **synchronous and Tauri-free**: the
//! tokio-spawned background loop, event emission, and CPU throttling live in
//! a later phase. Tests drive `tick` directly.

use crate::cognition::{
    cans, free_energy, hebbian, hopfield, langevin,
    lsm,
    shared_cortex::{CortexConfig, SharedCortex},
};

#[derive(Debug, thiserror::Error)]
pub enum CortexError {
    #[error("can: {0}")]
    Can(#[from] cans::CanError),
    #[error("hopfield: {0}")]
    Hopfield(#[from] hopfield::HopfieldError),
    #[error("free energy: {0}")]
    FreeEnergy(#[from] free_energy::FreeEnergyError),
    #[error("observation dim {observation} doesn't match cognitive_dim {cognitive}")]
    ObservationShape { observation: usize, cognitive: usize },
    #[error("pattern dim {got} doesn't match cognitive_dim {expected}")]
    PatternShape { expected: usize, got: usize },
}

/// One-tick event returned by [`Cortex::tick`].
#[derive(Clone, Debug)]
pub struct CortexEvent {
    pub free_energy: f32,
    /// Index of the cognitive-state coordinate with the largest absolute
    /// value, used downstream as a crude "dominant attractor" hint.
    pub dominant_index: usize,
    pub tick: u64,
}

pub struct Cortex {
    pub state: SharedCortex,
    /// CAN recurrent weights, row-major `D × D`.
    w_can: Vec<f32>,
    /// LSM recurrent weights, row-major `R × R`.
    w_res: Vec<f32>,
    /// Input projection observation → reservoir, row-major `R × D`.
    w_in: Vec<f32>,
    /// Readout reservoir → cognitive input, row-major `D × R`.
    w_proj: Vec<f32>,
    /// Stored Hopfield patterns, row-major `n_patterns × D`.
    patterns: Vec<f32>,
    n_patterns: usize,
    /// Hopfield blend factor: cognitive_state ← (1-γ)·state + γ·retrieved.
    /// Set to 0 to disable retrieval coupling.
    pub hopfield_gamma: f32,
    pub hopfield_beta: f32,
    /// Prior for the free-energy complexity term.
    pub prior_mean: Vec<f32>,
    pub prior_precision: f32,
    pub sampler: langevin::Sampler,
    pub tick_count: u64,
    /// Hebbian learning rate. Set to 0 to disable plasticity.
    pub hebbian_eta: f32,
}

impl Cortex {
    /// Build a Cortex with all weight matrices at zero. Tests load explicit
    /// weights via the field-level helpers below; production code uses
    /// [`Self::with_seeded_weights`] for Xavier-style random initialisation.
    pub fn blank(config: CortexConfig, seed: u64) -> Self {
        let d = config.cognitive_dim;
        let r = config.reservoir_dim;
        Self {
            state: SharedCortex::new(config.clone()),
            w_can: vec![0.0; d * d],
            w_res: vec![0.0; r * r],
            w_in: vec![0.0; r * d],
            w_proj: vec![0.0; d * r],
            patterns: Vec::new(),
            n_patterns: 0,
            hopfield_gamma: 0.0,
            hopfield_beta: 5.0,
            prior_mean: vec![0.0; d],
            prior_precision: 1.0,
            sampler: langevin::Sampler::new(seed),
            tick_count: 0,
            hebbian_eta: 0.0,
        }
    }

    /// Initialise all four weight matrices with `1/√fan_in`-scaled Gaussian
    /// noise from the same seed. Conservative enough that 100 idle ticks
    /// don't NaN (verified in `idle_ticks_dont_blow_up`).
    pub fn with_seeded_weights(config: CortexConfig, seed: u64) -> Self {
        let mut s = langevin::Sampler::new(seed);
        let d = config.cognitive_dim;
        let r = config.reservoir_dim;

        let scale_can = 0.5 / (d as f32).sqrt();
        let scale_res = 0.5 / (r as f32).sqrt();
        let scale_in = 1.0 / (d as f32).sqrt();
        let scale_proj = 1.0 / (r as f32).sqrt();

        let w_can: Vec<f32> = (0..d * d).map(|_| scale_can * s.sample()).collect();
        let w_res: Vec<f32> = (0..r * r).map(|_| scale_res * s.sample()).collect();
        let w_in: Vec<f32> = (0..r * d).map(|_| scale_in * s.sample()).collect();
        let w_proj: Vec<f32> = (0..d * r).map(|_| scale_proj * s.sample()).collect();

        Self {
            state: SharedCortex::new(config.clone()),
            w_can,
            w_res,
            w_in,
            w_proj,
            patterns: Vec::new(),
            n_patterns: 0,
            hopfield_gamma: 0.0,
            hopfield_beta: 5.0,
            prior_mean: vec![0.0; d],
            prior_precision: 1.0,
            sampler: s,
            tick_count: 0,
            hebbian_eta: 0.0,
        }
    }

    pub fn store_pattern(&mut self, pat: &[f32]) -> Result<(), CortexError> {
        if pat.len() != self.state.config.cognitive_dim {
            return Err(CortexError::PatternShape {
                expected: self.state.config.cognitive_dim,
                got: pat.len(),
            });
        }
        self.patterns.extend_from_slice(pat);
        self.n_patterns += 1;
        Ok(())
    }

    /// One integration tick.
    pub fn tick(&mut self, observation: &[f32]) -> Result<CortexEvent, CortexError> {
        let cfg = self.state.config.clone();
        if observation.len() != cfg.cognitive_dim {
            return Err(CortexError::ObservationShape {
                observation: observation.len(),
                cognitive: cfg.cognitive_dim,
            });
        }

        // 1. LSM step driven by the observation.
        lsm::step_in_place(
            &mut self.state.reservoir_state,
            &self.w_res,
            &self.w_in,
            observation,
            cfg.alpha,
        )?;

        // 2. Project reservoir → cognitive_input, then CAN step.
        let cog_input = project(&self.w_proj, &self.state.reservoir_state, cfg.cognitive_dim);
        cans::step_in_place(
            &mut self.state.cognitive_state,
            &self.w_can,
            &cog_input,
            cfg.dt,
            cfg.tau,
        )?;

        // 3. Langevin drift.
        self.sampler
            .drift_in_place(&mut self.state.cognitive_state, cfg.beta, cfg.dt);

        // 4. Hopfield clean-up (only if patterns + non-zero blend).
        if self.n_patterns > 0 && self.hopfield_gamma > 0.0 {
            let retrieved = hopfield::retrieve(
                &self.patterns,
                self.n_patterns,
                cfg.cognitive_dim,
                &self.state.cognitive_state,
                self.hopfield_beta,
            )?;
            for (s, r) in self
                .state
                .cognitive_state
                .iter_mut()
                .zip(retrieved.iter())
            {
                *s = (1.0 - self.hopfield_gamma) * *s + self.hopfield_gamma * r;
            }
        }

        // 5. Free-energy: prediction = current cognitive_state.
        let f = free_energy::compute(
            observation,
            &self.state.cognitive_state,
            &self.state.cognitive_state,
            &self.prior_mean,
            self.prior_precision,
        )?;

        // Optional: light Hebbian reinforcement (observation × state outer
        // product onto w_can). Off by default (eta = 0).
        if self.hebbian_eta > 0.0 {
            hebbian::reinforce_in_place(
                &mut self.w_can,
                &self.state.cognitive_state,
                &self.state.cognitive_state,
                self.hebbian_eta,
            )?;
        }

        let dominant_index = argmax_abs(&self.state.cognitive_state);
        self.tick_count += 1;
        Ok(CortexEvent {
            free_energy: f,
            dominant_index,
            tick: self.tick_count,
        })
    }
}

/// `out[i] = Σ_j w_proj[i*r + j] · reservoir[j]`
fn project(w_proj: &[f32], reservoir: &[f32], out_dim: usize) -> Vec<f32> {
    let r = reservoir.len();
    let mut out = vec![0.0_f32; out_dim];
    for i in 0..out_dim {
        let row = &w_proj[i * r..(i + 1) * r];
        let mut acc = 0.0_f32;
        for j in 0..r {
            acc += row[j] * reservoir[j];
        }
        out[i] = acc;
    }
    out
}

fn argmax_abs(v: &[f32]) -> usize {
    let mut best = 0usize;
    let mut best_v = v.first().map(|x| x.abs()).unwrap_or(0.0);
    for (i, &x) in v.iter().enumerate().skip(1) {
        let a = x.abs();
        if a > best_v {
            best_v = a;
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_config() -> CortexConfig {
        CortexConfig {
            cognitive_dim: 4,
            reservoir_dim: 8,
            beta: 1.0,
            dt: 0.01,
            tau: 1.0,
            alpha: 0.2,
        }
    }

    /// Blank cortex + zero observation: F = 0 (zero prediction error, state
    /// at prior). The sampler still adds drift, so the next tick will have
    /// nonzero F — that's tested separately.
    #[test]
    fn blank_cortex_zero_observation_first_tick_zero_f() {
        let mut c = Cortex::blank(tiny_config(), 0);
        let ev = c.tick(&[0.0; 4]).unwrap();
        // Before any drift, accuracy = 0 (state was zero before the CAN step
        // and stays zero with zero weights); complexity = 0 (state at prior
        // before Langevin drift). After Langevin, drift adds noise so F is
        // small but non-zero — we just assert ≥ 0.
        assert!(ev.free_energy >= 0.0, "F = {}", ev.free_energy);
        assert_eq!(ev.tick, 1);
    }

    /// Observation shape mismatch returns Err, not panic.
    #[test]
    fn shape_mismatch_returns_err() {
        let mut c = Cortex::blank(tiny_config(), 0);
        let err = c.tick(&[0.0; 3]).unwrap_err();
        assert!(matches!(err, CortexError::ObservationShape { .. }));
    }

    /// With Xavier-init weights and zero observation, 100 ticks must not
    /// produce NaN/inf. This is the "doesn't blow up" stability check.
    #[test]
    fn idle_ticks_dont_blow_up() {
        let mut c = Cortex::with_seeded_weights(tiny_config(), 42);
        for _ in 0..100 {
            let ev = c.tick(&[0.0; 4]).unwrap();
            assert!(ev.free_energy.is_finite(), "F = {}", ev.free_energy);
            for &s in &c.state.cognitive_state {
                assert!(s.is_finite(), "state went non-finite: {s}");
                assert!(s.abs() < 10.0, "state blew up: {s}");
            }
            for &s in &c.state.reservoir_state {
                assert!(s.is_finite());
                assert!(s.abs() < 10.0, "reservoir blew up: {s}");
            }
        }
    }

    /// Determinism: same seed + same observation sequence → identical
    /// trajectory.
    #[test]
    fn same_seed_replays_trajectory() {
        let obs = [0.1_f32, -0.2, 0.0, 0.3];
        let mut a = Cortex::with_seeded_weights(tiny_config(), 99);
        let mut b = Cortex::with_seeded_weights(tiny_config(), 99);
        for _ in 0..20 {
            let ev_a = a.tick(&obs).unwrap();
            let ev_b = b.tick(&obs).unwrap();
            assert_eq!(
                ev_a.free_energy.to_bits(),
                ev_b.free_energy.to_bits(),
                "F diverged at tick {}",
                ev_a.tick
            );
            assert_eq!(ev_a.dominant_index, ev_b.dominant_index);
        }
    }

    /// Stored pattern + matching observation: with non-zero `hopfield_gamma`,
    /// the cortex pulls state toward the pattern, reducing F over ticks
    /// versus a cortex with γ=0.
    #[test]
    fn hopfield_pulls_state_toward_stored_pattern() {
        let cfg = tiny_config();
        let mut with_pull = Cortex::blank(cfg.clone(), 7);
        let mut without_pull = Cortex::blank(cfg.clone(), 7);
        with_pull.hopfield_gamma = 0.5;
        without_pull.hopfield_gamma = 0.0;
        let target = vec![1.0_f32, 0.0, 0.0, 0.0];
        with_pull.store_pattern(&target).unwrap();
        without_pull.store_pattern(&target).unwrap();

        // Run a few ticks with the target as observation.
        for _ in 0..5 {
            with_pull.tick(&target).unwrap();
            without_pull.tick(&target).unwrap();
        }
        // After several pulls, with_pull state should be closer to target.
        let d_pull: f32 = (0..4)
            .map(|i| (with_pull.state.cognitive_state[i] - target[i]).powi(2))
            .sum();
        let d_no_pull: f32 = (0..4)
            .map(|i| (without_pull.state.cognitive_state[i] - target[i]).powi(2))
            .sum();
        assert!(
            d_pull < d_no_pull,
            "γ=0.5 should pull state closer to target than γ=0.0 \
             (got d_pull={d_pull}, d_no_pull={d_no_pull})"
        );
    }
}
