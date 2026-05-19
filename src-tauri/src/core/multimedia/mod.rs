//! Phase 9: local multimedia ingestion + unified-space encoding.
//!
//! Aura's spec calls for Whisper-tiny + SigLIP-small ONNX models and yt-dlp
//! for URL ingestion. None of those are available in the headless sandbox,
//! so this module ships a real but limited stand-in:
//!
//! - Kind detection from file extension (audio / video / image).
//! - A 384-dim "media fingerprint" encoder that bundles the textual
//!   description (filename + folder + kind) with a byte-window hash of the
//!   file content. Two copies of the same file land at the same point in
//!   the space; renames change only the textual half so they're still
//!   nearby. The dim matches the text encoder so media and text hits share
//!   one cosine space (the spec's "unified embedding").
//! - A `tools` submodule that probes for `yt-dlp` / `ffmpeg` / `ffprobe`
//!   so the UI can grey out URL ingestion until they exist on PATH.
//!
//! Real Whisper/SigLIP swap: implement
//! `core::multimedia::encoder::MediaEncoder` against ONNX, drop in the new
//! struct, and re-`scan_media`. No schema change is required.

pub mod tools;
pub mod url_ingest;

use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::core::embeddings::{TextEncoder, EMBED_DIM};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Audio,
    Video,
    Image,
}

impl MediaKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaKind::Audio => "audio",
            MediaKind::Video => "video",
            MediaKind::Image => "image",
        }
    }
}

/// File extensions Aura is willing to index as media. Mirrors what most
/// vaults actually contain.
pub fn detect_kind(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "mp3" | "wav" | "ogg" | "flac" | "m4a" | "aac" | "opus" => Some(MediaKind::Audio),
        "mp4" | "mov" | "mkv" | "webm" | "avi" => Some(MediaKind::Video),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" => Some(MediaKind::Image),
        _ => None,
    }
}

/// Build a short textual description used for both the user-facing label
/// and as the text-half of the unified embedding.
pub fn describe(path_rel: &str, kind: MediaKind, size_bytes: u64) -> String {
    let stem = Path::new(path_rel)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent = Path::new(path_rel)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let human_size = human_bytes(size_bytes);

    let mut out = format!("{} {} {} {}", kind.as_str(), stem, parent, human_size);
    // Normalise whitespace so the description is friendly to FTS later.
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    out
}

fn human_bytes(n: u64) -> String {
    if n < 1024 {
        format!("{}B", n)
    } else if n < 1024 * 1024 {
        format!("{:.1}KB", n as f64 / 1024.0)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1}MB", n as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2}GB", n as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Compute a unified-space embedding for a media file.
///
/// The result is the L2-normalised sum of:
/// - The text encoder applied to `description`.
/// - A 384-dim "byte fingerprint" derived from a few SHA-256-seeded
///   pseudo-random projections of fixed-position byte windows.
///
/// Both halves land in [-1, 1]^EMBED_DIM so addition keeps everything in
/// the same space. The byte half gives non-trivial signal even when two
/// files share the same filename (e.g. duplicate uploads → identical
/// fingerprints, different files → divergent ones).
pub fn encode_media(
    text_enc: &dyn TextEncoder,
    description: &str,
    file_bytes: &[u8],
) -> Vec<f32> {
    let text_part = text_enc.encode(description);
    let byte_fp = unit_norm(byte_fingerprint(file_bytes));

    // Description carries the user-visible signal (the words they'd actually
    // type into search); the byte fingerprint only differentiates duplicates
    // and resists trivial renames. 0.85/0.15 keeps text retrieval working
    // while still injecting per-file uniqueness.
    const TEXT_WEIGHT: f32 = 0.85;
    const BYTE_WEIGHT: f32 = 0.15;

    let mut combined: Vec<f32> = text_part
        .iter()
        .zip(byte_fp.iter())
        .map(|(t, b)| TEXT_WEIGHT * t + BYTE_WEIGHT * b)
        .collect();
    let norm = combined.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-9 {
        for x in &mut combined {
            *x /= norm;
        }
    }
    combined
}

fn unit_norm(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-9 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
    v
}

/// Hash a few fixed-position byte windows into a 384-dim vector.
///
/// We deliberately sample a constant set of windows — leading 4KB, trailing
/// 4KB, and an interior chunk if the file is large — so the fingerprint is
/// stable and cheap regardless of file size (no point reading a 200MB
/// video into memory).
fn byte_fingerprint(bytes: &[u8]) -> Vec<f32> {
    let mut out = vec![0.0f32; EMBED_DIM];
    if bytes.is_empty() {
        return out;
    }
    let window = 4096.min(bytes.len());
    let leading = &bytes[..window];
    let trailing = &bytes[bytes.len() - window..];
    project_into(&mut out, leading, 0xA110A2026);
    project_into(&mut out, trailing, 0xB220B2026);
    if bytes.len() > window * 4 {
        let mid_start = bytes.len() / 2 - window / 2;
        let mid = &bytes[mid_start..mid_start + window];
        project_into(&mut out, mid, 0xC330C2026);
    }
    out
}

fn project_into(target: &mut [f32], bytes: &[u8], seed: u64) {
    let mut hasher = Sha256::new();
    hasher.update(seed.to_le_bytes());
    hasher.update(bytes);
    let digest = hasher.finalize();
    // Stride the digest across the 384 dims so a different file produces
    // different sign patterns at different coordinates.
    for (i, slot) in target.iter_mut().enumerate() {
        let byte = digest[i % digest.len()];
        let bit = (byte >> (i % 8)) & 1;
        *slot += if bit == 1 { 1.0 } else { -1.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::embeddings::HashEmbedder;
    use std::path::PathBuf;

    fn enc() -> HashEmbedder {
        HashEmbedder::new()
    }

    #[test]
    fn detects_common_extensions() {
        assert_eq!(detect_kind(&PathBuf::from("a.mp3")), Some(MediaKind::Audio));
        assert_eq!(detect_kind(&PathBuf::from("b.MP4")), Some(MediaKind::Video));
        assert_eq!(detect_kind(&PathBuf::from("c/d.PNG")), Some(MediaKind::Image));
        assert_eq!(detect_kind(&PathBuf::from("note.md")), None);
        assert_eq!(detect_kind(&PathBuf::from("no_ext")), None);
    }

    #[test]
    fn description_contains_stem_and_folder() {
        let d = describe("Recordings/jam-session.mp3", MediaKind::Audio, 2_500_000);
        assert!(d.contains("audio"));
        assert!(d.contains("jam-session"));
        assert!(d.contains("Recordings"));
        assert!(d.contains("MB"));
    }

    #[test]
    fn encoding_is_deterministic_and_normalised() {
        let v1 = encode_media(&enc(), "audio jam-session", &[1, 2, 3, 4, 5]);
        let v2 = encode_media(&enc(), "audio jam-session", &[1, 2, 3, 4, 5]);
        assert_eq!(v1, v2);
        let norm = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3);
    }

    #[test]
    fn different_descriptions_produce_distant_embeddings() {
        // Two media files with genuinely different descriptions should land
        // far apart in cosine — that's the user-visible search property.
        let v1 = encode_media(
            &enc(),
            "audio Recordings jam-session morning routine",
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        );
        let v2 = encode_media(
            &enc(),
            "image Diagrams architecture-overview cluster",
            &[200, 201, 202, 203, 204, 205, 206, 207, 208, 209],
        );
        let cos: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        assert!(
            cos < 0.4,
            "distinct media should be far apart in cosine, got {}",
            cos
        );
    }

    #[test]
    fn same_description_different_bytes_stay_close_but_not_identical() {
        // Renaming a file (or duplicating it byte-for-byte) shouldn't yank
        // the embedding to a completely different point — the byte
        // fingerprint is only 15% of the weight by design.
        let v1 = encode_media(&enc(), "audio Recordings session-a", &[1, 2, 3, 4, 5]);
        let v2 = encode_media(&enc(), "audio Recordings session-a", &[200, 201, 202, 203, 204]);
        let cos: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        assert!(cos > 0.8, "shared description should keep them close, got {}", cos);
        assert!(cos < 0.999, "byte fingerprint should still distinguish them");
    }
}
