//! Fact extraction from raw conversation text.
//!
//! Two implementations:
//!
//! - [`RegexExtractor`] — deterministic, offline. Picks first-person
//!   declarative sentences and a handful of explicit-preference
//!   templates. Used by every test in this module.
//! - The Anthropic LLM extractor lives in
//!   `crate::ai::providers::anthropic`; it implements [`FactExtractor`]
//!   by sending the message through a cached system prompt. It's not
//!   wired by default because doing so without a key in
//!   `<vault>/.aura/secrets/anthropic.key` would silently fail; the
//!   caller picks the impl at construction time.

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("provider: {0}")]
    Provider(String),
}

/// Trait-erased fact extractor. Returns zero or more *candidate* fact
/// statements; the [`Mem0Engine`](crate::memory::mem0::Mem0Engine)
/// then decides whether each one is `ADD` / `UPDATE` / `DELETE` /
/// `NOOP` against the existing store.
#[async_trait]
pub trait FactExtractor: Send + Sync {
    /// Extract fact statements from a single conversation message.
    /// Implementations should return statements in canonical form:
    /// single sentence, no leading whitespace, no trailing punctuation
    /// duplication. Casing is preserved so the UI can display them as
    /// the user phrased the original.
    async fn extract(&self, message: &str) -> Result<Vec<String>, ExtractError>;
}

/// Pattern-based extractor. Matches first-person declarative templates
/// like "I am X", "I like X", "My X is Y", "I don't X", "I no longer
/// X". Returns the matched fragment trimmed of leading "I ".
///
/// Limitations (intentional — anything more sophisticated belongs in
/// the LLM impl):
/// - Only first-person.
/// - One fact per regex match per sentence.
/// - Negation is preserved literally in the candidate text; the
///   decision policy is responsible for noticing it.
#[derive(Debug, Default, Clone, Copy)]
pub struct RegexExtractor;

impl RegexExtractor {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl FactExtractor for RegexExtractor {
    async fn extract(&self, message: &str) -> Result<Vec<String>, ExtractError> {
        let mut out = Vec::new();
        // Split on sentence terminators. Cheap and good enough for
        // English/Arabic conversation; the LLM extractor handles edge
        // cases (multi-clause sentences, embedded quotes, …).
        for raw in message.split(['.', '!', '?', '\n']) {
            let s = raw.trim();
            if s.is_empty() {
                continue;
            }
            if let Some(fact) = match_first_person(s) {
                out.push(fact);
            }
        }
        Ok(out)
    }
}

/// Returns the canonical fact text if the sentence matches one of the
/// known templates; otherwise `None`. We do NOT lower-case — the
/// caller wants the user's wording preserved for display.
fn match_first_person(sentence: &str) -> Option<String> {
    // Normalise leading whitespace + the leading "I" / "I'm" / "My".
    let s = sentence.trim();
    // Pattern A: "I <verb> ..." where verb is a known declarative
    // (am / like / love / hate / prefer / live / work / speak /
    // don't / never / no longer / used to).
    let lower = s.to_ascii_lowercase();
    const FIRST_PERSON_HEADS: &[&str] = &[
        "i am ",
        "i'm ",
        "i like ",
        "i love ",
        "i hate ",
        "i prefer ",
        "i live ",
        "i work ",
        "i speak ",
        "i don't ",
        "i do not ",
        "i never ",
        "i no longer ",
        "i used to ",
        "my ",
    ];
    for head in FIRST_PERSON_HEADS {
        if lower.starts_with(head) {
            // Trim trailing junk like a stray comma or quote.
            let trimmed = s.trim_end_matches([',', '"']);
            return Some(trimmed.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_blocking(msg: &str) -> Vec<String> {
        let r = RegexExtractor::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(r.extract(msg)).unwrap()
    }

    #[test]
    fn extracts_simple_first_person_facts() {
        let facts = extract_blocking("I am from Riyadh. I work as a backend engineer.");
        assert_eq!(facts.len(), 2);
        assert!(facts[0].starts_with("I am"));
        assert!(facts[1].starts_with("I work"));
    }

    #[test]
    fn extracts_negation_intact() {
        let facts = extract_blocking("I don't drink coffee anymore.");
        assert_eq!(facts.len(), 1);
        assert!(facts[0].to_lowercase().contains("don't"));
    }

    #[test]
    fn skips_unmatched_sentences() {
        let facts = extract_blocking("The weather is fine today.");
        assert!(facts.is_empty());
    }

    #[test]
    fn handles_my_possessive() {
        let facts = extract_blocking("My favourite colour is blue.");
        assert_eq!(facts.len(), 1);
        assert!(facts[0].starts_with("My favourite"));
    }
}
