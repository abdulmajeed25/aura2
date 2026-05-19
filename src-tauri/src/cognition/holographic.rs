//! FHRR holographic memory. Phase 12 (experimental).
//!
//! Fourier Holographic Reduced Representation (Plate, 2003): vectors live
//! in `ℂ^D` with every component constrained to **unit modulus**
//! (`|z_i| = 1`). The algebraic operations become element-wise:
//!
//! | Op                  | Formula                                          |
//! | ------------------- | ------------------------------------------------ |
//! | `bind(A, B)`        | `A_i · B_i` (element-wise complex multiply)      |
//! | `unbind(C, A)`      | `bind(C, conj(A))` — recover `B` from `bind(A, B)` |
//! | `bundle({A^(k)})`   | sum, then renormalise each `z_i ← z_i / |z_i|`   |
//! | `similarity(A, B)`  | `Re(⟨A, B*⟩) / D` ∈ `[-1, 1]`                    |
//!
//! No FFT is needed because we represent the vector directly in the
//! frequency domain (element-wise multiplication in frequency is circular
//! convolution in time). That keeps the kernel tiny and free of external
//! deps — we use a local `Cmplx` struct rather than pulling `num_complex`.
//!
//! Capacity is `O(D / log K)` for `K` simultaneously bundled items, same
//! big-O as bipolar HDC but with two real-valued slots per dimension
//! instead of one bit — practical retrieval thresholds reach further.
//!
//! As with `hamiltonian.rs`, this module ships kernels + math-driven tests
//! only. The decision to swap or augment the bipolar HDC engine with FHRR
//! lands in a later gate after the spec's required telemetry has run.

use rand::Rng;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Lightweight complex number. We only need add, multiply, conjugate,
/// dot-product, and modulus — no need for `num_complex`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cmplx {
    pub re: f32,
    pub im: f32,
}

impl Cmplx {
    pub fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    /// Unit-modulus complex number from phase angle `θ` (radians).
    pub fn from_phase(theta: f32) -> Self {
        Self {
            re: theta.cos(),
            im: theta.sin(),
        }
    }

    pub fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    pub fn modulus(self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }

    /// `(a + bi) · (c + di) = (ac − bd) + (ad + bc)i`.
    pub fn cmul(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }

    pub fn cadd(self, other: Self) -> Self {
        Self {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }
}

/// FHRR vector: `Vec<Cmplx>` where every component should have modulus 1.
/// The invariant is restored after `bundle` via `normalise_in_place`.
#[derive(Clone, Debug)]
pub struct FhrrVec(pub Vec<Cmplx>);

impl FhrrVec {
    pub fn dim(&self) -> usize {
        self.0.len()
    }

    /// Allocate a vector of length `dim` with uniformly-random phases in
    /// `[-π, π]`. Deterministic from `seed`.
    pub fn random(dim: usize, seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let pi = std::f32::consts::PI;
        let mut data = Vec::with_capacity(dim);
        for _ in 0..dim {
            let u: f32 = rng.gen(); // uniform in [0, 1)
            let theta = (u * 2.0 - 1.0) * pi;
            data.push(Cmplx::from_phase(theta));
        }
        Self(data)
    }

    /// Element-wise complex multiply.
    pub fn bind(&self, other: &Self) -> Self {
        assert_eq!(self.dim(), other.dim(), "bind: dimension mismatch");
        let v = self
            .0
            .iter()
            .zip(other.0.iter())
            .map(|(a, b)| a.cmul(*b))
            .collect();
        Self(v)
    }

    /// `unbind(C, A) = C ⊙ conj(A)` recovers `B` from `bind(A, B) = C`
    /// because complex multiplication of unit-modulus numbers is rotation.
    pub fn unbind(&self, key: &Self) -> Self {
        assert_eq!(self.dim(), key.dim(), "unbind: dimension mismatch");
        let v = self
            .0
            .iter()
            .zip(key.0.iter())
            .map(|(a, b)| a.cmul(b.conj()))
            .collect();
        Self(v)
    }

    /// Sum + per-component re-normalise.
    pub fn bundle(items: &[&Self]) -> Self {
        assert!(!items.is_empty(), "bundle: empty input");
        let dim = items[0].dim();
        let mut acc = vec![Cmplx::new(0.0, 0.0); dim];
        for item in items {
            assert_eq!(item.dim(), dim, "bundle: dimension mismatch");
            for (slot, src) in acc.iter_mut().zip(item.0.iter()) {
                *slot = slot.cadd(*src);
            }
        }
        for z in &mut acc {
            let m = z.modulus();
            if m > 1e-9 {
                z.re /= m;
                z.im /= m;
            } else {
                // Pathological case: all summed components cancelled to
                // ~zero at this coordinate. Default to (1, 0) so the vector
                // stays length D and the invariant holds.
                z.re = 1.0;
                z.im = 0.0;
            }
        }
        Self(acc)
    }

    /// `Re(⟨A, B*⟩) / D` ∈ `[-1, 1]` for unit-modulus FHRR vectors.
    pub fn similarity(&self, other: &Self) -> f32 {
        assert_eq!(self.dim(), other.dim(), "similarity: dimension mismatch");
        let dim = self.dim();
        if dim == 0 {
            return 0.0;
        }
        // Re(a · conj(b)) = a.re·b.re + a.im·b.im
        let acc: f32 = self
            .0
            .iter()
            .zip(other.0.iter())
            .map(|(a, b)| a.re * b.re + a.im * b.im)
            .sum();
        acc / dim as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    /// Every random component is on the unit circle.
    #[test]
    fn random_has_unit_modulus_per_component() {
        let v = FhrrVec::random(256, 42);
        for z in &v.0 {
            assert!(
                approx(z.modulus(), 1.0, 1e-5),
                "|z| = {} (not unit modulus)",
                z.modulus()
            );
        }
    }

    /// Same seed → same vector (determinism).
    #[test]
    fn same_seed_replays() {
        let a = FhrrVec::random(64, 7);
        let b = FhrrVec::random(64, 7);
        for i in 0..64 {
            assert_eq!(a.0[i].re.to_bits(), b.0[i].re.to_bits());
            assert_eq!(a.0[i].im.to_bits(), b.0[i].im.to_bits());
        }
    }

    /// `bind` preserves unit modulus.
    #[test]
    fn bind_preserves_unit_modulus() {
        let a = FhrrVec::random(128, 1);
        let b = FhrrVec::random(128, 2);
        let c = a.bind(&b);
        for z in &c.0 {
            assert!(approx(z.modulus(), 1.0, 1e-4), "|c| = {}", z.modulus());
        }
    }

    /// Self-similarity is 1.0.
    #[test]
    fn similarity_with_self_is_one() {
        let a = FhrrVec::random(256, 11);
        let s = a.similarity(&a);
        assert!(approx(s, 1.0, 1e-5), "sim(a, a) = {s}");
    }

    /// Two independent random vectors are ~orthogonal at chance:
    /// for D=4096, Var(Re(⟨A,B*⟩)/D) = 1/(2D), so std ≈ 1/√(2D) ≈ 0.011.
    /// Assert |sim| < 0.05 (≥ 4.5σ envelope).
    #[test]
    fn distinct_seeds_are_uncorrelated_at_chance() {
        let a = FhrrVec::random(4096, 1);
        let b = FhrrVec::random(4096, 2);
        let s = a.similarity(&b);
        assert!(s.abs() < 0.05, "|sim| = {} (too far from 0)", s.abs());
    }

    /// Unbind inverts bind: `unbind(bind(A, B), A) ≈ B`.
    /// In exact arithmetic the recovery is perfect; with f32 we expect
    /// `sim > 0.999`.
    #[test]
    fn unbind_recovers_filler_from_role_filler_product() {
        let role = FhrrVec::random(1024, 100);
        let filler = FhrrVec::random(1024, 200);
        let bound = role.bind(&filler);
        let recovered = bound.unbind(&role);
        let sim = recovered.similarity(&filler);
        assert!(sim > 0.999, "unbind recovery sim = {sim}");
    }

    /// Bundle preserves similarity to each component above chance.
    /// For K=3 bundled patterns of dim 1024, expected `sim(bundle, comp_i)`
    /// ≈ `1/√K` ≈ 0.577 if components are unit-modulus orthogonal.
    /// We require `> 0.45` to leave room for noise.
    #[test]
    fn bundle_preserves_similarity_to_components() {
        let a = FhrrVec::random(1024, 10);
        let b = FhrrVec::random(1024, 20);
        let c = FhrrVec::random(1024, 30);
        let bundled = FhrrVec::bundle(&[&a, &b, &c]);
        assert!(bundled.similarity(&a) > 0.45, "sim(bun, a) = {}", bundled.similarity(&a));
        assert!(bundled.similarity(&b) > 0.45);
        assert!(bundled.similarity(&c) > 0.45);
    }

    /// Compositional retrieval: build `M = bind(K1, V1) + bind(K2, V2)`
    /// and recover `V1` by `unbind(M, K1)`. The recovered vector should be
    /// closer to V1 than to V2 (above chance), even though V2 is in M too.
    #[test]
    fn role_filler_retrieval_works_under_bundling() {
        let k1 = FhrrVec::random(2048, 1);
        let v1 = FhrrVec::random(2048, 2);
        let k2 = FhrrVec::random(2048, 3);
        let v2 = FhrrVec::random(2048, 4);
        let memory = FhrrVec::bundle(&[&k1.bind(&v1), &k2.bind(&v2)]);
        let recovered = memory.unbind(&k1);
        let s1 = recovered.similarity(&v1);
        let s2 = recovered.similarity(&v2);
        assert!(
            s1 > s2 + 0.3,
            "recovery should prefer V1 over V2 (got s1={s1}, s2={s2})"
        );
    }
}
