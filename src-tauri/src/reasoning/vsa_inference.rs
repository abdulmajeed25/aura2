//! VSA inference over bipolar-HDC key-value memories.
//!
//! Standard HRR pattern:
//!
//! ```text
//! M = bundle( bind(K₁, V₁), bind(K₂, V₂), …, bind(Kₙ, Vₙ) )
//! ```
//!
//! Queries built on bind being self-inverse (`bind(K, K) = +1` for bipolar):
//!
//! - `recover_value(M, K) = bind(M, K)` then nearest-neighbour against a
//!   pool of known fillers.
//! - `recover_key(M, V) = bind(M, V)` then nearest-neighbour against a
//!   pool of known roles.
//! - `is_pair_present(M, K, V) = similarity(M, bind(K, V))` ∈ `[-1, 1]`.
//!
//! Capacity bound (Plate): for `D = 10_000`, ~`D / (4·log K)` independent
//! pairs before retrieval breaks down. At `K = 5` that's well over 1000
//! pairs in theory; in practice the bipolar f32 noise floor cuts it
//! sooner. We benchmark against `K = 6` here and require sim > 0.10 vs
//! a non-stored filler.

use crate::core::hdc::hypervector::Hypervector;

/// A key-value memory: the bundled hyperveector `memory` plus the lists of
/// canonical roles and fillers used to "clean up" noisy retrievals via
/// nearest-neighbour against the stored vocabulary.
pub struct VsaKnowledge {
    pub memory: Hypervector,
    pub roles: Vec<(String, Hypervector)>,
    pub fillers: Vec<(String, Hypervector)>,
}

#[derive(Clone, Debug)]
pub struct Retrieval {
    /// Best-matching label from the stored vocabulary.
    pub label: String,
    /// Cosine similarity in `[-1, 1]` between the (noisy) retrieved
    /// hypervector and the labelled stored vector.
    pub similarity: f32,
}

impl VsaKnowledge {
    /// Build a knowledge base from `(role_name, filler_name)` string pairs.
    /// Roles and fillers get deterministic hypervectors via
    /// `Hypervector::from_token`. Pairs are bound and bundled into `memory`.
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        let mut roles: Vec<(String, Hypervector)> = Vec::new();
        let mut fillers: Vec<(String, Hypervector)> = Vec::new();
        let mut bound: Vec<Hypervector> = Vec::with_capacity(pairs.len());

        for (k, v) in pairs {
            let kh = role_or_filler(&mut roles, k);
            let vh = role_or_filler(&mut fillers, v);
            bound.push(kh.bind(&vh));
        }
        let refs: Vec<&Hypervector> = bound.iter().collect();
        let memory = if refs.is_empty() {
            Hypervector::one()
        } else {
            Hypervector::bundle(&refs)
        };
        Self {
            memory,
            roles,
            fillers,
        }
    }

    /// `recover_value(K) = bind(M, K)`; report the closest filler in the
    /// stored vocabulary.
    pub fn recover_value(&self, role: &str) -> Option<Retrieval> {
        let kh = self.roles.iter().find(|(n, _)| n == role).map(|(_, h)| h)?;
        let probe = self.memory.bind(kh);
        Some(nearest(&probe, &self.fillers))
    }

    /// `recover_key(V) = bind(M, V)`; report the closest role.
    pub fn recover_key(&self, filler: &str) -> Option<Retrieval> {
        let vh = self
            .fillers
            .iter()
            .find(|(n, _)| n == filler)
            .map(|(_, h)| h)?;
        let probe = self.memory.bind(vh);
        Some(nearest(&probe, &self.roles))
    }

    /// `is_pair_present(K, V) = similarity(M, bind(K, V))`. Returns the
    /// similarity score directly so callers can threshold per task.
    pub fn pair_score(&self, role: &str, filler: &str) -> Option<f32> {
        let kh = self.roles.iter().find(|(n, _)| n == role).map(|(_, h)| h)?;
        let vh = self
            .fillers
            .iter()
            .find(|(n, _)| n == filler)
            .map(|(_, h)| h)?;
        Some(self.memory.similarity(&kh.bind(vh)))
    }
}

fn role_or_filler(table: &mut Vec<(String, Hypervector)>, name: &str) -> Hypervector {
    if let Some((_, h)) = table.iter().find(|(n, _)| n == name) {
        return h.clone();
    }
    let h = Hypervector::from_token(name);
    table.push((name.to_string(), h.clone()));
    h
}

fn nearest(probe: &Hypervector, pool: &[(String, Hypervector)]) -> Retrieval {
    let mut best_label = String::new();
    let mut best_sim = f32::NEG_INFINITY;
    for (label, h) in pool {
        let s = probe.similarity(h);
        if s > best_sim {
            best_sim = s;
            best_label = label.clone();
        }
    }
    Retrieval {
        label: best_label,
        similarity: best_sim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3 key-value pairs. Each `recover_value(K_i)` should retrieve V_i.
    #[test]
    fn three_pair_kb_recovers_each_value() {
        let kb = VsaKnowledge::from_pairs(&[
            ("name", "abdulmajeed"),
            ("country", "saudi_arabia"),
            ("project", "aura"),
        ]);
        let r1 = kb.recover_value("name").unwrap();
        let r2 = kb.recover_value("country").unwrap();
        let r3 = kb.recover_value("project").unwrap();
        assert_eq!(r1.label, "abdulmajeed");
        assert_eq!(r2.label, "saudi_arabia");
        assert_eq!(r3.label, "aura");
        // Each best match well above noise floor (with K=3, sim > 0.3
        // is achievable for HV_DIM = 10_000).
        assert!(r1.similarity > 0.3, "name sim = {}", r1.similarity);
        assert!(r2.similarity > 0.3, "country sim = {}", r2.similarity);
        assert!(r3.similarity > 0.3, "project sim = {}", r3.similarity);
    }

    /// Reverse lookup: given a filler, recover its role.
    #[test]
    fn recover_key_from_filler() {
        let kb = VsaKnowledge::from_pairs(&[("name", "abdulmajeed"), ("project", "aura")]);
        let r = kb.recover_key("aura").unwrap();
        assert_eq!(r.label, "project");
        assert!(r.similarity > 0.3, "sim = {}", r.similarity);
    }

    /// `pair_score` is high for stored pairs, low for non-stored.
    #[test]
    fn pair_score_distinguishes_stored_from_unstored() {
        let kb = VsaKnowledge::from_pairs(&[("name", "abdulmajeed"), ("project", "aura")]);
        let stored = kb.pair_score("name", "abdulmajeed").unwrap();
        // Cross-pair binding NOT in the bundle:
        let unstored = kb.pair_score("name", "aura").unwrap();
        assert!(
            stored > unstored + 0.2,
            "stored ({stored}) should be clearly higher than unstored ({unstored})"
        );
    }

    /// Six pairs — exercises bundling capacity. At HV_DIM=10_000, K=6,
    /// each value retrieval should still produce a clear winner.
    #[test]
    fn six_pair_kb_still_distinguishes_each_value() {
        let kb = VsaKnowledge::from_pairs(&[
            ("color", "blue"),
            ("shape", "circle"),
            ("size", "large"),
            ("year", "2026"),
            ("city", "riyadh"),
            ("kind", "knowledge_engine"),
        ]);
        for (role, expected) in [
            ("color", "blue"),
            ("shape", "circle"),
            ("size", "large"),
            ("year", "2026"),
            ("city", "riyadh"),
            ("kind", "knowledge_engine"),
        ] {
            let r = kb.recover_value(role).unwrap();
            assert_eq!(r.label, expected, "wrong recovery for role {role}");
            assert!(
                r.similarity > 0.10,
                "weak signal for {role}: sim = {}",
                r.similarity
            );
        }
    }

    /// Unknown role → None, not panic.
    #[test]
    fn unknown_role_returns_none() {
        let kb = VsaKnowledge::from_pairs(&[("a", "1")]);
        assert!(kb.recover_value("nonexistent").is_none());
        assert!(kb.recover_key("nonexistent").is_none());
        assert!(kb.pair_score("nonexistent", "1").is_none());
    }
}
