//! Modern Hopfield retrieval (Ramsauer et al. 2020).
//!
//! Stored patterns `X ∈ ℝ^{N×D}` (row-major, one pattern per row), query
//! `ξ ∈ ℝ^D`, inverse temperature `β`:
//!
//! ```text
//! retrieved = Xᵀ · softmax(β · X · ξ)
//! ```
//!
//! High `β` makes the softmax peaky — output collapses to the single
//! best-matching pattern. Low `β` averages across multiple patterns.
//!
//! Convergence: a single step suffices when patterns are well separated
//! (the "metastable" regime); iterate for ambiguous queries.

#[derive(Debug, thiserror::Error)]
pub enum HopfieldError {
    #[error("empty pattern bank")]
    EmptyBank,
    #[error("pattern dim {pattern_dim} doesn't match query dim {query_dim}")]
    DimMismatch {
        pattern_dim: usize,
        query_dim: usize,
    },
    #[error("patterns length {got} doesn't match n_patterns × dim = {expected}")]
    PatternsShape { expected: usize, got: usize },
    #[error("β must be > 0, got {0}")]
    InvalidBeta(f32),
}

/// One Hopfield retrieval step.
///
/// `patterns` is row-major `n_patterns × dim`. The function does not modify
/// `patterns`; it allocates a fresh result vector of length `dim`.
pub fn retrieve(
    patterns: &[f32],
    n_patterns: usize,
    dim: usize,
    query: &[f32],
    beta: f32,
) -> Result<Vec<f32>, HopfieldError> {
    if n_patterns == 0 {
        return Err(HopfieldError::EmptyBank);
    }
    if query.len() != dim {
        return Err(HopfieldError::DimMismatch {
            pattern_dim: dim,
            query_dim: query.len(),
        });
    }
    let expected = n_patterns * dim;
    if patterns.len() != expected {
        return Err(HopfieldError::PatternsShape {
            expected,
            got: patterns.len(),
        });
    }
    // Catches β = 0, negative, and NaN. `partial_cmp` over `f32` returns
    // None for NaN so any non-`Some(Greater)` is invalid.
    if !matches!(beta.partial_cmp(&0.0), Some(std::cmp::Ordering::Greater)) {
        return Err(HopfieldError::InvalidBeta(beta));
    }

    // sims[i] = β · ⟨pattern_i, query⟩
    let mut sims = Vec::with_capacity(n_patterns);
    for i in 0..n_patterns {
        let row = &patterns[i * dim..(i + 1) * dim];
        let mut dot = 0.0_f32;
        for j in 0..dim {
            dot += row[j] * query[j];
        }
        sims.push(beta * dot);
    }

    // softmax with max-subtraction for numerical stability
    let max_sim = sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut weights: Vec<f32> = sims.iter().map(|s| (s - max_sim).exp()).collect();
    let z: f32 = weights.iter().sum();
    for w in &mut weights {
        *w /= z;
    }

    // retrieved = Σ_i weights[i] · pattern_i
    let mut out = vec![0.0_f32; dim];
    for i in 0..n_patterns {
        let row = &patterns[i * dim..(i + 1) * dim];
        let w = weights[i];
        for j in 0..dim {
            out[j] += w * row[j];
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    /// Single pattern: retrieval is the identity.
    /// X = [P], query = P → softmax([β|P|²]) = [1.0] → out = P.
    #[test]
    fn single_pattern_returns_itself() {
        let pattern = vec![1.0_f32, 2.0, 3.0];
        let out = retrieve(&pattern, 1, 3, &pattern, 1.0).unwrap();
        for i in 0..3 {
            assert!(
                approx_eq(out[i], pattern[i], 1e-6),
                "out[{i}] = {} expected {}",
                out[i],
                pattern[i]
            );
        }
    }

    /// Two orthogonal patterns; query = P1; high β → output very close to P1.
    /// With β=10, weights = [exp(10)/(exp(10)+exp(0)), 1/(exp(10)+exp(0))]
    ///                    ≈ [0.99995, 0.00005]
    /// out ≈ 0.99995·P1 + 0.00005·P2 ≈ P1
    #[test]
    fn high_beta_collapses_to_nearest() {
        // P1 = [1, 0], P2 = [0, 1] — unit norm, orthogonal.
        let patterns = vec![1.0_f32, 0.0, 0.0, 1.0];
        let query = vec![1.0_f32, 0.0];
        let out = retrieve(&patterns, 2, 2, &query, 10.0).unwrap();
        // Closer to P1 than to P2:
        let d1 = (out[0] - 1.0).powi(2) + out[1].powi(2);
        let d2 = out[0].powi(2) + (out[1] - 1.0).powi(2);
        assert!(d1 < d2 * 1e-3, "should be much closer to P1");
    }

    /// Low β averages patterns: with β=0.001 and two orthogonal unit
    /// patterns, output is approximately the centroid (P1 + P2) / 2.
    #[test]
    fn low_beta_averages_across_bank() {
        let patterns = vec![1.0_f32, 0.0, 0.0, 1.0];
        let query = vec![1.0_f32, 0.0];
        let out = retrieve(&patterns, 2, 2, &query, 0.001).unwrap();
        // Centroid is [0.5, 0.5].
        assert!(approx_eq(out[0], 0.5, 1e-3), "out[0] = {}", out[0]);
        assert!(approx_eq(out[1], 0.5, 1e-3), "out[1] = {}", out[1]);
    }

    /// Noisy query of P1 still retrieves P1 at high β.
    #[test]
    fn noisy_query_retrieves_nearest_pattern() {
        // Three well-separated patterns in ℝ³ (one-hot like).
        let patterns = vec![
            1.0_f32, 0.0, 0.0, //
            0.0, 1.0, 0.0, //
            0.0, 0.0, 1.0,
        ];
        // Query is P1 with a sprinkle of noise.
        let query = vec![0.9_f32, 0.1, 0.05];
        let out = retrieve(&patterns, 3, 3, &query, 20.0).unwrap();
        // out[0] should be the largest component.
        assert!(out[0] > out[1] && out[0] > out[2]);
        assert!(out[0] > 0.99, "out[0] = {}, expected near 1.0", out[0]);
    }

    /// Empty bank is an error, not a panic.
    #[test]
    fn empty_bank_errors() {
        let err = retrieve(&[], 0, 3, &[0.0, 0.0, 0.0], 1.0).unwrap_err();
        assert!(matches!(err, HopfieldError::EmptyBank));
    }

    /// Mismatched dims are an error.
    #[test]
    fn dim_mismatch_errors() {
        let patterns = vec![1.0_f32, 0.0];
        let err = retrieve(&patterns, 1, 2, &[1.0, 0.0, 0.0], 1.0).unwrap_err();
        assert!(matches!(err, HopfieldError::DimMismatch { .. }));
    }
}
