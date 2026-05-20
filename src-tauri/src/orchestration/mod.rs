//! Phase 15/16: orchestration. v5.0 NEW directory.
//!
//! Currently:
//! - [`skills`] — load Anthropic Skills from disk. A skill is a
//!   directory containing `SKILL.md` with YAML frontmatter
//!   (`name`, `description`) plus body markdown. The loader walks
//!   `<vault>/.aura/skills/` (and optionally the user-configured
//!   global path) and returns parsed `Skill` records. Execution —
//!   feeding the skill text + arguments into an LLM — is deferred to
//!   the LLM-backed gates.
//! - [`workflows`] — load workflow definitions (JSON) from
//!   `<vault>/.aura/workflows/`. A workflow is `{ name, description,
//!   trigger_vector?, steps: [Step] }`. The runtime that actually
//!   executes them is a Phase 16(b) follow-on.
//!
//! Resonance triggers (Phase 15's "auto-suggest workflows" mechanism)
//! sit in the `Cortex::tick` loop and dot-product the cortex's current
//! state against each workflow's `trigger_vector`. That wiring lands
//! once the executor exists.

pub mod executor;
pub mod skills;
pub mod workflows;
