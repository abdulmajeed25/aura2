//! Probe `yt-dlp`, `ffmpeg`, and `ffprobe` on PATH so the frontend can
//! disable URL ingestion when they're absent (the sandbox case).

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ToolsStatus {
    pub yt_dlp: Option<String>,
    pub ffmpeg: Option<String>,
    pub ffprobe: Option<String>,
}

impl ToolsStatus {
    pub fn probe() -> Self {
        Self {
            yt_dlp: which_version("yt-dlp", &["--version"]),
            ffmpeg: which_version("ffmpeg", &["-version"]),
            ffprobe: which_version("ffprobe", &["-version"]),
        }
    }

    pub fn url_ingestion_ready(&self) -> bool {
        self.yt_dlp.is_some() && self.ffmpeg.is_some()
    }
}

fn which_version(bin: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(bin).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        Some("present".to_string())
    } else {
        Some(line.chars().take(120).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_does_not_panic_on_missing_binaries() {
        // Sandbox guarantees these are missing; we only assert no panic.
        let _ = ToolsStatus::probe();
    }
}
