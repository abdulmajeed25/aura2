//! Phase batch step 6: DSPy-style prompt self-modifier (scaffolding).
//!
//! Three pieces:
//! 1. [`PromptRegistry`] — loads `.aura/prompts/<name>/v<n>.md` files
//!    from disk. Picks the highest version as the active "champion".
//!    Falls back to a hardcoded default if the directory is empty.
//! 2. [`PromptCall`] / [`record_call`] — append-only log of every
//!    versioned-prompt call into the `prompt_calls` table
//!    (migration 008). Includes a score for ranking later.
//! 3. [`scoring`] — Wilson confidence interval helper so the eventual
//!    A/B promotion job has a single source of truth.
//!
//! What's NOT in this gate (deferred):
//! - The periodic evolution job (`tokio-cron-scheduler` every 6h)
//!   that generates variants via Sonnet, A/B tests them, promotes
//!   winners. Lands once the executor (Step 4) is exercised in the
//!   real UI and we have observed scoring data to validate against.
//! - The Sonnet-driven variant generator. Same reason.
//!
//! The registry + scoring are usable today: `llm_summarizer` and
//! `llm_answer` can opt in by reading their system prompt from the
//! registry instead of from a `const`. That migration is a one-line
//! change per call site once a user actually drops a prompt file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use thiserror::Error;

use crate::db::sqlite::VaultDb;

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("unknown prompt {0:?}")]
    Unknown(String),
    #[error("db: {0}")]
    Db(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptVersion {
    pub name: String,
    pub version: u32,
    pub body: String,
    pub path: PathBuf,
}

/// Index of `<vault>/.aura/prompts/<name>/v<n>.md` files. Populated by
/// [`PromptRegistry::load_from`]. Cheap to clone (just `Arc`s under the
/// hood would be a future optimisation; for now we own the strings).
#[derive(Debug, Default, Clone)]
pub struct PromptRegistry {
    /// `name → all versions, sorted ascending by version number`.
    entries: HashMap<String, Vec<PromptVersion>>,
    /// `name → hardcoded fallback body`. Used when no on-disk version
    /// exists for `name`. Lets calling code unconditionally ask the
    /// registry without first checking "is there a file?".
    defaults: HashMap<String, String>,
}

impl PromptRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_default(mut self, name: impl Into<String>, body: impl Into<String>) -> Self {
        self.defaults.insert(name.into(), body.into());
        self
    }

    /// Read every `<root>/<name>/v<n>.md` under the prompts directory.
    /// Silently skips non-conforming filenames.
    pub fn load_from(root: &Path, mut defaults: HashMap<String, String>) -> Result<Self, PromptError> {
        let mut entries: HashMap<String, Vec<PromptVersion>> = HashMap::new();
        if root.is_dir() {
            for name_entry in std::fs::read_dir(root)? {
                let name_entry = name_entry?;
                if !name_entry.file_type()?.is_dir() {
                    continue;
                }
                let name = name_entry.file_name().to_string_lossy().to_string();
                let mut versions: Vec<PromptVersion> = Vec::new();
                for v_entry in std::fs::read_dir(name_entry.path())? {
                    let v_entry = v_entry?;
                    let p = v_entry.path();
                    let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    let Some(ext) = p.extension().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    if ext != "md" {
                        continue;
                    }
                    if !stem.starts_with('v') {
                        continue;
                    }
                    let Ok(num) = stem[1..].parse::<u32>() else {
                        continue;
                    };
                    let body = std::fs::read_to_string(&p)?;
                    versions.push(PromptVersion {
                        name: name.clone(),
                        version: num,
                        body,
                        path: p,
                    });
                }
                versions.sort_by_key(|v| v.version);
                if !versions.is_empty() {
                    entries.insert(name, versions);
                }
            }
        }
        // If a prompt has on-disk versions, drop its default — the
        // disk wins. Otherwise keep the default for fallback.
        for name in entries.keys() {
            defaults.remove(name);
        }
        Ok(Self { entries, defaults })
    }

    /// Highest-versioned prompt body for `name`. Falls back to the
    /// hardcoded default when no on-disk versions exist.
    pub fn champion(&self, name: &str) -> Option<&str> {
        if let Some(versions) = self.entries.get(name) {
            return versions.last().map(|v| v.body.as_str());
        }
        self.defaults.get(name).map(|s| s.as_str())
    }

    /// Active version number — `u32::MAX` for hardcoded defaults so
    /// the audit row can tell "default" apart from any reasonable
    /// on-disk version.
    pub fn champion_version(&self, name: &str) -> u32 {
        self.entries
            .get(name)
            .and_then(|vs| vs.last())
            .map(|v| v.version)
            .unwrap_or(u32::MAX)
    }

    /// Every known version of every known prompt — for the UI's
    /// "Prompt Lab" panel.
    pub fn all(&self) -> Vec<&PromptVersion> {
        let mut out: Vec<&PromptVersion> =
            self.entries.values().flat_map(|v| v.iter()).collect();
        out.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
        out
    }
}

/// One observed prompt call. Logged via [`record_call`].
#[derive(Debug, Clone, Serialize)]
pub struct PromptCall {
    pub prompt_name: String,
    pub prompt_version: u32,
    pub input_hash: String,
    pub score: Option<f32>,
    pub user_feedback: i32,
    pub metadata_json: serde_json::Value,
}

/// Append one row to `prompt_calls`.
pub async fn record_call(db: &Arc<VaultDb>, call: PromptCall) -> Result<(), PromptError> {
    let ts = chrono::Utc::now().timestamp_millis();
    db.prompt_call_insert(
        ts,
        &call.prompt_name,
        call.prompt_version as i64,
        &call.input_hash,
        call.score.map(|s| s as f64),
        call.user_feedback as i64,
        &serde_json::to_string(&call.metadata_json).unwrap_or_default(),
    )
    .await
    .map_err(|e| PromptError::Db(e.to_string()))
}

/// SHA-256 hex of `input`. Cheap, deterministic, hides the prompt
/// content from anyone reading the table directly.
pub fn input_hash(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

pub mod scoring {
    //! Wilson score interval — the right way to compare A/B variants
    //! with small sample sizes. (Naive mean is misleading until you
    //! have hundreds of samples; Wilson's lower bound is conservative
    //! enough to prevent flukes from being promoted.)
    //!
    //! Reference: <https://en.wikipedia.org/wiki/Binomial_proportion_confidence_interval#Wilson_score_interval>

    /// 95%-confidence Wilson interval `(lower, upper)` for a fraction
    /// `p̂ = positives / n`. Returns `(0.0, 0.0)` when `n == 0` so
    /// callers can use the lower bound as an "I'd promote this" key.
    pub fn wilson_interval(positives: u32, n: u32) -> (f32, f32) {
        if n == 0 {
            return (0.0, 0.0);
        }
        let nf = n as f32;
        let p = positives as f32 / nf;
        // z = 1.96 ≈ 95% confidence.
        let z = 1.96_f32;
        let z2 = z * z;
        let denom = 1.0 + z2 / nf;
        let center = p + z2 / (2.0 * nf);
        let margin = z * ((p * (1.0 - p) / nf) + z2 / (4.0 * nf * nf)).sqrt();
        ((center - margin) / denom, (center + margin) / denom)
    }

    /// "Should `challenger` replace `champion`?" — yes iff the
    /// challenger's lower bound exceeds the champion's lower bound.
    pub fn should_promote(
        challenger: (u32, u32),
        champion: (u32, u32),
    ) -> bool {
        let (c_lower, _) = wilson_interval(challenger.0, challenger.1);
        let (champ_lower, _) = wilson_interval(champion.0, champion.1);
        c_lower > champ_lower
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fresh_dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("aura-prompt-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn empty_root_yields_default_only() {
        let mut defaults = HashMap::new();
        defaults.insert("greet".into(), "Say hello.".to_string());
        let r = PromptRegistry::load_from(&fresh_dir(), defaults).unwrap();
        assert_eq!(r.champion("greet"), Some("Say hello."));
        assert_eq!(r.champion_version("greet"), u32::MAX);
        assert!(r.champion("nonexistent").is_none());
    }

    #[test]
    fn loads_highest_version_as_champion() {
        let dir = fresh_dir();
        let sub = dir.join("greet");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("v1.md"), "Say hi.").unwrap();
        std::fs::write(sub.join("v3.md"), "Greet warmly with the user's name.").unwrap();
        std::fs::write(sub.join("v2.md"), "Say hello!").unwrap();
        // Non-conforming filenames should be silently skipped:
        std::fs::write(sub.join("README.md"), "ignored").unwrap();
        std::fs::write(sub.join("v_notes.md"), "ignored").unwrap();

        let r = PromptRegistry::load_from(&dir, HashMap::new()).unwrap();
        assert_eq!(r.champion("greet"), Some("Greet warmly with the user's name."));
        assert_eq!(r.champion_version("greet"), 3);
        // `all()` lists every version in sorted order.
        let all = r.all();
        assert_eq!(all.len(), 3);
        let nums: Vec<u32> = all.iter().map(|v| v.version).collect();
        assert_eq!(nums, vec![1, 2, 3]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn on_disk_version_overrides_default() {
        let dir = fresh_dir();
        let sub = dir.join("greet");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("v1.md"), "Custom body.").unwrap();
        let mut defaults = HashMap::new();
        defaults.insert("greet".into(), "Hardcoded body.".into());
        let r = PromptRegistry::load_from(&dir, defaults).unwrap();
        assert_eq!(r.champion("greet"), Some("Custom body."));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn input_hash_is_deterministic() {
        let a = input_hash("hello world");
        let b = input_hash("hello world");
        let c = input_hash("hello worle"); // one char diff
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn wilson_zero_samples_returns_zero_interval() {
        let (lo, hi) = scoring::wilson_interval(0, 0);
        assert_eq!(lo, 0.0);
        assert_eq!(hi, 0.0);
    }

    /// Sanity: for `9/10` positives, the lower bound is bounded
    /// well above 0.5, well below 0.9, and the upper bound is
    /// strictly above 0.9. Hand-computed envelope.
    #[test]
    fn wilson_lower_bound_for_9_of_10() {
        let (lo, hi) = scoring::wilson_interval(9, 10);
        assert!(lo > 0.55 && lo < 0.85, "lo = {lo}");
        assert!(hi > 0.9 && hi <= 1.0, "hi = {hi}");
    }

    /// Should-promote: a small sample of all-wins beats a larger sample
    /// of mostly-wins only if the lower bound is actually higher.
    /// `5/5` (lower≈0.57) vs `90/100` (lower≈0.83) → champion stays.
    #[test]
    fn should_promote_rejects_small_sample_perfect_record() {
        let challenger = (5_u32, 5_u32);
        let champion = (90_u32, 100_u32);
        assert!(!scoring::should_promote(challenger, champion));
    }

    /// And: `48/50` (lower≈0.84) beats `90/100` (lower≈0.83) → promote.
    #[test]
    fn should_promote_accepts_clear_winner() {
        let challenger = (48_u32, 50_u32);
        let champion = (90_u32, 100_u32);
        assert!(scoring::should_promote(challenger, champion));
    }
}
