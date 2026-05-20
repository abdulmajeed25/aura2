//! Phase batch step 6: Mem0-style fact memory.
//!
//! Closes part of stand-in #15 (memory layer). Pattern (per the Mem0
//! paper, arxiv:2504.19413): for each candidate fact extracted from a
//! conversation, query the existing fact store for similar facts; the
//! engine then commits one of four operations:
//!
//! - `ADD`    — the candidate is genuinely new.
//! - `UPDATE` — the candidate refines or supersedes an existing fact.
//! - `DELETE` — the candidate contradicts (negates) an existing fact.
//! - `NOOP`   — the candidate is already represented.
//!
//! The real Mem0 system uses one LLM call for extraction and another
//! for the ADD/UPDATE/DELETE/NOOP decision. Aura keeps that swap as
//! traits ([`FactExtractor`], [`DecisionPolicy`]) so the LLM call sites
//! land in `crate::ai::providers::anthropic` once the user supplies a
//! key. Today's build ships a deterministic regex/rule stand-in for
//! both, so every unit test runs offline.
//!
//! What's NOT in this gate (Letta sleeptime, free-energy-gated memory
//! consolidation, Mem0 graph mode) is part of the second half of stand-in
//! #15 and is scheduled in [`docs/STAND_IN_REGISTRY.md`].

pub mod extractor;
pub mod mem0;
pub mod store;

pub use extractor::{ExtractError, FactExtractor, RegexExtractor};
pub use mem0::{Decision, DecisionPolicy, Mem0Engine, RuleDecisionPolicy};
pub use store::{Fact, FactHistoryEntry, FactStore};
