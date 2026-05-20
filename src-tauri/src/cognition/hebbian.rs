//! Hebbian synaptic update.
//!
//! ```text
//! Δw_ij = η · x_i · y_j
//! ```
//!
//! Co-active pre- and post-synaptic units strengthen the connection between
//! them. Non-co-active pairs are left alone.
//!
//! For now we expose [`reinforce_in_place`] for a dense outer-product update.
//! The sparse / STDP-timed variant is a later phase.

use crate::cognition::cans::CanError;

/// Apply one Hebbian update to a dense weight matrix `weights[i, j]`.
/// Row-major, shape `(presyn.len(), postsyn.len())`.
pub fn reinforce_in_place(
    weights: &mut [f32],
    presyn: &[f32],
    postsyn: &[f32],
    eta: f32,
) -> Result<(), CanError> {
    let n_pre = presyn.len();
    let n_post = postsyn.len();
    if weights.len() != n_pre * n_post {
        return Err(CanError::WeightsShape {
            expected: n_pre * n_post,
            got: weights.len(),
        });
    }
    if eta <= 0.0 {
        return Err(CanError::InvalidTimestep { dt: eta, tau: 1.0 });
    }
    for i in 0..n_pre {
        let xi = presyn[i];
        if xi == 0.0 {
            continue; // no contribution from this presynaptic unit
        }
        let row = &mut weights[i * n_post..(i + 1) * n_post];
        for j in 0..n_post {
            row[j] += eta * xi * postsyn[j];
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-computed outer product:
    ///   x = [1, 0.5], y = [0.5, 1], η = 0.1
    ///   Δw = [[0.1·1·0.5, 0.1·1·1],
    ///         [0.1·0.5·0.5, 0.1·0.5·1]]
    ///       = [[0.05, 0.10],
    ///          [0.025, 0.05]]
    #[test]
    fn outer_product_matches_hand_computation() {
        let mut w = vec![0.0_f32; 4];
        let x = [1.0_f32, 0.5];
        let y = [0.5_f32, 1.0];
        reinforce_in_place(&mut w, &x, &y, 0.1).unwrap();
        let expected = [0.05_f32, 0.10, 0.025, 0.05];
        for i in 0..4 {
            let diff = (w[i] - expected[i]).abs();
            assert!(
                diff < 1e-6,
                "w[{i}] = {} expected {}",
                w[i],
                expected[i]
            );
        }
    }

    /// Co-active indices grow; non-co-active stay at zero.
    #[test]
    fn co_active_strengthens_non_co_active_does_not() {
        let n = 8;
        let mut w = vec![0.0_f32; n * n];
        // Two units (0 and 3) always fire together; the rest are silent.
        for _ in 0..50 {
            let mut x = vec![0.0_f32; n];
            x[0] = 1.0;
            x[3] = 1.0;
            reinforce_in_place(&mut w, &x, &x, 0.01).unwrap();
        }
        // w[0,3] and w[3,0] should be positive; w[1,2] should be exactly 0.
        assert!(w[3] > 0.0, "w[0,3] never grew");
        assert!(w[3 * n] > 0.0, "w[3,0] never grew");
        assert_eq!(w[n + 2], 0.0, "w[1,2] grew despite silence");
    }

    /// Zero presynaptic unit makes no contribution (sparse shortcut).
    #[test]
    fn zero_presynaptic_row_is_skipped() {
        let mut w = vec![1.0_f32; 4]; // 2×2, all 1's
        let x = [0.0_f32, 1.0];
        let y = [1.0_f32, 1.0];
        reinforce_in_place(&mut w, &x, &y, 0.5).unwrap();
        // Row 0 (presyn 0 = 0) must be untouched.
        assert_eq!(w[0], 1.0);
        assert_eq!(w[1], 1.0);
        // Row 1 (presyn 1 = 1): each += 0.5·1·1 = 0.5
        assert_eq!(w[2], 1.5);
        assert_eq!(w[3], 1.5);
    }

    /// Shape mismatch returns Err.
    #[test]
    fn shape_mismatch_returns_err() {
        let mut w = vec![0.0_f32; 4];
        let err = reinforce_in_place(&mut w, &[1.0], &[1.0, 1.0, 1.0], 0.1).unwrap_err();
        assert!(matches!(err, CanError::WeightsShape { .. }));
    }
}
