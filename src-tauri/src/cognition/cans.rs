//! Continuous Attractor Network — Wilson-Cowan style.
//!
//! Update rule, integrated by Euler:
//!
//! ```text
//! τ · dx/dt = -x + σ(W·x + I)
//! ```
//!
//! where `σ = tanh`, `W` is a recurrent weight matrix, `I` is external input.
//! Discrete Euler step with timestep `dt`:
//!
//! ```text
//! x ← x + (dt / τ) · (-x + tanh(W·x + I))
//! ```
//!
//! For Phase 11 we expose [`step_in_place`] taking the state, the weight
//! matrix (row-major flat `Vec<f32>`), and the input vector. The full CAN
//! orchestrator that picks attractor labels and broadcasts events is later.

/// One Euler step of the Wilson-Cowan equation, in place on `state`.
///
/// Panics in debug if `weights.len() != n * n` or `input.len() != n`,
/// returns an `Err` in release. (Hard Rule #4: no `unwrap()` in production
/// paths — we choose `Err` over panic.)
pub fn step_in_place(
    state: &mut [f32],
    weights: &[f32],
    input: &[f32],
    dt: f32,
    tau: f32,
) -> Result<(), CanError> {
    let n = state.len();
    if weights.len() != n * n {
        return Err(CanError::WeightsShape {
            expected: n * n,
            got: weights.len(),
        });
    }
    if input.len() != n {
        return Err(CanError::InputShape {
            expected: n,
            got: input.len(),
        });
    }
    if tau <= 0.0 || dt <= 0.0 {
        return Err(CanError::InvalidTimestep { dt, tau });
    }

    // pre_activation_i = Σ_j W[i,j] · x[j] + I[i]
    let mut pre = vec![0.0_f32; n];
    for i in 0..n {
        let row = &weights[i * n..(i + 1) * n];
        let mut acc = 0.0_f32;
        for j in 0..n {
            acc += row[j] * state[j];
        }
        pre[i] = acc + input[i];
    }

    let alpha = dt / tau;
    for i in 0..n {
        let target = pre[i].tanh();
        state[i] += alpha * (-state[i] + target);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum CanError {
    #[error("weights shape mismatch: expected {expected}, got {got}")]
    WeightsShape { expected: usize, got: usize },
    #[error("input shape mismatch: expected {expected}, got {got}")]
    InputShape { expected: usize, got: usize },
    #[error("invalid timestep: dt={dt}, tau={tau} (both must be > 0)")]
    InvalidTimestep { dt: f32, tau: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zero state, zero weights, zero input — should stay at zero.
    #[test]
    fn zero_everything_is_a_fixed_point() {
        let mut state = vec![0.0_f32; 3];
        let weights = vec![0.0_f32; 9];
        let input = vec![0.0_f32; 3];
        step_in_place(&mut state, &weights, &input, 0.1, 1.0).unwrap();
        for &s in &state {
            assert_eq!(s, 0.0, "zero fixed point broken");
        }
    }

    /// Hand-computed single step:
    ///   n=1, x=0.5, W=[0], I=[0], dt=0.1, τ=1
    ///   pre = 0·0.5 + 0 = 0; target = tanh(0) = 0
    ///   Δ = 0.1 · (-0.5 + 0) = -0.05
    ///   x' = 0.45
    #[test]
    fn pure_decay_single_step_is_exact() {
        let mut state = vec![0.5];
        let weights = vec![0.0];
        let input = vec![0.0];
        step_in_place(&mut state, &weights, &input, 0.1, 1.0).unwrap();
        let diff = (state[0] - 0.45_f32).abs();
        assert!(diff < 1e-6, "expected 0.45, got {} (diff {diff})", state[0]);
    }

    /// Hand-computed single step with input only:
    ///   n=1, x=0.0, W=[0], I=[1.0], dt=0.1, τ=1
    ///   pre = 0 + 1 = 1; target = tanh(1) ≈ 0.7615941559557649
    ///   Δ = 0.1 · (-0 + tanh(1)) ≈ 0.0762
    #[test]
    fn input_drives_state_by_expected_amount() {
        let mut state = vec![0.0];
        let weights = vec![0.0];
        let input = vec![1.0];
        step_in_place(&mut state, &weights, &input, 0.1, 1.0).unwrap();
        let expected = 0.1 * 1.0_f32.tanh();
        let diff = (state[0] - expected).abs();
        assert!(
            diff < 1e-6,
            "expected {expected}, got {} (diff {diff})",
            state[0]
        );
    }

    /// 1D bistable system: τ ẋ = -x + tanh(2x). Fixed points at 0 and ±x*
    /// where x* = tanh(2x*) ≈ 0.95775. Starting at x₀=0.5 (positive basin),
    /// integration should converge to +x*.
    #[test]
    fn bistable_converges_to_positive_attractor() {
        let mut state = vec![0.5_f32];
        let weights = vec![2.0_f32];
        let input = vec![0.0_f32];
        for _ in 0..2_000 {
            step_in_place(&mut state, &weights, &input, 0.01, 1.0).unwrap();
        }
        // x* satisfies x* = tanh(2x*). Solved numerically to ~0.957823.
        let expected = 0.957823_f32;
        let diff = (state[0] - expected).abs();
        assert!(
            diff < 1e-3,
            "should converge to +x* ≈ {expected}, got {} (diff {diff})",
            state[0]
        );
    }

    /// Same bistable system from x₀=-0.5 must land on the negative attractor.
    #[test]
    fn bistable_converges_to_negative_attractor_from_below() {
        let mut state = vec![-0.5_f32];
        let weights = vec![2.0_f32];
        let input = vec![0.0_f32];
        for _ in 0..2_000 {
            step_in_place(&mut state, &weights, &input, 0.01, 1.0).unwrap();
        }
        let expected = -0.957823_f32;
        let diff = (state[0] - expected).abs();
        assert!(
            diff < 1e-3,
            "should converge to -x*, got {} (diff {diff})",
            state[0]
        );
    }

    /// Shape mismatch is a Result error, not a panic.
    #[test]
    fn shape_errors_are_returned_not_panicked() {
        let mut state = vec![0.0_f32; 3];
        let weights = vec![0.0_f32; 9];
        let input = vec![0.0_f32; 2]; // wrong
        let err = step_in_place(&mut state, &weights, &input, 0.1, 1.0).unwrap_err();
        assert!(
            matches!(err, CanError::InputShape { expected: 3, got: 2 }),
            "wrong error variant: {err:?}"
        );
    }
}
