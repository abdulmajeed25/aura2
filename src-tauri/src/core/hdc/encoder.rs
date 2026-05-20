//! Text → Hypervector. Tokenises the source the same way the Phase 5
//! [`crate::core::embeddings::HashEmbedder`] does, then bundles unigram +
//! bigram HVs into a single document HV. Two documents that share
//! vocabulary land close to each other in HV space.

use std::collections::HashSet;

use crate::core::hdc::Hypervector;

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "are", "was", "were", "but", "not", "you",
    "your", "have", "has", "had", "will", "would", "could", "should", "into", "onto", "over",
    "than", "then", "them", "they", "their", "there", "what", "when", "which", "while", "where",
    "who", "whom", "whose", "why", "how",
];

fn tokenize(text: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2 && !stop.contains(*t))
        .map(String::from)
        .collect()
}

/// Build a single HV that represents a piece of text.
pub fn encode_text(text: &str) -> Hypervector {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return Hypervector::one();
    }

    let unigram_hvs: Vec<Hypervector> = tokens.iter().map(|t| Hypervector::from_token(t)).collect();
    let bigram_hvs: Vec<Hypervector> = tokens
        .windows(2)
        .map(|w| {
            let l = Hypervector::from_token(&w[0]);
            let r = Hypervector::from_token(&w[1]);
            // bind with a permute so "a b" ≠ "b a"
            l.bind(&r.permute(1))
        })
        .collect();

    let mut refs: Vec<&Hypervector> = Vec::with_capacity(unigram_hvs.len() + bigram_hvs.len());
    for hv in &unigram_hvs {
        refs.push(hv);
    }
    for hv in &bigram_hvs {
        refs.push(hv);
    }
    Hypervector::bundle(&refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn related_texts_are_closer_than_unrelated() {
        let a = encode_text("morning routine productivity focus work");
        let b = encode_text("productivity focus work morning habits");
        let c = encode_text("rust async runtime tokio futures executor");

        let sim_same = a.similarity(&b);
        let sim_cross = a.similarity(&c);
        assert!(
            sim_same > sim_cross + 0.05,
            "expected same-topic > cross-topic + 0.05, got {} vs {}",
            sim_same,
            sim_cross
        );
    }

    #[test]
    fn empty_text_returns_identity_one() {
        let hv = encode_text("");
        assert!(hv.0.iter().all(|&v| v == 1));
    }
}
