//! Phase 5 text embedder.
//!
//! Real production deployment will swap [`HashEmbedder`] for an ONNX-backed
//! `all-MiniLM-L6-v2` encoder; the [`TextEncoder`] trait is the seam where
//! that swap happens. Until then, we use a 384-dim feature-hashed embedding
//! over unigrams + bigrams. It's deterministic, requires no model file, and
//! gives genuine vector similarity for documents that share vocabulary or
//! short phrases.

use std::collections::HashSet;

/// 384 matches `all-MiniLM-L6-v2`'s output dimension so swapping later
/// requires no schema change.
pub const EMBED_DIM: usize = 384;

/// Common trait so we can swap in a real ONNX encoder later without touching
/// callers in [`crate::core::vault`] or the search module.
pub trait TextEncoder: Send + Sync {
    fn dim(&self) -> usize;
    fn encode(&self, text: &str) -> Vec<f32>;
}

/// Hash-embedder: each (unigram, bigram) token is hashed into a (sign, index)
/// pair via FNV-1a; signs are summed, then the vector is L2-normalised.
/// Two co-occurring tokens push the vector toward the same coordinates →
/// documents sharing concept vocabulary land close to each other in the 384-d
/// space.
#[derive(Debug, Default, Clone, Copy)]
pub struct HashEmbedder;

impl HashEmbedder {
    pub const fn new() -> Self {
        Self
    }
}

impl TextEncoder for HashEmbedder {
    fn dim(&self) -> usize {
        EMBED_DIM
    }

    fn encode(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; EMBED_DIM];
        let tokens = tokenize(text);
        if tokens.is_empty() {
            return v;
        }

        for token in &tokens {
            apply_hash(&mut v, token);
        }
        for win in tokens.windows(2) {
            let bigram = format!("{} {}", win[0], win[1]);
            apply_hash(&mut v, &bigram);
        }

        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-9 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    }
}

/// Cosine similarity between two unit-norm vectors. Falls back to a manual
/// dot product so we never assume the caller normalised them.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-9);
    dot / denom
}

/// Pack a Vec<f32> into a little-endian byte buffer for SQLite storage.
pub fn embedding_to_bytes(emb: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(emb.len() * 4);
    for v in emb {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Inverse of [`embedding_to_bytes`].
pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn tokenize(text: &str) -> Vec<String> {
    let stopwords: HashSet<&str> = STOPWORDS.iter().copied().collect();
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2 && !stopwords.contains(*t))
        .map(String::from)
        .collect()
}

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "are", "was", "were", "but", "not", "you",
    "your", "have", "has", "had", "will", "would", "could", "should", "into", "onto", "over",
    "than", "then", "them", "they", "their", "there", "what", "when", "which", "while", "where",
    "who", "whom", "whose", "why", "how",
];

fn apply_hash(vec: &mut [f32], token: &str) {
    let h = fnv1a_64(token);
    let idx = (h as usize) % EMBED_DIM;
    // Use a high-order bit for the sign so it decorrelates from the index.
    let sign = if (h >> 56) & 1 == 0 { 1.0f32 } else { -1.0 };
    vec[idx] += sign;
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
    fn embedding_has_expected_dimension_and_is_unit_norm() {
        let enc = HashEmbedder::new();
        let v = enc.encode("the quick brown fox jumps over the lazy dog");
        assert_eq!(v.len(), EMBED_DIM);
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "norm={}", norm);
    }

    #[test]
    fn empty_text_returns_zero_vector() {
        let enc = HashEmbedder::new();
        let v = enc.encode("");
        assert_eq!(v.len(), EMBED_DIM);
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn related_texts_are_more_similar_than_unrelated() {
        let enc = HashEmbedder::new();
        let v_topic_a1 = enc.encode("productivity habits morning routines focus time blocking");
        let v_topic_a2 = enc.encode("morning routine for productivity and focused work");
        let v_topic_b = enc.encode("rust async runtime tokio futures executor");

        let sim_same = cosine_similarity(&v_topic_a1, &v_topic_a2);
        let sim_cross = cosine_similarity(&v_topic_a1, &v_topic_b);
        assert!(
            sim_same > sim_cross,
            "same-topic similarity ({}) must exceed cross-topic ({})",
            sim_same,
            sim_cross
        );
    }

    #[test]
    fn round_trip_to_bytes() {
        let enc = HashEmbedder::new();
        let v = enc.encode("aura knowledge engine");
        let bytes = embedding_to_bytes(&v);
        let v2 = bytes_to_embedding(&bytes);
        assert_eq!(v, v2);
    }
}
