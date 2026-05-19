//! Phase 9(a): real URL ingest via yt-dlp + ffprobe.
//!
//! Both binaries are detected at runtime (`tools::ToolsStatus::probe`).
//! When present, [`download_url`] runs `yt-dlp` to fetch the media into a
//! caller-provided directory, and [`probe_metadata`] runs `ffprobe` to
//! pull duration + format_name without re-encoding.
//!
//! Honest disclosure: yt-dlp's generic extractor accepts direct HTTP file
//! URLs (verified in the build sandbox against a 5.5 MB Big Buck Bunny
//! clip on GitHub raw). Site-specific extractors (YouTube, Vimeo, etc.)
//! also work — that's yt-dlp's whole job. Aura just shells out and stays
//! out of the rate-limiting / format-selection game.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UrlIngestError {
    #[error("yt-dlp not on PATH (run `pip install yt-dlp` or apt install)")]
    YtDlpMissing,
    #[error("ffprobe not on PATH (apt install ffmpeg)")]
    FfprobeMissing,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yt-dlp failed (exit {code:?}): {stderr}")]
    YtDlpFailed { code: Option<i32>, stderr: String },
    #[error("yt-dlp produced no downloadable file in {0}")]
    NoOutput(PathBuf),
    #[error("ffprobe failed: {0}")]
    FfprobeFailed(String),
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
}

#[derive(Debug, Clone)]
pub struct DownloadedMedia {
    /// Absolute path to the downloaded file.
    pub path: PathBuf,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Best-effort duration from ffprobe (`None` for images / probe failure).
    pub duration_ms: Option<i64>,
    /// `ffprobe` `format=format_name` field, e.g. `"mov,mp4,m4a,…"`.
    pub format_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DownloadOptions {
    /// Pass `--no-check-certificate` to yt-dlp. Useful behind self-signed
    /// proxy CAs (the sandbox case) but should be `false` in production.
    pub trust_self_signed: bool,
}

/// Reject URLs that aren't `http(s)://` so a caller can't trick us into
/// shelling out `file://` or `ftp://` or anything weirder.
pub fn validate_url(url: &str) -> Result<(), UrlIngestError> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(UrlIngestError::InvalidUrl(format!(
            "must start with http:// or https://, got {}",
            url.chars().take(80).collect::<String>()
        )));
    }
    if url.contains('\0') {
        return Err(UrlIngestError::InvalidUrl("null byte in URL".into()));
    }
    Ok(())
}

/// Run `yt-dlp` to download `url` into `dest_dir`. Returns the absolute path
/// of the resulting file along with size + ffprobe metadata.
///
/// `dest_dir` must already exist. The output filename template is
/// `%(title)s.%(ext)s` so different downloads don't clobber each other.
pub async fn download_url(
    url: &str,
    dest_dir: &Path,
    options: DownloadOptions,
) -> Result<DownloadedMedia, UrlIngestError> {
    validate_url(url)?;
    if which("yt-dlp").is_none() {
        return Err(UrlIngestError::YtDlpMissing);
    }
    std::fs::create_dir_all(dest_dir)?;

    // Snapshot the directory contents BEFORE yt-dlp runs so we can identify
    // the newly-downloaded file even if yt-dlp's stdout parsing is brittle.
    let before: std::collections::HashSet<PathBuf> = std::fs::read_dir(dest_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();

    let mut cmd = tokio::process::Command::new("yt-dlp");
    cmd.arg("--no-playlist")
        .arg("--no-warnings")
        .arg("--no-progress")
        .arg("--no-call-home")
        .arg("-P")
        .arg(dest_dir)
        .arg("-o")
        .arg("%(title)s.%(ext)s")
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if options.trust_self_signed {
        cmd.arg("--no-check-certificate");
    }

    let output = cmd.output().await?;
    if !output.status.success() {
        return Err(UrlIngestError::YtDlpFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    // Identify the new file.
    let after: Vec<PathBuf> = std::fs::read_dir(dest_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    let new_file = after
        .into_iter()
        .find(|p| !before.contains(p))
        .ok_or_else(|| UrlIngestError::NoOutput(dest_dir.to_path_buf()))?;

    let size_bytes = std::fs::metadata(&new_file)?.len();
    let (duration_ms, format_name) = probe_metadata(&new_file).unwrap_or((None, None));

    Ok(DownloadedMedia {
        path: new_file,
        size_bytes,
        duration_ms,
        format_name,
    })
}

/// Run `ffprobe -show_entries format=duration,format_name` on `path`.
/// Returns `(duration_ms, format_name)` — both `None` on failure (we don't
/// want a missing duration to break ingestion).
pub fn probe_metadata(path: &Path) -> Result<(Option<i64>, Option<String>), UrlIngestError> {
    if which("ffprobe").is_none() {
        return Err(UrlIngestError::FfprobeMissing);
    }
    let out = std::process::Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration,format_name")
        .arg("-of")
        .arg("default=noprint_wrappers=1")
        .arg(path)
        .output()?;
    if !out.status.success() {
        let s = String::from_utf8_lossy(&out.stderr);
        return Err(UrlIngestError::FfprobeFailed(s.into_owned()));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let mut duration_ms = None;
    let mut format_name = None;
    for line in s.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("duration=") {
            duration_ms = rest.parse::<f64>().ok().map(|sec| (sec * 1000.0) as i64);
        } else if let Some(rest) = line.strip_prefix("format_name=") {
            format_name = Some(rest.to_string());
        }
    }
    Ok((duration_ms, format_name))
}

/// Quick PATH check without spawning the binary. Used to fail fast in
/// `download_url` before doing any filesystem work.
fn which(bin: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for p in std::env::split_paths(&paths) {
        let candidate = p.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_url() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("ftp://example.com/x").is_err());
        assert!(validate_url("javascript:alert(1)").is_err());
        assert!(validate_url("").is_err());
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("http://example.com").is_ok());
    }

    #[test]
    fn rejects_null_byte_in_url() {
        assert!(validate_url("https://example.com\0/x").is_err());
    }

    /// End-to-end smoke test. Skipped when the sandbox doesn't have yt-dlp
    /// (CI of the upstream repo without our installed binaries).
    /// Downloads a 5.3 MB MIT-licensed Big Buck Bunny clip from a stable
    /// GitHub raw URL.
    #[tokio::test]
    #[ignore = "live network; opt in with `cargo test -- --ignored url_ingest_smoke`"]
    async fn url_ingest_smoke() {
        if which("yt-dlp").is_none() {
            eprintln!("skip: yt-dlp not installed");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("aura-url-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let out = download_url(
            "https://raw.githubusercontent.com/mediaelement/mediaelement-files/master/big_buck_bunny.mp4",
            &tmp,
            DownloadOptions {
                trust_self_signed: true,
            },
        )
        .await
        .expect("download");
        assert!(out.path.is_file(), "no output file");
        assert!(out.size_bytes > 1_000_000, "expected ≥ 1 MB, got {}", out.size_bytes);
        // Big Buck Bunny clip is ~60 seconds.
        if let Some(dur) = out.duration_ms {
            assert!(dur > 30_000 && dur < 90_000, "duration_ms = {dur}");
        }
        std::fs::remove_dir_all(&tmp).ok();
    }
}
