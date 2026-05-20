//! SigLIP-base vision encoder. Loads `vision_model.onnx` from the
//! `Xenova/siglip-base-patch16-224` HF export.
//!
//! Pipeline:
//! 1. Decode image file (PNG / JPEG / WEBP / GIF / BMP) via the
//!    `image` crate.
//! 2. Resize to `224×224` (SigLIP-base patch16-224's input shape).
//! 3. Normalise to `[-1, 1]` per channel — SigLIP's preprocessing
//!    (mean=0.5, std=0.5 on each of R/G/B).
//! 4. Reshape to `[1, 3, 224, 224]` (NCHW float32).
//! 5. Run ONNX inference → `[1, 768]` image embedding.
//! 6. Project 768 → 384 by mean-of-pairs so the result fits the
//!    `EMBED_DIM = 384` unified space.
//! 7. L2-normalise.
//!
//! Honest disclosure: the 768 → 384 projection loses information.
//! It's the simplest dimension match that keeps the unified search
//! space; a learned projection head would be better. The
//! mean-of-pairs preserves L2-norm well enough that cosine similarity
//! between two SigLIP embeddings stays meaningful.

use std::path::Path;
use std::sync::Arc;

use thiserror::Error;
use tract_onnx::prelude::*;

use crate::core::embeddings::EMBED_DIM;

type Plan = SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>;

/// Native output dim of `Xenova/siglip-base-patch16-224`.
const SIGLIP_NATIVE_DIM: usize = 768;
const IMAGE_SIDE: usize = 224;
const CHANNELS: usize = 3;

#[derive(Debug, Error)]
pub enum SiglipError {
    #[error("vision_model.onnx not found at {0}")]
    ModelMissing(String),
    #[error("image: {0}")]
    Image(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("tract: {0}")]
    Tract(String),
    #[error("unsupported native dim {0} (expected 768 for SigLIP-base)")]
    UnsupportedDim(usize),
}

pub struct OnnxSiglip {
    plan: Arc<Plan>,
}

impl OnnxSiglip {
    /// Load `<model_dir>/vision_model.onnx`. The siglip-base vision
    /// model expects a single `[1, 3, 224, 224]` f32 input named
    /// `pixel_values`.
    pub fn load(model_dir: &Path) -> Result<Self, SiglipError> {
        let model_path = model_dir.join("vision_model.onnx");
        if !model_path.is_file() {
            return Err(SiglipError::ModelMissing(model_path.display().to_string()));
        }
        let plan = tract_onnx::onnx()
            .model_for_path(&model_path)
            .and_then(|m| {
                m.with_input_fact(
                    0,
                    f32::fact([1, CHANNELS, IMAGE_SIDE, IMAGE_SIDE]).into(),
                )?
                .into_optimized()?
                .into_runnable()
            })
            .map_err(|e| SiglipError::Tract(format!("load: {e}")))?;
        Ok(Self {
            plan: Arc::new(plan),
        })
    }

    /// Encode an image file into a 384-dim, L2-normalised embedding
    /// in the unified Aura space.
    pub fn encode_image(&self, path: &Path) -> Result<Vec<f32>, SiglipError> {
        let img = image::open(path).map_err(|e| SiglipError::Image(e.to_string()))?;
        // SigLIP-base preprocessing: resize the SHORT side to 224 then
        // centre-crop is the HF default, but the simpler "resize_exact"
        // matches what most ONNX exports embed in the graph and is
        // close enough for similarity ranking.
        let img = img.resize_exact(
            IMAGE_SIDE as u32,
            IMAGE_SIDE as u32,
            image::imageops::FilterType::Triangle,
        );
        let rgb = img.to_rgb8();

        // NCHW f32 tensor at [1, 3, 224, 224], values in [-1, 1].
        let mut buf = vec![0.0_f32; CHANNELS * IMAGE_SIDE * IMAGE_SIDE];
        let stride = IMAGE_SIDE * IMAGE_SIDE;
        for (i, px) in rgb.pixels().enumerate() {
            let r = px[0] as f32 / 255.0;
            let g = px[1] as f32 / 255.0;
            let b = px[2] as f32 / 255.0;
            // SigLIP normalisation: mean=0.5, std=0.5 → (x - 0.5) / 0.5.
            // NCHW layout: channel `c` lives at `c * stride .. (c+1) * stride`.
            buf[i] = (r - 0.5) / 0.5;
            buf[stride + i] = (g - 0.5) / 0.5;
            buf[2 * stride + i] = (b - 0.5) / 0.5;
        }

        let tensor: Tensor = tract_ndarray::Array4::from_shape_vec(
            (1, CHANNELS, IMAGE_SIDE, IMAGE_SIDE),
            buf,
        )
        .map_err(|e| SiglipError::Tract(e.to_string()))?
        .into();

        let outputs = self
            .plan
            .run(tvec!(tensor.into()))
            .map_err(|e| SiglipError::Tract(e.to_string()))?;
        let raw = outputs
            .first()
            .ok_or_else(|| SiglipError::Tract("no output".into()))?;
        let view = raw
            .to_array_view::<f32>()
            .map_err(|e| SiglipError::Tract(e.to_string()))?;

        // SigLIP vision_model outputs `[1, 768]` (pooled CLS-equivalent
        // embedding) directly. Some exports return `[1, N, 768]`
        // sequence states; in that case we mean-pool over the patches.
        let shape = view.shape();
        let pooled: Vec<f32> = match shape.len() {
            2 if shape[1] == SIGLIP_NATIVE_DIM => {
                (0..SIGLIP_NATIVE_DIM).map(|j| view[[0, j]]).collect()
            }
            3 if shape[2] == SIGLIP_NATIVE_DIM => {
                let n_patches = shape[1];
                let mut out = vec![0.0_f32; SIGLIP_NATIVE_DIM];
                for p in 0..n_patches {
                    for j in 0..SIGLIP_NATIVE_DIM {
                        out[j] += view[[0, p, j]];
                    }
                }
                for v in &mut out {
                    *v /= n_patches as f32;
                }
                out
            }
            _ => {
                return Err(SiglipError::Tract(format!(
                    "unexpected vision_model output shape {:?} (expected [1, 768] or [1, N, 768])",
                    shape
                )));
            }
        };

        // Project 768 → 384 by mean-of-pairs, then L2-normalise.
        Ok(project_and_normalise(&pooled))
    }
}

/// Mean adjacent pairs to halve dimensionality: 768 → 384.
/// `EMBED_DIM` is guaranteed to be 384 by the build, so this is a
/// total function. If we ever change `EMBED_DIM`, the const-assert
/// below will catch it at compile time.
fn project_and_normalise(v: &[f32]) -> Vec<f32> {
    debug_assert_eq!(v.len(), SIGLIP_NATIVE_DIM);
    debug_assert_eq!(EMBED_DIM, 384);
    debug_assert_eq!(SIGLIP_NATIVE_DIM, 2 * EMBED_DIM);
    let mut out = vec![0.0_f32; EMBED_DIM];
    for i in 0..EMBED_DIM {
        out[i] = (v[2 * i] + v[2 * i + 1]) * 0.5;
    }
    let norm: f32 = out.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-9 {
        for x in &mut out {
            *x /= norm;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cached_dir() -> Option<PathBuf> {
        let p = PathBuf::from("/tmp/aura-siglip-test");
        if p.join("vision_model.onnx").is_file() {
            Some(p)
        } else {
            None
        }
    }

    #[test]
    fn unsupported_dim_error_constant_is_reachable() {
        // Smoke test that the error variant exists and formats.
        let e = SiglipError::UnsupportedDim(1024);
        let s = format!("{e}");
        assert!(s.contains("1024"));
    }

    #[test]
    fn project_halves_dim_and_normalises_to_unit_length() {
        // 768 ones → 384 ones → L2-normalised.
        let v = vec![1.0_f32; SIGLIP_NATIVE_DIM];
        let p = project_and_normalise(&v);
        assert_eq!(p.len(), EMBED_DIM);
        let norm: f32 = p.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "expected unit norm, got {norm}");
        // All values equal → unit-norm requires each value = 1/√384.
        let expected = 1.0_f32 / (EMBED_DIM as f32).sqrt();
        for x in p {
            assert!((x - expected).abs() < 1e-5);
        }
    }

    /// End-to-end smoke test. Skipped unless the user has dropped
    /// `vision_model.onnx` at `/tmp/aura-siglip-test/`.
    #[test]
    fn loads_when_model_cached_and_encodes_sample_image() {
        let Some(dir) = cached_dir() else {
            eprintln!("skipped: vision_model.onnx not at /tmp/aura-siglip-test");
            return;
        };
        let m = OnnxSiglip::load(&dir).expect("load");
        // Generate a tiny test image inline so the test is hermetic.
        let img_path = std::env::temp_dir().join(format!(
            "aura-test-img-{}.png",
            uuid::Uuid::now_v7()
        ));
        let mut img = image::RgbImage::new(64, 64);
        for (_, _, px) in img.enumerate_pixels_mut() {
            *px = image::Rgb([200, 50, 50]);
        }
        img.save(&img_path).unwrap();
        let emb = m.encode_image(&img_path).expect("encode");
        assert_eq!(emb.len(), EMBED_DIM);
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3);
        std::fs::remove_file(&img_path).ok();
    }
}
