//! Shared cortex state container.
//!
//! Holds the long-lived state vectors that the kernels operate on:
//! - `cognitive_state ∈ ℝ^D` (D=512 by default), driven by the CAN.
//! - `reservoir_state ∈ ℝ^R` (R=2048 by default), driven by the LSM.
//! - `synaptic_matrix` block-sparse, capped to fit the RAM budget.
//!
//! This struct is **pure state + no logic**. The perpetual loop and the
//! kernel orchestrator that integrate this state come in a later phase.
//!
//! Sizing follows `docs/COGNITIVE_LOOPS.md`.

/// Default cognitive-state dimension (matches the unified multimodal space).
pub const COGNITIVE_DIM: usize = 512;
/// Default reservoir dimension.
pub const RESERVOIR_DIM: usize = 2048;
/// Default Langevin inverse temperature `β`. Higher = colder = less noise.
pub const DEFAULT_BETA: f32 = 1.0;
/// Default integration timestep (10 ms).
pub const DEFAULT_DT: f32 = 0.01;
/// Default CAN time constant.
pub const DEFAULT_TAU: f32 = 1.0;
/// Default LSM leak rate.
pub const DEFAULT_ALPHA: f32 = 0.2;

pub struct SharedCortex {
    pub cognitive_state: Vec<f32>,
    pub reservoir_state: Vec<f32>,
    /// Per-tick scratch, owned here so the integrator doesn't allocate.
    pub scratch: Vec<f32>,
    pub config: CortexConfig,
}

#[derive(Clone, Debug)]
pub struct CortexConfig {
    pub cognitive_dim: usize,
    pub reservoir_dim: usize,
    pub beta: f32,
    pub dt: f32,
    pub tau: f32,
    pub alpha: f32,
}

impl Default for CortexConfig {
    fn default() -> Self {
        Self {
            cognitive_dim: COGNITIVE_DIM,
            reservoir_dim: RESERVOIR_DIM,
            beta: DEFAULT_BETA,
            dt: DEFAULT_DT,
            tau: DEFAULT_TAU,
            alpha: DEFAULT_ALPHA,
        }
    }
}

impl SharedCortex {
    pub fn new(config: CortexConfig) -> Self {
        let scratch_len = config.cognitive_dim.max(config.reservoir_dim);
        Self {
            cognitive_state: vec![0.0; config.cognitive_dim],
            reservoir_state: vec![0.0; config.reservoir_dim],
            scratch: vec![0.0; scratch_len],
            config,
        }
    }

    /// Total f32 memory footprint in bytes (state vectors only).
    /// Useful for the CPU/RAM budget tests.
    pub fn state_bytes(&self) -> usize {
        4 * (self.cognitive_state.len() + self.reservoir_state.len() + self.scratch.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_doc_specified_dims() {
        let cfg = CortexConfig::default();
        assert_eq!(cfg.cognitive_dim, 512);
        assert_eq!(cfg.reservoir_dim, 2048);
    }

    #[test]
    fn new_zeroes_state() {
        let c = SharedCortex::new(CortexConfig::default());
        assert!(c.cognitive_state.iter().all(|&x| x == 0.0));
        assert!(c.reservoir_state.iter().all(|&x| x == 0.0));
    }

    /// Memory budget: default state vectors must stay well under the 50 MB
    /// upper bound from `docs/COGNITIVE_LOOPS.md` (with headroom for the
    /// synaptic matrix and reflection buffers that come later).
    #[test]
    fn default_state_fits_within_memory_budget() {
        let c = SharedCortex::new(CortexConfig::default());
        // 4 * (512 + 2048 + 2048) = 4 * 4608 = 18,432 bytes ≈ 18 kB.
        let bytes = c.state_bytes();
        assert!(bytes < 50_000, "state too large: {bytes} bytes");
    }
}
