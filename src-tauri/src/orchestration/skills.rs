//! Anthropic-Skills-style loader.
//!
//! Format:
//!
//! ```text
//! <skills_dir>/
//!     <skill_name>/
//!         SKILL.md            ← YAML frontmatter + markdown body
//!         (optional supporting files: scripts, references)
//! ```
//!
//! `SKILL.md` is split by the standard `---\n…\n---\n` frontmatter
//! marker. Required frontmatter keys are `name` (string) and
//! `description` (string). Everything after the closing `---` is the
//! body — passed verbatim to the LLM when the skill is invoked.
//!
//! Skills with malformed frontmatter (missing required keys, bad YAML,
//! no closing marker) are **skipped with a warning** rather than
//! failing the load — a single user's broken skill shouldn't block
//! every other skill from being available.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    /// Canonical name from frontmatter `name:`.
    pub name: String,
    /// One-line description from frontmatter `description:`.
    pub description: String,
    /// Skill markdown body (everything after the closing frontmatter
    /// `---`). This is the text fed into the LLM when the skill is
    /// invoked.
    pub body: String,
    /// Absolute filesystem path of the `SKILL.md` we loaded from.
    pub path: PathBuf,
    /// Optional auxiliary files in the skill directory (anything that
    /// isn't `SKILL.md` itself).
    pub aux_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadReport {
    pub loaded: Vec<Skill>,
    pub skipped: Vec<SkillSkip>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillSkip {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
}

/// Load every skill under `dir`. Returns a `LoadReport` rather than a
/// hard error so the caller can surface partial success in the UI.
pub fn load_skills(dir: &Path) -> LoadReport {
    let mut report = LoadReport {
        loaded: Vec::new(),
        skipped: Vec::new(),
    };
    if !dir.is_dir() {
        return report;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            report.skipped.push(SkillSkip {
                path: dir.to_path_buf(),
                reason: format!("read_dir: {e}"),
            });
            return report;
        }
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let skill_md = p.join("SKILL.md");
        if !skill_md.is_file() {
            // Not a skill directory; silently skip.
            continue;
        }
        match parse_skill_dir(&p, &skill_md) {
            Ok(skill) => report.loaded.push(skill),
            Err(reason) => report.skipped.push(SkillSkip {
                path: skill_md,
                reason,
            }),
        }
    }
    // Stable order so the UI list is deterministic.
    report.loaded.sort_by(|a, b| a.name.cmp(&b.name));
    report
}

fn parse_skill_dir(dir: &Path, skill_md: &Path) -> Result<Skill, String> {
    let content = std::fs::read_to_string(skill_md).map_err(|e| format!("read: {e}"))?;

    // Split on `---\n` markers. Expect:
    //   "---\n<yaml>\n---\n<body>"
    let after_first = content
        .strip_prefix("---\n")
        .ok_or_else(|| "missing opening `---` frontmatter marker".to_string())?;
    let (yaml, body) = after_first
        .split_once("\n---\n")
        .or_else(|| after_first.split_once("\n---"))
        .ok_or_else(|| "missing closing `---` frontmatter marker".to_string())?;

    let fm: Frontmatter =
        serde_yaml::from_str(yaml).map_err(|e| format!("yaml: {e}"))?;
    if fm.name.is_empty() {
        return Err("`name` is empty".into());
    }
    if fm.description.is_empty() {
        return Err("`description` is empty".into());
    }

    let mut aux_files = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let path = e.path();
            if path.is_file()
                && path.file_name().map(|n| n != "SKILL.md").unwrap_or(false)
            {
                aux_files.push(path);
            }
        }
    }
    aux_files.sort();

    Ok(Skill {
        name: fm.name,
        description: fm.description,
        body: body.trim_start_matches('\n').to_string(),
        path: skill_md.to_path_buf(),
        aux_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("aura-skills-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_skill(parent: &Path, name: &str, content: &str) -> PathBuf {
        let dir = parent.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("SKILL.md");
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn empty_directory_loads_no_skills() {
        let dir = fresh_dir();
        let r = load_skills(&dir);
        assert!(r.loaded.is_empty());
        assert!(r.skipped.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn well_formed_skill_loads() {
        let dir = fresh_dir();
        write_skill(
            &dir,
            "summarise-pr",
            "---\nname: Summarise PR\ndescription: Concise GitHub PR summary\n---\n\
             # Body\nSummarise the PR's user-visible impact in 3 bullets.\n",
        );
        let r = load_skills(&dir);
        assert_eq!(r.loaded.len(), 1);
        assert_eq!(r.skipped.len(), 0);
        assert_eq!(r.loaded[0].name, "Summarise PR");
        assert!(r.loaded[0].body.starts_with("# Body"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn aux_files_are_listed() {
        let dir = fresh_dir();
        let sk = write_skill(
            &dir,
            "demo",
            "---\nname: Demo\ndescription: With aux files\n---\nbody\n",
        );
        let parent = sk.parent().unwrap();
        std::fs::write(parent.join("prompt_template.txt"), "Hello {name}").unwrap();
        std::fs::write(parent.join("helper.py"), "def hi(): return 'hi'\n").unwrap();
        let r = load_skills(&dir);
        assert_eq!(r.loaded.len(), 1);
        let s = &r.loaded[0];
        assert_eq!(s.aux_files.len(), 2);
        let names: Vec<String> = s
            .aux_files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"helper.py".to_string()));
        assert!(names.contains(&"prompt_template.txt".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_skill_is_skipped_with_reason() {
        let dir = fresh_dir();
        // Missing frontmatter entirely.
        write_skill(&dir, "no_frontmatter", "Just body text, no YAML.\n");
        // Missing required `description`.
        write_skill(
            &dir,
            "missing_desc",
            "---\nname: Has only name\n---\nbody\n",
        );
        // Good one alongside.
        write_skill(
            &dir,
            "ok",
            "---\nname: OK\ndescription: Loads fine\n---\nbody\n",
        );

        let r = load_skills(&dir);
        assert_eq!(r.loaded.len(), 1, "only one well-formed skill expected");
        assert_eq!(r.loaded[0].name, "OK");
        assert_eq!(r.skipped.len(), 2);
        for s in &r.skipped {
            assert!(!s.reason.is_empty(), "skip reason should be populated");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loaded_skills_are_sorted_by_name() {
        let dir = fresh_dir();
        write_skill(
            &dir,
            "zeta",
            "---\nname: Zeta skill\ndescription: z\n---\nbody\n",
        );
        write_skill(
            &dir,
            "alpha",
            "---\nname: Alpha skill\ndescription: a\n---\nbody\n",
        );
        write_skill(
            &dir,
            "mid",
            "---\nname: Mid skill\ndescription: m\n---\nbody\n",
        );
        let r = load_skills(&dir);
        let names: Vec<&str> = r.loaded.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha skill", "Mid skill", "Zeta skill"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_existent_directory_returns_empty_report() {
        let r = load_skills(Path::new("/nonexistent/aura/skills"));
        assert!(r.loaded.is_empty());
        assert!(r.skipped.is_empty());
    }
}
