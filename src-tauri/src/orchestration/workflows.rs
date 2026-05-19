//! Workflow definition loader.
//!
//! A workflow is one JSON file under `<vault>/.aura/workflows/`:
//!
//! ```json
//! {
//!   "name": "Daily journal review",
//!   "description": "Pull the last 24h of journal entries and summarise.",
//!   "trigger_vector": null,
//!   "steps": [
//!     { "kind": "search", "args": { "query": "tag:journal", "limit": 10 } },
//!     { "kind": "llm", "args": { "prompt": "Summarise these journal entries…" } }
//!   ]
//! }
//! ```
//!
//! Phase 16(a) ships the loader + validation. The executor that walks
//! the `steps` array (and dispatches to search / llm / file write / etc.)
//! lands once the LLM provider trait is wired (the API-key gate).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub description: String,
    /// Resonance trigger: when the cortex's `cognitive_state` has cosine
    /// similarity above `trigger_threshold` to this vector, the UI
    /// auto-suggests the workflow. `None` = manual-only.
    pub trigger_vector: Option<Vec<f32>>,
    #[serde(default = "default_threshold")]
    pub trigger_threshold: f32,
    pub steps: Vec<WorkflowStep>,
    /// Resolved absolute path the workflow was loaded from. Skipped in
    /// JSON via `#[serde(default)]` so a workflow author doesn't have to
    /// supply it.
    #[serde(default, skip_deserializing)]
    pub path: PathBuf,
}

fn default_threshold() -> f32 {
    0.7
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step verb: `"search"`, `"llm"`, `"write_note"`, `"shell"`, …
    /// Unknown kinds are accepted at load time (forward compat); the
    /// executor decides which to handle.
    pub kind: String,
    /// Free-form args object — the executor for `kind` interprets it.
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowLoadReport {
    pub loaded: Vec<Workflow>,
    pub skipped: Vec<WorkflowSkip>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowSkip {
    pub path: PathBuf,
    pub reason: String,
}

/// Load every `*.json` file under `dir`. Files that fail to parse are
/// reported in `skipped`, not propagated as a hard error.
pub fn load_workflows(dir: &Path) -> WorkflowLoadReport {
    let mut report = WorkflowLoadReport {
        loaded: Vec::new(),
        skipped: Vec::new(),
    };
    if !dir.is_dir() {
        return report;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            report.skipped.push(WorkflowSkip {
                path: dir.to_path_buf(),
                reason: format!("read_dir: {e}"),
            });
            return report;
        }
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        match parse_workflow(&p) {
            Ok(wf) => report.loaded.push(wf),
            Err(reason) => report.skipped.push(WorkflowSkip { path: p, reason }),
        }
    }
    report.loaded.sort_by(|a, b| a.name.cmp(&b.name));
    report
}

fn parse_workflow(path: &Path) -> Result<Workflow, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
    let mut wf: Workflow =
        serde_json::from_str(&raw).map_err(|e| format!("json: {e}"))?;
    if wf.name.is_empty() {
        return Err("`name` is empty".into());
    }
    if wf.steps.is_empty() {
        return Err("workflow has no steps".into());
    }
    wf.path = path.to_path_buf();
    Ok(wf)
}

/// Cosine similarity helper for resonance triggers. Same shape as
/// `core::embeddings::cosine_similarity` but local so this module
/// doesn't depend on a specific encoder dimension.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-9);
    dot / denom
}

/// Which workflows fire given the current cortex state? Returns those
/// whose `trigger_vector` has cosine ≥ `trigger_threshold` against
/// `state`. Manual-only workflows (`trigger_vector: None`) are never
/// returned by this filter.
pub fn resonant<'a>(workflows: &'a [Workflow], state: &[f32]) -> Vec<&'a Workflow> {
    workflows
        .iter()
        .filter(|w| {
            let Some(v) = w.trigger_vector.as_ref() else {
                return false;
            };
            cosine(state, v) >= w.trigger_threshold
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fresh_dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("aura-wf-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_wf(dir: &Path, filename: &str, body: serde_json::Value) {
        std::fs::write(dir.join(filename), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    }

    #[test]
    fn empty_dir_loads_no_workflows() {
        let dir = fresh_dir();
        let r = load_workflows(&dir);
        assert!(r.loaded.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn well_formed_workflow_loads_with_defaults() {
        let dir = fresh_dir();
        write_wf(
            &dir,
            "review.json",
            json!({
                "name": "Daily review",
                "description": "Roll up today's notes",
                "steps": [
                    { "kind": "search", "args": { "query": "today" } }
                ]
            }),
        );
        let r = load_workflows(&dir);
        assert_eq!(r.loaded.len(), 1);
        let wf = &r.loaded[0];
        assert_eq!(wf.name, "Daily review");
        assert_eq!(wf.steps.len(), 1);
        // Default threshold applied.
        assert!((wf.trigger_threshold - 0.7).abs() < 1e-6);
        assert!(wf.trigger_vector.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_workflow_is_skipped() {
        let dir = fresh_dir();
        // No steps.
        write_wf(
            &dir,
            "empty.json",
            json!({ "name": "X", "description": "no steps", "steps": [] }),
        );
        // Missing name.
        write_wf(
            &dir,
            "noname.json",
            json!({
                "name": "",
                "description": "no name",
                "steps": [{ "kind": "noop", "args": {} }]
            }),
        );
        // Pure garbage.
        std::fs::write(dir.join("garbage.json"), "not json at all").unwrap();
        // One good.
        write_wf(
            &dir,
            "ok.json",
            json!({
                "name": "OK",
                "description": "Loads fine",
                "steps": [{ "kind": "noop", "args": {} }]
            }),
        );
        let r = load_workflows(&dir);
        assert_eq!(r.loaded.len(), 1);
        assert_eq!(r.loaded[0].name, "OK");
        assert_eq!(r.skipped.len(), 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_json_files_are_ignored() {
        let dir = fresh_dir();
        std::fs::write(dir.join("README.md"), "# Workflows directory").unwrap();
        std::fs::write(dir.join(".gitkeep"), "").unwrap();
        write_wf(
            &dir,
            "ok.json",
            json!({
                "name": "OK",
                "description": "Loads fine",
                "steps": [{ "kind": "noop", "args": {} }]
            }),
        );
        let r = load_workflows(&dir);
        assert_eq!(r.loaded.len(), 1);
        assert_eq!(r.skipped.len(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resonant_picks_workflows_whose_trigger_matches() {
        let mut wfs: Vec<Workflow> = vec![
            Workflow {
                name: "Aligned".into(),
                description: "yes".into(),
                trigger_vector: Some(vec![1.0, 0.0, 0.0]),
                trigger_threshold: 0.5,
                steps: vec![WorkflowStep {
                    kind: "noop".into(),
                    args: json!({}),
                }],
                path: PathBuf::new(),
            },
            Workflow {
                name: "Off-axis".into(),
                description: "no".into(),
                trigger_vector: Some(vec![0.0, 0.0, 1.0]),
                trigger_threshold: 0.5,
                steps: vec![WorkflowStep {
                    kind: "noop".into(),
                    args: json!({}),
                }],
                path: PathBuf::new(),
            },
            Workflow {
                name: "Manual".into(),
                description: "trigger=None never fires".into(),
                trigger_vector: None,
                trigger_threshold: 0.5,
                steps: vec![WorkflowStep {
                    kind: "noop".into(),
                    args: json!({}),
                }],
                path: PathBuf::new(),
            },
        ];
        wfs.sort_by(|a, b| a.name.cmp(&b.name));
        let state = vec![1.0_f32, 0.0, 0.0];
        let fired = resonant(&wfs, &state);
        let names: Vec<&str> = fired.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["Aligned"]);
    }
}
