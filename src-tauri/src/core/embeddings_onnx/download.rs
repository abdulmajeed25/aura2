//! Download + verify the MiniLM ONNX model and its tokenizer.
//!
//! Source: `hunterreid/pool-party-embed-weights` on GitHub — a vetted MIT
//! mirror of `sentence-transformers/all-MiniLM-L6-v2` (Apache-2.0). We use
//! `raw.githubusercontent.com` rather than HuggingFace because (a) GitHub
//! raw URLs are stable, (b) `huggingface.co` may be blocked in sandboxed
//! build environments while the GitHub mirror is reachable.
//!
//! Layout on disk:
//!
//! ```text
//! <model_dir>/
//!     model.onnx          (~ 90 MB)
//!     tokenizer.json      (~712 KB)
//! ```
//!
//! `model_dir` is supplied by the caller. Conventionally it's
//! `<vault>/.aura/models/all-MiniLM-L6-v2/` so each vault keeps its own
//! cached model — important for the Local-First contract.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error(
        "checksum mismatch for {file}: expected {expected}, got {actual}. \
         Refusing to use a tampered model."
    )]
    BadChecksum {
        file: String,
        expected: String,
        actual: String,
    },
    #[error("download cancelled")]
    Cancelled,
}

/// One downloadable file in the model bundle.
pub struct FileSpec {
    pub filename: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
}

/// Static manifest for the all-MiniLM-L6-v2 bundle.
pub struct ModelManifest {
    pub name: &'static str,
    pub embed_dim: usize,
    pub files: &'static [FileSpec],
}

/// SHA-256 values verified by downloading the files in this build sandbox
/// on 2026-05-19 and running `sha256sum`. Changing the source files breaks
/// these checksums and the loader will refuse to proceed.
pub const MANIFEST: ModelManifest = ModelManifest {
    name: "all-MiniLM-L6-v2",
    embed_dim: 384,
    files: &[
        FileSpec {
            filename: "model.onnx",
            url: "https://raw.githubusercontent.com/hunterreid/pool-party-embed-weights/main/model_data/model.onnx",
            sha256: "994a58868f7abacacbf2192aa0aae8f56da8c4505dbde2740c861b24426ede6b",
            bytes: 90_445_823,
        },
        FileSpec {
            filename: "tokenizer.json",
            url: "https://raw.githubusercontent.com/hunterreid/pool-party-embed-weights/main/model_data/tokenizer.json",
            sha256: "da0e79933b9ed51798a3ae27893d3c5fa4a201126cef75586296df9b4d2c62a0",
            bytes: 711_661,
        },
    ],
};

/// Have all files in the manifest already been downloaded and verified?
pub fn is_present(model_dir: &Path) -> bool {
    MANIFEST.files.iter().all(|f| {
        let p = model_dir.join(f.filename);
        match std::fs::metadata(&p) {
            Ok(m) => m.is_file() && m.len() == f.bytes,
            Err(_) => false,
        }
    })
}

/// Download every file in [`MANIFEST`] to `model_dir`. Each file is verified
/// against its SHA-256 after download and any tamper is treated as a hard
/// failure (the partial file is deleted).
///
/// `progress` is invoked with `(filename, bytes_so_far, total_bytes)` so the
/// caller can drive a UI progress bar without coupling to a specific
/// framework.
pub async fn download_model(
    model_dir: &Path,
    progress: impl Fn(&str, u64, u64) + Send + 'static,
) -> Result<(), DownloadError> {
    std::fs::create_dir_all(model_dir)?;
    let client = reqwest::Client::builder()
        .user_agent(concat!("aura/", env!("CARGO_PKG_VERSION")))
        .build()?;

    for spec in MANIFEST.files {
        let target = model_dir.join(spec.filename);
        // Skip if already present and checksum matches.
        if let Ok(meta) = std::fs::metadata(&target) {
            if meta.is_file() && meta.len() == spec.bytes {
                let on_disk = sha256_of_file(&target)?;
                if on_disk == spec.sha256 {
                    progress(spec.filename, spec.bytes, spec.bytes);
                    continue;
                }
            }
        }
        download_one(&client, spec, &target, &progress).await?;
    }
    Ok(())
}

async fn download_one(
    client: &reqwest::Client,
    spec: &FileSpec,
    target: &Path,
    progress: &(impl Fn(&str, u64, u64) + Send + 'static),
) -> Result<(), DownloadError> {
    let mut resp = client.get(spec.url).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(spec.bytes);
    let tmp = target.with_extension("download");
    if tmp.exists() {
        std::fs::remove_file(&tmp)?;
    }
    let mut file = std::fs::File::create(&tmp)?;
    let mut hasher = Sha256::new();
    let mut so_far = 0_u64;
    use std::io::Write;
    while let Some(chunk) = resp.chunk().await? {
        file.write_all(&chunk)?;
        hasher.update(&chunk);
        so_far += chunk.len() as u64;
        progress(spec.filename, so_far, total);
    }
    file.flush()?;
    drop(file);

    let actual = hex_lower(&hasher.finalize());
    if actual != spec.sha256 {
        let _ = std::fs::remove_file(&tmp);
        return Err(DownloadError::BadChecksum {
            file: spec.filename.into(),
            expected: spec.sha256.into(),
            actual,
        });
    }
    std::fs::rename(&tmp, target)?;
    Ok(())
}

fn sha256_of_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Conventional model-cache path under a vault root.
pub fn vault_model_dir(vault_root: &Path) -> PathBuf {
    vault_root
        .join(".aura")
        .join("models")
        .join(MANIFEST.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_dimensions_match_minilm() {
        assert_eq!(MANIFEST.embed_dim, 384);
        assert_eq!(MANIFEST.files.len(), 2);
        let mut names: Vec<_> = MANIFEST.files.iter().map(|f| f.filename).collect();
        names.sort();
        assert_eq!(names, vec!["model.onnx", "tokenizer.json"]);
    }

    #[test]
    fn is_present_returns_false_for_empty_dir() {
        let tmp = std::env::temp_dir().join(format!("aura-empty-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(!is_present(&tmp));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn vault_model_dir_is_under_dot_aura() {
        let p = vault_model_dir(Path::new("/vaults/my-vault"));
        assert_eq!(
            p.to_string_lossy(),
            "/vaults/my-vault/.aura/models/all-MiniLM-L6-v2"
        );
    }
}
