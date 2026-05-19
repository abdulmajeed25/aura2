//! Bipolar hypervectors of dimension [`HV_DIM`].
//!
//! Each element is `-1` or `+1`. Operations:
//! - `bind(a, b)` is the element-wise product (binding a key/role to a value).
//! - `bundle([a..])` is the sign of the element-wise sum (set-like
//!   superposition).
//! - `permute(k)` is a cyclic right rotation by `k` (positional / sequential).
//! - `similarity(a, b)` is cosine = `(a · b) / DIM`, which for bipolar HVs is
//!   the fraction of agreeing coordinates rescaled to `[-1, 1]`.

use rand::Rng;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;

pub const HV_DIM: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hypervector(pub Vec<i8>);

impl Hypervector {
    /// Allocate a balanced bipolar vector from a deterministic seed.
    /// Sampling uses a raw uniform `u32` and inspects one bit, which is
    /// fallible-free (no `Bernoulli::new` `Result` to unwrap) — fits Hard
    /// Rule #4 (no panics in production paths).
    pub fn random(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let data: Vec<i8> = (0..HV_DIM)
            .map(|_| if rng.gen::<u32>() & 1 == 0 { 1i8 } else { -1i8 })
            .collect();
        Self(data)
    }

    /// HV derived deterministically from any string token.
    pub fn from_token(token: &str) -> Self {
        let seed = fnv1a_64(token);
        Self::random(seed)
    }

    /// `+1` everywhere.
    pub fn one() -> Self {
        Self(vec![1i8; HV_DIM])
    }

    pub fn bind(&self, other: &Self) -> Self {
        debug_assert_eq!(self.0.len(), other.0.len());
        let data: Vec<i8> = self
            .0
            .iter()
            .zip(other.0.iter())
            .map(|(&a, &b)| a * b)
            .collect();
        Self(data)
    }

    /// Sum a slice of HVs and project back to bipolar via `sign`.
    /// Ties (sum == 0) resolve to `+1` deterministically.
    pub fn bundle(vectors: &[&Hypervector]) -> Self {
        if vectors.is_empty() {
            return Self::one();
        }
        let mut sum = vec![0i32; HV_DIM];
        for v in vectors {
            for (i, &x) in v.0.iter().enumerate() {
                sum[i] += x as i32;
            }
        }
        let data: Vec<i8> = sum
            .into_iter()
            .map(|s| if s >= 0 { 1i8 } else { -1i8 })
            .collect();
        Self(data)
    }

    /// Cyclic right-rotation by `k`. `permute(1) ≠ permute(0)` so this is
    /// the standard way to encode positional / sequential information.
    pub fn permute(&self, k: usize) -> Self {
        let n = self.0.len();
        if n == 0 {
            return self.clone();
        }
        let k = k % n;
        if k == 0 {
            return self.clone();
        }
        let mut out = vec![0i8; n];
        for i in 0..n {
            out[(i + k) % n] = self.0[i];
        }
        Self(out)
    }

    /// Cosine similarity in `[-1, 1]`. Cheaper than the dot/norm formula
    /// because |a|=|b|=sqrt(DIM) is constant for bipolar HVs.
    pub fn similarity(&self, other: &Self) -> f32 {
        debug_assert_eq!(self.0.len(), other.0.len());
        let dot: i64 = self
            .0
            .iter()
            .zip(other.0.iter())
            .map(|(&a, &b)| (a as i64) * (b as i64))
            .sum();
        dot as f32 / self.0.len() as f32
    }

    /// Pack into one bit per dimension (`+1` → 1, `-1` → 0). 10k dim → 1250 bytes.
    pub fn to_packed_bytes(&self) -> Vec<u8> {
        let mut packed = vec![0u8; self.0.len().div_ceil(8)];
        for (i, &v) in self.0.iter().enumerate() {
            if v > 0 {
                packed[i / 8] |= 1 << (i % 8);
            }
        }
        packed
    }

    /// Inverse of [`Self::to_packed_bytes`].
    pub fn from_packed_bytes(bytes: &[u8], dim: usize) -> Self {
        let data: Vec<i8> = (0..dim)
            .map(|i| {
                let bit = (bytes[i / 8] >> (i % 8)) & 1;
                if bit == 1 {
                    1i8
                } else {
                    -1i8
                }
            })
            .collect();
        Self(data)
    }
}

fn fnv1a_64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_is_balanced() {
        let a = Hypervector::random(42);
        let pos = a.0.iter().filter(|&&v| v > 0).count();
        let ratio = pos as f32 / HV_DIM as f32;
        assert!(
            (ratio - 0.5).abs() < 0.05,
            "expected ~50/50 sign distribution, got {}",
            ratio
        );
    }

    #[test]
    fn from_token_is_deterministic_and_distinct() {
        let a = Hypervector::from_token("welcome");
        let b = Hypervector::from_token("welcome");
        let c = Hypervector::from_token("daily");
        assert_eq!(a.0, b.0);
        assert!(a.similarity(&c).abs() < 0.05, "unrelated tokens should be near-orthogonal");
    }

    #[test]
    fn bind_inverts_with_itself() {
        // bind is self-inverse for bipolar: a * a = +1 everywhere
        let a = Hypervector::from_token("x");
        let aa = a.bind(&a);
        assert!(aa.0.iter().all(|&v| v == 1));
    }

    #[test]
    fn bundle_preserves_components_above_chance() {
        let a = Hypervector::from_token("a");
        let b = Hypervector::from_token("b");
        let c = Hypervector::from_token("c");
        let bundled = Hypervector::bundle(&[&a, &b, &c]);
        let sim_a = bundled.similarity(&a);
        let sim_unrelated = bundled.similarity(&Hypervector::from_token("zzz"));
        assert!(
            sim_a > sim_unrelated + 0.1,
            "bundled HV should retain similarity to its components"
        );
    }

    #[test]
    fn permute_decorrelates_the_vector() {
        let a = Hypervector::from_token("seq");
        let p = a.permute(1);
        let sim = a.similarity(&p);
        assert!(sim.abs() < 0.05, "permute(1) should be ~orthogonal, got {}", sim);
    }

    #[test]
    fn packed_roundtrip() {
        let a = Hypervector::random(7);
        let packed = a.to_packed_bytes();
        assert_eq!(packed.len(), HV_DIM / 8);
        let b = Hypervector::from_packed_bytes(&packed, HV_DIM);
        assert_eq!(a, b);
    }
}
