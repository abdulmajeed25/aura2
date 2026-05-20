//! Variational free energy.
//!
//! Aura uses the accuracy + complexity decomposition (Friston, 2010):
//!
//! ```text
//! F = (1/2) · Σᵢ (yᵢ - ŷᵢ)²                  ← accuracy: prediction error
//!   + (1/2) · ρ · Σᵢ (xᵢ - μᵢ)²              ← complexity: divergence from prior
//! ```
//!
//! where `y` is the observation, `ŷ` is the cortex's prediction, `x` is the
//! current cognitive state, `μ` is the prior mean, and `ρ` is the prior
//! precision (inverse variance). With unit observation precision the
//! likelihood term is just squared error.
//!
//! Properties tested:
//! - `F ≥ 0` always.
//! - `F = 0` iff `y = ŷ` AND `x = μ`.
//! - `F` is monotone in the prediction error.

#[derive(Debug, thiserror::Error)]
pub enum FreeEnergyError {
    #[error("observation dim {observation} doesn't match prediction dim {prediction}")]
    ObsPredMismatch { observation: usize, prediction: usize },
    #[error("state dim {state} doesn't match prior_mean dim {prior_mean}")]
    StatePriorMismatch { state: usize, prior_mean: usize },
    #[error("prior precision must be ≥ 0, got {0}")]
    InvalidPriorPrecision(f32),
}

/// Compute the free energy of `(observation, prediction, state, prior_mean)`
/// under the chosen prior precision.
pub fn compute(
    observation: &[f32],
    prediction: &[f32],
    state: &[f32],
    prior_mean: &[f32],
    prior_precision: f32,
) -> Result<f32, FreeEnergyError> {
    if observation.len() != prediction.len() {
        return Err(FreeEnergyError::ObsPredMismatch {
            observation: observation.len(),
            prediction: prediction.len(),
        });
    }
    if state.len() != prior_mean.len() {
        return Err(FreeEnergyError::StatePriorMismatch {
            state: state.len(),
            prior_mean: prior_mean.len(),
        });
    }
    if prior_precision < 0.0 {
        return Err(FreeEnergyError::InvalidPriorPrecision(prior_precision));
    }

    let mut accuracy = 0.0_f32;
    for i in 0..observation.len() {
        let d = observation[i] - prediction[i];
        accuracy += d * d;
    }
    let mut complexity = 0.0_f32;
    for i in 0..state.len() {
        let d = state[i] - prior_mean[i];
        complexity += d * d;
    }
    Ok(0.5 * accuracy + 0.5 * prior_precision * complexity)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Perfect prediction + state at prior → F = 0.
    #[test]
    fn perfect_prediction_and_aligned_state_gives_zero() {
        let f = compute(&[1.0, 2.0], &[1.0, 2.0], &[0.0, 0.0], &[0.0, 0.0], 1.0).unwrap();
        assert_eq!(f, 0.0);
    }

    /// Pure prediction-error case:
    ///   obs = [1, 0], pred = [0, 0], state = prior_mean
    ///   accuracy = 1² + 0² = 1, complexity = 0
    ///   F = 0.5 · 1 = 0.5
    #[test]
    fn prediction_error_dominates_when_state_aligned() {
        let f = compute(&[1.0, 0.0], &[0.0, 0.0], &[0.0, 0.0], &[0.0, 0.0], 1.0).unwrap();
        assert!((f - 0.5).abs() < 1e-6, "F = {f}, expected 0.5");
    }

    /// Pure complexity case: perfect prediction, state offset by 1 from prior.
    ///   accuracy = 0, complexity = Σᵢ 1² = n
    ///   With prior_precision = 2 and n = 3 → F = 0.5 · 2 · 3 = 3.0
    #[test]
    fn complexity_term_scales_with_precision_and_offset() {
        let f = compute(
            &[0.0, 0.0, 0.0],
            &[0.0, 0.0, 0.0],
            &[1.0, 1.0, 1.0],
            &[0.0, 0.0, 0.0],
            2.0,
        )
        .unwrap();
        assert!((f - 3.0).abs() < 1e-6, "F = {f}, expected 3.0");
    }

    /// F is non-negative for any inputs.
    #[test]
    fn f_is_nonnegative() {
        let f = compute(
            &[-3.0, 4.0],
            &[2.0, -1.0],
            &[-1.0, 0.5],
            &[0.0, 0.0],
            0.7,
        )
        .unwrap();
        assert!(f >= 0.0, "F should be non-negative, got {f}");
    }

    /// Doubling the prediction error quadruples the accuracy term (squared).
    #[test]
    fn accuracy_term_is_quadratic_in_error() {
        let f1 = compute(&[1.0], &[0.0], &[0.0], &[0.0], 0.0).unwrap();
        let f2 = compute(&[2.0], &[0.0], &[0.0], &[0.0], 0.0).unwrap();
        // f1 = 0.5, f2 = 2.0 → ratio = 4
        assert!((f2 / f1 - 4.0).abs() < 1e-6, "ratio = {}", f2 / f1);
    }

    /// Mismatched obs/pred dims return an error, not a panic.
    #[test]
    fn obs_pred_mismatch_errors() {
        let err = compute(&[1.0, 2.0], &[1.0], &[0.0], &[0.0], 1.0).unwrap_err();
        assert!(matches!(err, FreeEnergyError::ObsPredMismatch { .. }));
    }
}
