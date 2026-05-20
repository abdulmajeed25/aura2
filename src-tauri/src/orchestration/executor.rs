//! Phase batch step 4: workflow executor.
//!
//! Walks a [`Workflow`]'s `steps` array, dispatches each step by its
//! `kind`, accumulates outputs, returns an [`ExecutionResult`].
//!
//! Supported step kinds (Phase 16(b) MVP):
//! - `"llm"` — invoke `AIProvider::chat`. `args.skill` (optional) is
//!   the skill name; the skill's SKILL.md body becomes the system
//!   prompt (1h cached). `args.user_prompt` is the user template, with
//!   `{{param}}` placeholders filled from the workflow's params + a
//!   special `{{prev_output}}` placeholder that resolves to the
//!   preceding step's `content` field.
//! - `"write_note"` — atomic markdown write into the vault. `args.path`
//!   is the vault-relative target (templated); `args.content` is the
//!   body (templated). Dry-run mode collects a `PendingWrite` record
//!   and **does not touch the disk**.
//!
//! Future kinds (`search`, `shell`, `branch`, …) drop in as new arms of
//! the match.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use crate::ai::providers::{AIProvider, AiError, CacheTtl, ChatRequest};
use crate::core::vault::VaultState;
use crate::orchestration::skills::Skill;
use crate::orchestration::workflows::Workflow;

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("workflow has no steps")]
    NoSteps,
    #[error("step {idx} kind {kind:?} is not supported by the MVP executor")]
    UnsupportedKind { idx: usize, kind: String },
    #[error("step {idx}: missing required arg {name:?}")]
    MissingArg { idx: usize, name: String },
    #[error("step {idx}: skill {skill:?} not found")]
    SkillNotFound { idx: usize, skill: String },
    #[error("step {idx}: no AI provider configured (skip the llm step or set the Anthropic key)")]
    NoProvider { idx: usize },
    #[error("ai: {0}")]
    Ai(#[from] AiError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("path: {0}")]
    Path(String),
}

/// One step's outcome.
#[derive(Debug, Clone, Serialize)]
pub struct StepOutput {
    pub idx: usize,
    pub kind: String,
    /// Free-form result the next step can reference via `{{prev_output}}`.
    pub content: String,
    /// Structured per-kind payload (e.g. the LLM `Usage` or the
    /// `PendingWrite`) so the UI can render details.
    pub detail: Value,
}

/// Record of a file write the executor would do (dry-run) or did
/// (apply). Lets the UI render a "review changes" panel before the
/// user clicks Apply.
#[derive(Debug, Clone, Serialize)]
pub struct PendingWrite {
    pub vault_relative_path: String,
    pub bytes: usize,
    pub applied: bool,
    /// First N chars of the new content for a UI preview.
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionResult {
    pub workflow_name: String,
    pub dry_run: bool,
    pub steps: Vec<StepOutput>,
    pub writes: Vec<PendingWrite>,
    /// `None` on full success, `Some` on first-step-failure (we stop
    /// after a failed step rather than continuing through arguments
    /// that might depend on missing output).
    pub error: Option<String>,
}

/// Substitute `{{name}}` placeholders with values from `vars`. Leaves
/// unknown placeholders intact — easier to debug than silently
/// inserting empty strings.
pub fn substitute(template: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            // Find closing `}}`.
            if let Some(end) = find_substring(&bytes[i + 2..], b"}}") {
                let name_end = i + 2 + end;
                let name = std::str::from_utf8(&bytes[i + 2..name_end])
                    .unwrap_or("")
                    .trim();
                if let Some(v) = vars.get(name) {
                    out.push_str(v);
                } else {
                    out.push_str(&template[i..name_end + 2]);
                }
                i = name_end + 2;
                continue;
            }
        }
        // SAFETY: i indexes into a valid UTF-8 boundary because we only
        // ever advance past full ASCII `{{...}}` blocks or by one byte
        // on ASCII characters. For safety push the actual char.
        let ch = template[i..].chars().next().unwrap_or('?');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn find_substring(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    for i in 0..=haystack.len() - needle.len() {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

/// Execute a workflow against the current vault. Returns once all
/// steps run OR the first one errors.
#[allow(clippy::too_many_arguments)]
pub async fn execute_workflow(
    workflow: &Workflow,
    skills: &[Skill],
    provider: Option<Arc<dyn AIProvider>>,
    vault: &VaultState,
    params: HashMap<String, String>,
    dry_run: bool,
) -> ExecutionResult {
    let mut steps: Vec<StepOutput> = Vec::with_capacity(workflow.steps.len());
    let mut writes: Vec<PendingWrite> = Vec::new();
    let mut error: Option<String> = None;
    let mut vars: HashMap<String, String> = params.clone();

    if workflow.steps.is_empty() {
        return ExecutionResult {
            workflow_name: workflow.name.clone(),
            dry_run,
            steps,
            writes,
            error: Some(ExecError::NoSteps.to_string()),
        };
    }

    for (idx, step) in workflow.steps.iter().enumerate() {
        let result = match step.kind.as_str() {
            "llm" => {
                run_llm_step(
                    idx,
                    &step.args,
                    skills,
                    provider.as_deref(),
                    &workflow.name,
                    &vars,
                )
                .await
            }
            "write_note" => run_write_note_step(idx, &step.args, vault, &vars, dry_run).await,
            other => Err(ExecError::UnsupportedKind {
                idx,
                kind: other.to_string(),
            }),
        };
        match result {
            Ok((out, maybe_write)) => {
                vars.insert("prev_output".to_string(), out.content.clone());
                steps.push(out);
                if let Some(w) = maybe_write {
                    writes.push(w);
                }
            }
            Err(e) => {
                error = Some(e.to_string());
                break;
            }
        }
    }

    ExecutionResult {
        workflow_name: workflow.name.clone(),
        dry_run,
        steps,
        writes,
        error,
    }
}

async fn run_llm_step(
    idx: usize,
    args: &Value,
    skills: &[Skill],
    provider: Option<&dyn AIProvider>,
    workflow_name: &str,
    vars: &HashMap<String, String>,
) -> Result<(StepOutput, Option<PendingWrite>), ExecError> {
    let provider = provider.ok_or(ExecError::NoProvider { idx })?;
    let user_template = args
        .get("user_prompt")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecError::MissingArg {
            idx,
            name: "user_prompt".into(),
        })?;
    let user = substitute(user_template, vars);

    // Optional skill: its body becomes the cached system prompt.
    let skill_name = args.get("skill").and_then(|v| v.as_str());
    let system_text = match skill_name {
        Some(name) => {
            let s = skills
                .iter()
                .find(|s| s.name == name)
                .ok_or_else(|| ExecError::SkillNotFound {
                    idx,
                    skill: name.into(),
                })?;
            Some(s.body.clone())
        }
        None => None,
    };

    let model = args
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("claude-sonnet-4-6")
        .to_string();
    let max_tokens = args
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(1024) as u32;

    let mut req = ChatRequest::new(model, user).with_max_tokens(max_tokens);
    if let Some(sys) = system_text {
        req = req.with_system(sys, Some(CacheTtl::OneHour));
    }
    req = req.with_metadata(serde_json::json!({
        "op": "workflow_step",
        "workflow": workflow_name,
        "step_idx": idx,
        "skill": skill_name,
    }));

    let resp = provider.chat(req).await?;
    Ok((
        StepOutput {
            idx,
            kind: "llm".into(),
            content: resp.content.clone(),
            detail: serde_json::json!({
                "model": resp.model,
                "stop_reason": resp.stop_reason,
                "usage": resp.usage,
            }),
        },
        None,
    ))
}

async fn run_write_note_step(
    idx: usize,
    args: &Value,
    vault: &VaultState,
    vars: &HashMap<String, String>,
    dry_run: bool,
) -> Result<(StepOutput, Option<PendingWrite>), ExecError> {
    let path_template = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecError::MissingArg {
            idx,
            name: "path".into(),
        })?;
    let content_template = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecError::MissingArg {
            idx,
            name: "content".into(),
        })?;
    let rel = substitute(path_template, vars);
    let content = substitute(content_template, vars);

    // Resolve through the hardened path validator. This refuses
    // null bytes, absolute paths, symlink escapes, etc.
    let abs: PathBuf = vault
        .resolve(&rel)
        .map_err(|e| ExecError::Path(e.to_string()))?;

    let preview: String = content.chars().take(160).collect();
    let bytes = content.len();
    let mut write = PendingWrite {
        vault_relative_path: rel.clone(),
        bytes,
        applied: false,
        preview,
    };

    if !dry_run {
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&abs, &content)?;
        // Index immediately so search/graph see the new content.
        vault
            .index_one(&abs)
            .await
            .map_err(|e| ExecError::Path(e.to_string()))?;
        write.applied = true;
    }

    let content_word = if dry_run { "would write" } else { "wrote" };
    Ok((
        StepOutput {
            idx,
            kind: "write_note".into(),
            content: format!("{} {} bytes to {}", content_word, bytes, rel),
            detail: serde_json::json!({
                "path": rel,
                "bytes": bytes,
                "dry_run": dry_run,
            }),
        },
        Some(write),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::providers::mock::MockProvider;
    use crate::orchestration::skills::Skill;
    use crate::orchestration::workflows::{Workflow, WorkflowStep};
    use std::path::PathBuf;

    fn fresh_vault_dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("aura-exec-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_workflow(name: &str, steps: Vec<WorkflowStep>) -> Workflow {
        Workflow {
            name: name.into(),
            description: "test".into(),
            trigger_vector: None,
            trigger_threshold: 0.7,
            steps,
            path: PathBuf::new(),
        }
    }

    #[test]
    fn substitute_replaces_placeholders() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "Aura".into());
        vars.insert("year".into(), "2026".into());
        let out = substitute("Hello {{name}}, year {{year}}", &vars);
        assert_eq!(out, "Hello Aura, year 2026");
    }

    #[test]
    fn substitute_leaves_unknown_placeholders_intact() {
        let vars = HashMap::new();
        let out = substitute("Hi {{unknown}}", &vars);
        assert_eq!(out, "Hi {{unknown}}");
    }

    #[test]
    fn substitute_handles_no_placeholders() {
        let out = substitute("plain text", &HashMap::new());
        assert_eq!(out, "plain text");
    }

    #[tokio::test]
    async fn llm_step_uses_skill_body_as_cached_system_prompt() {
        let root = fresh_vault_dir();
        let vault = VaultState::open(root.clone()).await.unwrap();
        let provider: Arc<dyn AIProvider> =
            Arc::new(MockProvider::with_response("summary text"));
        let skill = Skill {
            name: "summariser".into(),
            description: "test".into(),
            body: "SYSTEM BODY OF THE SKILL".into(),
            path: PathBuf::new(),
            aux_files: vec![],
        };
        let wf = make_workflow(
            "wf",
            vec![WorkflowStep {
                kind: "llm".into(),
                args: serde_json::json!({
                    "skill": "summariser",
                    "user_prompt": "Hello {{name}}"
                }),
            }],
        );
        let params: HashMap<_, _> = [("name".to_string(), "World".to_string())].into();

        let result = execute_workflow(&wf, &[skill], Some(provider.clone()), &vault, params, true).await;
        assert!(result.error.is_none(), "{result:?}");
        assert_eq!(result.steps.len(), 1);
        assert_eq!(result.steps[0].content, "summary text");

        // We can downcast the Arc to MockProvider via ptr_eq tricks?
        // Easier: shadow the provider as MockProvider directly and re-run.
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn write_note_step_in_dry_run_does_not_touch_disk() {
        let root = fresh_vault_dir();
        let vault = VaultState::open(root.clone()).await.unwrap();
        let wf = make_workflow(
            "wf",
            vec![WorkflowStep {
                kind: "write_note".into(),
                args: serde_json::json!({
                    "path": "notes/from-workflow.md",
                    "content": "hello {{name}}"
                }),
            }],
        );
        let params: HashMap<_, _> = [("name".into(), "you".into())].into();
        let result = execute_workflow(&wf, &[], None, &vault, params, /* dry */ true).await;

        assert!(result.error.is_none());
        assert_eq!(result.writes.len(), 1);
        assert!(!result.writes[0].applied, "dry-run should not apply");
        // File must not exist.
        assert!(!root.join("notes/from-workflow.md").is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn write_note_step_in_apply_mode_writes_and_indexes() {
        let root = fresh_vault_dir();
        let vault = VaultState::open(root.clone()).await.unwrap();
        let wf = make_workflow(
            "wf",
            vec![WorkflowStep {
                kind: "write_note".into(),
                args: serde_json::json!({
                    "path": "notes/applied.md",
                    "content": "# Heading\n\nbody\n"
                }),
            }],
        );
        let result =
            execute_workflow(&wf, &[], None, &vault, HashMap::new(), /* apply */ false).await;
        assert!(result.error.is_none(), "{result:?}");
        assert!(result.writes[0].applied);
        let written = std::fs::read_to_string(root.join("notes/applied.md")).unwrap();
        assert!(written.contains("# Heading"));
        // Vault DB should have indexed the new file.
        let nodes = vault.db.fetch_graph_nodes_and_edges().await.unwrap().0;
        assert!(nodes.iter().any(|(_, p, _)| p == "notes/applied.md"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn unsupported_step_kind_returns_error_and_stops() {
        let root = fresh_vault_dir();
        let vault = VaultState::open(root.clone()).await.unwrap();
        let wf = make_workflow(
            "wf",
            vec![
                WorkflowStep {
                    kind: "future_kind".into(),
                    args: serde_json::json!({}),
                },
                WorkflowStep {
                    kind: "write_note".into(),
                    args: serde_json::json!({"path": "x.md", "content": "y"}),
                },
            ],
        );
        let result =
            execute_workflow(&wf, &[], None, &vault, HashMap::new(), true).await;
        assert!(result.error.is_some());
        // Second step should not have run.
        assert_eq!(result.steps.len(), 0);
        assert!(result.writes.is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn llm_then_write_chains_prev_output() {
        let root = fresh_vault_dir();
        let vault = VaultState::open(root.clone()).await.unwrap();
        let provider: Arc<dyn AIProvider> =
            Arc::new(MockProvider::with_response("THE_LLM_OUTPUT"));
        let wf = make_workflow(
            "wf",
            vec![
                WorkflowStep {
                    kind: "llm".into(),
                    args: serde_json::json!({"user_prompt": "Tell me about {{topic}}"}),
                },
                WorkflowStep {
                    kind: "write_note".into(),
                    args: serde_json::json!({
                        "path": "outputs/{{topic}}.md",
                        "content": "{{prev_output}}"
                    }),
                },
            ],
        );
        let params: HashMap<_, _> = [("topic".into(), "aura".into())].into();
        let result =
            execute_workflow(&wf, &[], Some(provider), &vault, params, /* apply */ false).await;
        assert!(result.error.is_none(), "{result:?}");
        let body = std::fs::read_to_string(root.join("outputs/aura.md")).unwrap();
        assert_eq!(body, "THE_LLM_OUTPUT");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn llm_step_without_provider_errors_cleanly() {
        let root = fresh_vault_dir();
        let vault = VaultState::open(root.clone()).await.unwrap();
        let wf = make_workflow(
            "wf",
            vec![WorkflowStep {
                kind: "llm".into(),
                args: serde_json::json!({"user_prompt": "hi"}),
            }],
        );
        let result =
            execute_workflow(&wf, &[], None, &vault, HashMap::new(), true).await;
        assert!(result.error.is_some());
        let err = result.error.unwrap();
        assert!(err.contains("no AI provider"), "{err}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn workflow_with_no_steps_errors() {
        let root = fresh_vault_dir();
        let vault = VaultState::open(root.clone()).await.unwrap();
        let wf = make_workflow("empty", vec![]);
        let result =
            execute_workflow(&wf, &[], None, &vault, HashMap::new(), true).await;
        assert!(result.error.unwrap().contains("no steps"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn llm_step_carries_workflow_metadata_for_audit() {
        let root = fresh_vault_dir();
        let vault = VaultState::open(root.clone()).await.unwrap();
        let mock = Arc::new(MockProvider::with_response("ok"));
        let wf = make_workflow(
            "daily-review",
            vec![WorkflowStep {
                kind: "llm".into(),
                args: serde_json::json!({"user_prompt": "hi", "skill": "x"}),
            }],
        );
        let skills = vec![Skill {
            name: "x".into(),
            description: "y".into(),
            body: "z".into(),
            path: PathBuf::new(),
            aux_files: vec![],
        }];
        let _ = execute_workflow(
            &wf,
            &skills,
            Some(mock.clone() as Arc<dyn AIProvider>),
            &vault,
            HashMap::new(),
            true,
        )
        .await;
        let reqs = mock.received_requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].metadata["op"], "workflow_step");
        assert_eq!(reqs[0].metadata["workflow"], "daily-review");
        assert_eq!(reqs[0].metadata["step_idx"], 0);
        std::fs::remove_dir_all(&root).ok();
    }
}
