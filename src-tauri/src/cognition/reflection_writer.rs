//! Reflection writer.
//!
//! Subscribes to cortex snapshots and materialises one Markdown file under
//! `<vault>/.aura/brain/reflections/YYYY-MM-DD/HHMMSS-trigger-tickN.md`
//! whenever the cortex shows a "notable" event — currently:
//!
//! - **`f_spike`** — free energy crossed `f_high_threshold` (default `5.0`).
//!   The cortex's predictive model is surprised; worth surfacing.
//! - **`curiosity_dip`** — curiosity score (= rate of F decrease) dropped
//!   below `curiosity_dip_threshold` (default `-1.0`). The cortex is getting
//!   *more* surprised over time; the underlying model is degrading.
//!
//! Debounced so a sustained-surprise burst only fires once per
//! `debounce_ticks` (default 100 — one second at the default 10 ms tick).
//!
//! The body is **template-only** in Phase 15a. The LLM-narrated synthesis
//! ("here's what the cortex actually noticed and what it might mean")
//! arrives in Phase 15b alongside the Claude bindings.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;
use thiserror::Error;

use crate::cognition::perpetual_loop::CortexSnapshot;

#[derive(Debug, Error)]
pub enum ReflectionError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialisation: {0}")]
    Serialise(#[from] serde_yaml::Error),
}

/// Why a reflection fired. Lands in the frontmatter for downstream filtering.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    FSpike,
    CuriosityDip,
}

impl Trigger {
    fn as_slug(self) -> &'static str {
        match self {
            Trigger::FSpike => "f-spike",
            Trigger::CuriosityDip => "curiosity-dip",
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Trigger::FSpike => "f_spike",
            Trigger::CuriosityDip => "curiosity_dip",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReflectionWriterConfig {
    pub f_high_threshold: f32,
    pub curiosity_dip_threshold: f32,
    pub debounce_ticks: u64,
}

impl Default for ReflectionWriterConfig {
    fn default() -> Self {
        Self {
            f_high_threshold: 5.0,
            curiosity_dip_threshold: -1.0,
            debounce_ticks: 100,
        }
    }
}

pub struct ReflectionWriter {
    brain_dir: PathBuf,
    config: ReflectionWriterConfig,
    last_fired_tick: u64,
    fired_count: u64,
}

impl ReflectionWriter {
    /// `<vault_root>/.aura/brain/reflections/` is created lazily on the first
    /// successful write.
    pub fn new(vault_root: &Path, config: ReflectionWriterConfig) -> Self {
        let brain_dir = vault_root.join(".aura").join("brain").join("reflections");
        Self {
            brain_dir,
            config,
            last_fired_tick: 0,
            fired_count: 0,
        }
    }

    pub fn fired_count(&self) -> u64 {
        self.fired_count
    }

    /// Apply trigger rules. Returns the trigger that fires (or `None` if the
    /// snapshot is unremarkable / inside the debounce window).
    pub fn classify(&self, snap: &CortexSnapshot) -> Option<Trigger> {
        if snap.tick.saturating_sub(self.last_fired_tick) < self.config.debounce_ticks
            && self.last_fired_tick > 0
        {
            return None;
        }
        if snap.free_energy >= self.config.f_high_threshold {
            return Some(Trigger::FSpike);
        }
        if snap.curiosity <= self.config.curiosity_dip_threshold {
            return Some(Trigger::CuriosityDip);
        }
        None
    }

    /// Synchronously: classify + write. Returns the absolute path of the
    /// reflection that was written, or `None` if no trigger fired.
    pub fn consider(&mut self, snap: &CortexSnapshot) -> Result<Option<PathBuf>, ReflectionError> {
        let Some(trigger) = self.classify(snap) else {
            return Ok(None);
        };
        let now = Utc::now();
        let path = self.write(now, trigger, snap)?;
        self.last_fired_tick = snap.tick;
        self.fired_count += 1;
        Ok(Some(path))
    }

    fn write(
        &self,
        when: DateTime<Utc>,
        trigger: Trigger,
        snap: &CortexSnapshot,
    ) -> Result<PathBuf, ReflectionError> {
        let day_dir = self.brain_dir.join(when.format("%Y-%m-%d").to_string());
        std::fs::create_dir_all(&day_dir)?;
        let fname = format!(
            "{}-{}-tick{}.md",
            when.format("%H%M%S"),
            trigger.as_slug(),
            snap.tick
        );
        let path = day_dir.join(fname);
        let body = render_markdown(when, trigger, snap)?;

        // Atomic write: stage to .tmp, fsync, rename.
        let tmp = path.with_extension("md.tmp");
        std::fs::write(&tmp, body.as_bytes())?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }
}

#[derive(Serialize)]
struct Frontmatter {
    trigger: &'static str,
    tick: u64,
    free_energy: f32,
    curiosity: f32,
    dominant_index: usize,
    created_at: String,
    template_only: bool,
}

fn render_markdown(
    when: DateTime<Utc>,
    trigger: Trigger,
    snap: &CortexSnapshot,
) -> Result<String, ReflectionError> {
    let fm = Frontmatter {
        trigger: trigger.as_str(),
        tick: snap.tick,
        free_energy: snap.free_energy,
        curiosity: snap.curiosity,
        dominant_index: snap.dominant_index,
        created_at: when.to_rfc3339(),
        template_only: true,
    };
    let yaml = serde_yaml::to_string(&fm)?;

    let title = match trigger {
        Trigger::FSpike => format!(
            "Surprise event at tick {} (F = {:.2})",
            snap.tick, snap.free_energy
        ),
        Trigger::CuriosityDip => format!(
            "Curiosity dipped at tick {} (score = {:.2})",
            snap.tick, snap.curiosity
        ),
    };

    let explanation = match trigger {
        Trigger::FSpike => format!(
            "Free energy reached **{:.3}**, above the configured surprise \
             threshold. The cortex's predictive model couldn't fit a recent \
             observation. Worth checking what just happened in your vault.",
            snap.free_energy
        ),
        Trigger::CuriosityDip => format!(
            "The curiosity / learning-progress score dropped to **{:.3}**, \
             meaning free energy has been *rising* across the recent window. \
             The cortex is getting more surprised over time — the underlying \
             model may be drifting away from reality.",
            snap.curiosity
        ),
    };

    Ok(format!(
        "---\n{yaml}---\n\n# {title}\n\n{explanation}\n\n\
         > This is a **template reflection**. Phase 15b will add LLM-narrated \
         synthesis that names *what* the cortex actually noticed and proposes \
         an interpretation.\n\n\
         ## Snapshot\n\n\
         - **Free energy:** {:.3}\n\
         - **Curiosity:** {:.3}\n\
         - **Dominant attractor:** index {}\n\
         - **Tick:** {}\n\
         - **Captured at:** {}\n",
        snap.free_energy,
        snap.curiosity,
        snap.dominant_index,
        snap.tick,
        when.to_rfc3339()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::perpetual_loop::CortexSnapshot;

    fn fake_snap(tick: u64, f: f32, curiosity: f32) -> CortexSnapshot {
        CortexSnapshot {
            tick,
            free_energy: f,
            curiosity,
            dominant_index: 0,
        }
    }

    fn tmp_dir(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("aura-refl-{}-{}", label, uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn calm_snapshot_does_not_fire() {
        let dir = tmp_dir("calm");
        let w = ReflectionWriter::new(&dir, ReflectionWriterConfig::default());
        assert_eq!(
            w.classify(&fake_snap(1, 0.5, 0.1)),
            None,
            "calm snapshot should not fire"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn f_spike_fires() {
        let dir = tmp_dir("fspike");
        let mut w = ReflectionWriter::new(&dir, ReflectionWriterConfig::default());
        let snap = fake_snap(10, 6.0, 0.0);
        assert_eq!(w.classify(&snap), Some(Trigger::FSpike));
        let path = w.consider(&snap).unwrap().expect("should have fired");
        assert!(path.is_file(), "file not written at {}", path.display());
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("trigger: f_spike"));
        assert!(body.contains("Surprise event"));
        assert!(body.contains("template_only: true"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn curiosity_dip_fires() {
        let dir = tmp_dir("dip");
        let mut w = ReflectionWriter::new(&dir, ReflectionWriterConfig::default());
        let snap = fake_snap(20, 1.0, -2.0);
        assert_eq!(w.classify(&snap), Some(Trigger::CuriosityDip));
        let path = w.consider(&snap).unwrap().expect("should have fired");
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("trigger: curiosity_dip"));
        assert!(body.contains("Curiosity dipped"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn debounce_prevents_back_to_back_writes() {
        let dir = tmp_dir("debounce");
        let config = ReflectionWriterConfig {
            debounce_ticks: 10,
            ..Default::default()
        };
        let mut w = ReflectionWriter::new(&dir, config);

        // Fire at tick 100.
        let _ = w.consider(&fake_snap(100, 9.0, 0.0)).unwrap();
        assert_eq!(w.fired_count(), 1);

        // Next 9 ticks at the same energy must be debounced.
        for t in 101..=109 {
            let snap = fake_snap(t, 9.0, 0.0);
            assert_eq!(w.classify(&snap), None, "tick {t} should be debounced");
            let r = w.consider(&snap).unwrap();
            assert!(r.is_none());
        }
        assert_eq!(w.fired_count(), 1);

        // Tick 110: debounce window expired.
        let r = w.consider(&fake_snap(110, 9.0, 0.0)).unwrap();
        assert!(r.is_some(), "tick 110 should fire after debounce");
        assert_eq!(w.fired_count(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn frontmatter_round_trips_through_yaml() {
        let when = chrono::DateTime::parse_from_rfc3339("2026-05-19T03:55:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let md = render_markdown(
            when,
            Trigger::FSpike,
            &fake_snap(42, 7.5, -0.3),
        )
        .unwrap();
        // Extract the YAML block.
        let yaml = md
            .strip_prefix("---\n")
            .and_then(|s| s.split_once("\n---\n"))
            .map(|(y, _)| y)
            .expect("frontmatter delimiters");
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed["trigger"], "f_spike");
        assert_eq!(parsed["tick"], 42);
        assert_eq!(parsed["template_only"], true);
    }

    #[test]
    fn path_layout_is_yyyy_mm_dd_then_hhmmss_tick() {
        let dir = tmp_dir("layout");
        let mut w = ReflectionWriter::new(&dir, ReflectionWriterConfig::default());
        let path = w.consider(&fake_snap(7, 10.0, 0.0)).unwrap().unwrap();
        let rel = path.strip_prefix(&dir).unwrap();
        // Expect: .aura/brain/reflections/YYYY-MM-DD/HHMMSS-f-spike-tick7.md
        let s = rel.to_string_lossy();
        assert!(s.starts_with(".aura/brain/reflections/"));
        assert!(s.ends_with("-f-spike-tick7.md"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
