//! Liquid State Machine — reservoir update.
//!
//! ```text
//! r(t+dt) = (1 - α) · r(t) + α · tanh(W_res · r(t) + W_in · u(t))
//! ```
//!
//! `α ∈ (0, 1]` is the leak rate (high α = fast forgetting). For Phase 11
//! we expose [`step_in_place`] taking the reservoir, the recurrent weight
//! matrix `W_res`, the input projection `W_in`, the input vector, and α.

use crate::cognition::cans::CanError;

/// One step of the reservoir, in place on `reservoir`.
///
/// `w_res` is row-major `n × n`. `w_in` is row-major `n × m` where `m =
/// input.len()`. Either may be empty/absent in degenerate cases (n=0 or m=0).
pub fn step_in_place(
    reservoir: &mut [f32],
    w_res: &[f32],
    w_in: &[f32],
    input: &[f32],
    alpha: f32,
) -> Result<(), CanError> {
    let n = reservoir.len();
    if w_res.len() != n * n {
        return Err(CanError::WeightsShape {
            expected: n * n,
            got: w_res.len(),
        });
    }
    let m = input.len();
    if w_in.len() != n * m {
        return Err(CanError::WeightsShape {
            expected: n * m,
            got: w_in.len(),
        });
    }
    if !(0.0..=1.0).contains(&alpha) {
        return Err(CanError::InvalidTimestep {
            dt: alpha,
            tau: 1.0,
        });
    }

    let mut pre = vec![0.0_f32; n];
    for i in 0..n {
        let recurrent_row = &w_res[i * n..(i + 1) * n];
        let mut acc = 0.0_f32;
        for j in 0..n {
            acc += recurrent_row[j] * reservoir[j];
        }
        if m > 0 {
            let input_row = &w_in[i * m..(i + 1) * m];
            for j in 0..m {
                acc += input_row[j] * input[j];
            }
        }
        pre[i] = acc.tanh();
    }

    for i in 0..n {
        reservoir[i] = (1.0 - alpha) * reservoir[i] + alpha * pre[i];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// α=1, zero state, zero input, any W → r = tanh(0) = 0 stays at 0.
    #[test]
    fn zero_state_zero_input_stays_zero() {
        let mut r = vec![0.0_f32; 4];
        let w_res = vec![0.5_f32; 16];
        let w_in = vec![0.0_f32; 0];
        step_in_place(&mut r, &w_res, &w_in, &[], 1.0).unwrap();
        for &v in &r {
            assert_eq!(v, 0.0);
        }
    }

    /// α=1, zero W_res, identity W_in, input=[1, 0, 0]:
    ///   r_new[i] = tanh(W_in[i, :] · u)
    ///   = tanh([1, 0, 0]·u, [0, 1, 0]·u, [0, 0, 1]·u)
    ///   = [tanh(1), 0, 0]
    #[test]
    fn identity_input_projection_passes_through_tanh() {
        let mut r = vec![0.0_f32; 3];
        let w_res = vec![0.0_f32; 9];
        // 3×3 identity row-major
        let w_in = vec![
            1.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, //
            0.0, 0.0, 1.0,
        ];
        let u = vec![1.0_f32, 0.0, 0.0];
        step_in_place(&mut r, &w_res, &w_in, &u, 1.0).unwrap();
        let expected = [1.0_f32.tanh(), 0.0, 0.0];
        for i in 0..3 {
            let diff = (r[i] - expected[i]).abs();
            assert!(diff < 1e-6, "r[{i}] = {}, expected {}", r[i], expected[i]);
        }
    }

    /// α=0.5, r₀=[0.5, 0, 0], zero W_res, no input:
    ///   pre = tanh(0) = 0
    ///   r_new = 0.5·[0.5, 0, 0] + 0.5·0 = [0.25, 0, 0]
    #[test]
    fn leaky_decay_halves_state_when_alpha_half() {
        let mut r = vec![0.5_f32, 0.0, 0.0];
        let w_res = vec![0.0_f32; 9];
        let w_in: Vec<f32> = vec![];
        step_in_place(&mut r, &w_res, &w_in, &[], 0.5).unwrap();
        let expected = [0.25_f32, 0.0, 0.0];
        for i in 0..3 {
            let diff = (r[i] - expected[i]).abs();
            assert!(diff < 1e-6, "r[{i}] = {}, expected {}", r[i], expected[i]);
        }
    }

    /// Zero input + non-zero reservoir + zero W_res converges to 0.
    /// With α=0.2, r₀=1.0, the trajectory is `r_k = 0.8^k · r₀`.
    /// After 10 steps: 0.8^10 ≈ 0.1074.
    #[test]
    fn pure_leak_decays_geometrically() {
        let mut r = vec![1.0_f32];
        let w_res = vec![0.0_f32];
        let w_in: Vec<f32> = vec![];
        for _ in 0..10 {
            step_in_place(&mut r, &w_res, &w_in, &[], 0.2).unwrap();
        }
        let expected = 0.8_f32.powi(10);
        let diff = (r[0] - expected).abs();
        assert!(diff < 1e-4, "expected {expected}, got {} (diff {diff})", r[0]);
    }

    /// Bad alpha returns an error rather than panicking.
    #[test]
    fn bad_alpha_returns_error() {
        let mut r = vec![0.0_f32; 2];
        let w_res = vec![0.0_f32; 4];
        let w_in: Vec<f32> = vec![];
        let err = step_in_place(&mut r, &w_res, &w_in, &[], 1.5).unwrap_err();
        assert!(matches!(err, CanError::InvalidTimestep { .. }));
    }
}
