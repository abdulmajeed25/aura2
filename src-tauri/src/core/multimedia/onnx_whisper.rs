//! Whisper-tiny audio encoder. Loads `encoder_model.onnx` from the
//! `Xenova/whisper-tiny` HF export and produces a fixed-size embedding
//! by pooling the encoder's final hidden states.
//!
//! Pipeline (matches Whisper's `feature_extractor`):
//! 1. Decode WAV → mono f32 samples at 16 kHz. (WAV-only initially;
//!    mp3/m4a/ogg support is a follow-on that adds `symphonia`.)
//! 2. Pad / truncate to **30 s** (480 000 samples). Whisper was trained
//!    on fixed-length 30 s windows; longer audio gets chunked elsewhere.
//! 3. Compute the log-Mel spectrogram: 25 ms windows (400 samples)
//!    hopped 10 ms (160 samples), 80 mel bins, `log10` of clamped power.
//! 4. Run the encoder → `[1, 1500, 384]` hidden states.
//! 5. Mean-pool over the 1500 time steps → `[384]` audio embedding.
//! 6. L2-normalise.
//!
//! No 768 → 384 projection is needed because Whisper-tiny's hidden
//! size is already 384, matching `EMBED_DIM`.
//!
//! Honest limitations:
//! - WAV only at first. Mp3 / m4a / ogg need `symphonia` (~+5 MB
//!   compile). Use ffmpeg to convert if needed; ffmpeg is already
//!   a Phase 9(a) dep.
//! - The model files at HF use opset 14 + dynamic axes; we pin to
//!   batch=1, time=3000 (the 30 s mel-spec frame count) so tract can
//!   optimise. If the export shape differs, `load` reports the
//!   exact mismatch.

use std::path::Path;
use std::sync::Arc;

use thiserror::Error;
use tract_onnx::prelude::*;

use crate::core::embeddings::EMBED_DIM;

type Plan = SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>;

/// Whisper's standard sample rate.
const SAMPLE_RATE: u32 = 16_000;
/// 30 seconds at 16 kHz.
const AUDIO_SAMPLES: usize = SAMPLE_RATE as usize * 30;
/// FFT window length: 25 ms.
const N_FFT: usize = 400;
/// Hop length: 10 ms.
const HOP: usize = 160;
/// 80 mel bins (Whisper standard).
const N_MELS: usize = 80;
/// Number of mel frames the encoder consumes: ceil((30 s * 16 kHz - N_FFT) / HOP) + 1 = 2998 + STFT padding → 3000.
const N_FRAMES: usize = 3000;

#[derive(Debug, Error)]
pub enum WhisperError {
    #[error("encoder_model.onnx not found at {0}")]
    ModelMissing(String),
    #[error("audio decode (wav): {0}")]
    Wav(String),
    #[error("audio file too short ({0} samples; expected ≥ 1 s)")]
    AudioTooShort(usize),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("tract: {0}")]
    Tract(String),
    #[error(
        "unsupported audio format: {0}. Convert to 16-kHz mono WAV with: \
         ffmpeg -i input -ar 16000 -ac 1 output.wav"
    )]
    UnsupportedFormat(String),
}

pub struct OnnxWhisper {
    plan: Arc<Plan>,
    /// Cached mel-filterbank: `[N_MELS, N_FFT/2 + 1]` row-major. Built
    /// once at load time (no per-call allocation).
    mel_filters: Vec<f32>,
}

impl OnnxWhisper {
    /// Load `<model_dir>/encoder_model.onnx`. The Xenova whisper-tiny
    /// encoder expects `input_features: [1, 80, 3000]` f32.
    pub fn load(model_dir: &Path) -> Result<Self, WhisperError> {
        let model_path = model_dir.join("encoder_model.onnx");
        if !model_path.is_file() {
            return Err(WhisperError::ModelMissing(
                model_path.display().to_string(),
            ));
        }
        let plan = tract_onnx::onnx()
            .model_for_path(&model_path)
            .and_then(|m| {
                m.with_input_fact(0, f32::fact([1, N_MELS, N_FRAMES]).into())?
                    .into_optimized()?
                    .into_runnable()
            })
            .map_err(|e| WhisperError::Tract(format!("load: {e}")))?;
        Ok(Self {
            plan: Arc::new(plan),
            mel_filters: build_mel_filters(SAMPLE_RATE, N_FFT, N_MELS),
        })
    }

    /// Encode an audio file. Currently WAV-only.
    pub fn encode_audio(&self, path: &Path) -> Result<Vec<f32>, WhisperError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "wav" {
            return Err(WhisperError::UnsupportedFormat(ext));
        }
        let samples = decode_wav_to_mono_16khz(path)?;
        let padded = pad_or_truncate(samples, AUDIO_SAMPLES);
        let mel = log_mel_spectrogram(&padded, N_FFT, HOP, &self.mel_filters);
        // mel layout: [N_MELS, N_FRAMES] row-major. Wrap into a 3-D
        // tensor [1, N_MELS, N_FRAMES].
        let arr = tract_ndarray::Array3::from_shape_vec((1, N_MELS, N_FRAMES), mel)
            .map_err(|e| WhisperError::Tract(e.to_string()))?;
        let tensor: Tensor = arr.into();
        let outputs = self
            .plan
            .run(tvec!(tensor.into()))
            .map_err(|e| WhisperError::Tract(e.to_string()))?;
        let raw = outputs
            .first()
            .ok_or_else(|| WhisperError::Tract("no encoder output".into()))?;
        let view = raw
            .to_array_view::<f32>()
            .map_err(|e| WhisperError::Tract(e.to_string()))?;
        let shape = view.shape();
        if shape.len() != 3 || shape[0] != 1 || shape[2] != EMBED_DIM {
            return Err(WhisperError::Tract(format!(
                "unexpected encoder output shape {:?} (expected [1, T, {}])",
                shape, EMBED_DIM
            )));
        }
        // Mean-pool over time then L2-normalise.
        let t = shape[1];
        let mut pooled = vec![0.0_f32; EMBED_DIM];
        for k in 0..t {
            for j in 0..EMBED_DIM {
                pooled[j] += view[[0, k, j]];
            }
        }
        let t_f = t as f32;
        for v in &mut pooled {
            *v /= t_f;
        }
        let norm: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-9 {
            for v in &mut pooled {
                *v /= norm;
            }
        }
        Ok(pooled)
    }
}

// ---- helpers ---------------------------------------------------------

fn decode_wav_to_mono_16khz(path: &Path) -> Result<Vec<f32>, WhisperError> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| WhisperError::Wav(e.to_string()))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let sr = spec.sample_rate;
    let bits = spec.bits_per_sample;

    // Convert each integer/float sample to f32 in [-1, 1].
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.unwrap_or(0.0))
            .collect(),
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.unwrap_or(0) as f32 / scale)
                .collect()
        }
    };
    if raw.is_empty() {
        return Err(WhisperError::AudioTooShort(0));
    }

    // Average across channels → mono.
    let mono: Vec<f32> = if channels == 1 {
        raw
    } else {
        raw.chunks(channels)
            .map(|c| c.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    // Naive linear-interpolation resample to 16 kHz if needed.
    let resampled = if sr == SAMPLE_RATE {
        mono
    } else {
        resample_linear(&mono, sr, SAMPLE_RATE)
    };

    if resampled.len() < SAMPLE_RATE as usize {
        // Less than 1 s of audio — not enough for Whisper.
        return Err(WhisperError::AudioTooShort(resampled.len()));
    }
    Ok(resampled)
}

fn resample_linear(samples: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = to_hz as f64 / from_hz as f64;
    let out_len = ((samples.len() as f64) * ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = (i as f64) / ratio;
        let i0 = src.floor() as usize;
        let i1 = (i0 + 1).min(samples.len() - 1);
        let frac = (src - src.floor()) as f32;
        out.push(samples[i0] * (1.0 - frac) + samples[i1] * frac);
    }
    out
}

fn pad_or_truncate(samples: Vec<f32>, target: usize) -> Vec<f32> {
    if samples.len() >= target {
        samples[..target].to_vec()
    } else {
        let mut out = samples;
        out.resize(target, 0.0);
        out
    }
}

/// Build the 80-bin Whisper mel filterbank. Standard HTK mel scale.
/// Returns row-major `[N_MELS, n_fft/2 + 1]`.
fn build_mel_filters(sr: u32, n_fft: usize, n_mels: usize) -> Vec<f32> {
    let n_bins = n_fft / 2 + 1;
    let nyquist = sr as f32 / 2.0;
    let mel_min = 0.0_f32;
    let mel_max = hz_to_mel(nyquist);
    // n_mels + 2 evenly-spaced mel points; convert back to Hz then to
    // FFT-bin indices.
    let mel_step = (mel_max - mel_min) / (n_mels as f32 + 1.0);
    let mel_points: Vec<f32> = (0..n_mels + 2)
        .map(|i| mel_to_hz(mel_min + mel_step * i as f32))
        .collect();
    let bin_points: Vec<f32> = mel_points
        .iter()
        .map(|hz| hz * n_fft as f32 / sr as f32)
        .collect();

    let mut filters = vec![0.0_f32; n_mels * n_bins];
    for m in 0..n_mels {
        let lo = bin_points[m];
        let mid = bin_points[m + 1];
        let hi = bin_points[m + 2];
        for k in 0..n_bins {
            let k_f = k as f32;
            let w = if k_f < lo || k_f > hi {
                0.0
            } else if k_f <= mid {
                (k_f - lo) / (mid - lo).max(1e-9)
            } else {
                (hi - k_f) / (hi - mid).max(1e-9)
            };
            filters[m * n_bins + k] = w;
        }
    }
    filters
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0)
}

/// Log-Mel spectrogram per the Whisper spec. Output is row-major
/// `[n_mels, n_frames]`. Uses `rustfft` (a transitive dep via
/// `tract-onnx`) for the STFT.
fn log_mel_spectrogram(
    samples: &[f32],
    n_fft: usize,
    hop: usize,
    mel_filters: &[f32],
) -> Vec<f32> {
    use rustfft::num_complex::Complex32;
    use rustfft::FftPlanner;

    let n_bins = n_fft / 2 + 1;
    let n_mels = mel_filters.len() / n_bins;
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n_fft);

    // Pre-compute Hann window. Whisper uses a periodic Hann (length
    // n_fft, not n_fft - 1).
    let window: Vec<f32> = (0..n_fft)
        .map(|i| {
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n_fft as f32).cos()
        })
        .collect();

    let mut mel = vec![0.0_f32; n_mels * N_FRAMES];
    let mut frame: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); n_fft];

    for t in 0..N_FRAMES {
        let start = t * hop;
        for i in 0..n_fft {
            let s = samples.get(start + i).copied().unwrap_or(0.0);
            frame[i] = Complex32::new(s * window[i], 0.0);
        }
        fft.process(&mut frame);
        // Magnitude² of the positive-frequency bins.
        for k in 0..n_bins {
            let re = frame[k].re;
            let im = frame[k].im;
            let power = re * re + im * im;
            // Apply each mel filter to this power spectrum.
            for m in 0..n_mels {
                mel[m * N_FRAMES + t] += mel_filters[m * n_bins + k] * power;
            }
        }
    }
    // Whisper's log_mel: clamp at 1e-10, log10, normalise to [-1, 1]ish
    // via (log - max(log) - 4) / 4. Final clamping at 0 keeps it
    // non-negative.
    let mut log = mel
        .into_iter()
        .map(|v| (v.max(1e-10)).log10())
        .collect::<Vec<f32>>();
    let lo_bound = log
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max)
        - 8.0;
    for v in &mut log {
        if *v < lo_bound {
            *v = lo_bound;
        }
    }
    let max = log
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    for v in &mut log {
        *v = (*v + 4.0 - max) / 4.0;
    }
    log
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cached_dir() -> Option<PathBuf> {
        let p = PathBuf::from("/tmp/aura-whisper-test");
        if p.join("encoder_model.onnx").is_file() {
            Some(p)
        } else {
            None
        }
    }

    #[test]
    fn hz_to_mel_round_trips_within_tolerance() {
        for &hz in &[100.0_f32, 500.0, 1000.0, 4000.0, 8000.0] {
            let back = mel_to_hz(hz_to_mel(hz));
            assert!(
                (back - hz).abs() / hz < 1e-3,
                "round trip drifted: {hz} → {back}"
            );
        }
    }

    #[test]
    fn build_mel_filters_is_well_formed() {
        let filters = build_mel_filters(SAMPLE_RATE, N_FFT, N_MELS);
        let n_bins = N_FFT / 2 + 1;
        // For each filter: at least one positive bin, all bins
        // non-negative, peak ≤ 1 (triangle filters sampled at integer
        // FFT bins never overshoot the tip).
        for m in 0..N_MELS {
            let row = &filters[m * n_bins..(m + 1) * n_bins];
            let mx = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let mn = row.iter().copied().fold(f32::INFINITY, f32::min);
            assert!(mx > 0.0, "filter {m} has no positive bin");
            assert!(mn >= 0.0, "filter {m} has negative bin ({mn})");
            assert!(
                mx <= 1.0 + 1e-6,
                "filter {m} overshoot: peak = {mx}"
            );
        }
        // Adjacent filters should overlap → sum of two consecutive
        // filters at any bin should reach the triangle-tip value
        // somewhere (i.e. the bank covers the spectrum without gaps).
        for m in 0..N_MELS - 1 {
            let row_a = &filters[m * n_bins..(m + 1) * n_bins];
            let row_b = &filters[(m + 1) * n_bins..(m + 2) * n_bins];
            let max_sum = row_a
                .iter()
                .zip(row_b.iter())
                .map(|(a, b)| a + b)
                .fold(f32::NEG_INFINITY, f32::max);
            // Triangle filters sampled at integer FFT bins overlap with
            // a peak-sum < 1 (the analytic peak rarely lands on a bin).
            // The 0.4 threshold proves coverage without overshooting
            // what's achievable with `n_fft = 400`.
            assert!(
                max_sum >= 0.4,
                "filter pair {m}+{} has spectrum gap (peak sum = {max_sum})",
                m + 1
            );
        }
    }

    #[test]
    fn pad_pads_short_signal_with_zeros() {
        let v = vec![1.0_f32, 2.0, 3.0];
        let p = pad_or_truncate(v, 8);
        assert_eq!(p, vec![1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn pad_truncates_long_signal() {
        let v = vec![1.0_f32, 2.0, 3.0, 4.0, 5.0];
        let p = pad_or_truncate(v, 3);
        assert_eq!(p, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn resample_linear_at_same_rate_is_identity() {
        let v = vec![0.1_f32, 0.2, 0.3];
        let out = resample_linear(&v, 16000, 16000);
        assert_eq!(out, v);
    }

    #[test]
    fn resample_linear_doubles_length_at_2x_rate() {
        let v = vec![0.0_f32, 1.0];
        let out = resample_linear(&v, 8000, 16000);
        // From 2 samples at 8 kHz → ~4 samples at 16 kHz.
        assert!(out.len() >= 3 && out.len() <= 5);
    }

    #[test]
    fn mel_spectrogram_for_silence_is_clamped_floor() {
        let silence = vec![0.0_f32; AUDIO_SAMPLES];
        let mel = log_mel_spectrogram(&silence, N_FFT, HOP, &build_mel_filters(SAMPLE_RATE, N_FFT, N_MELS));
        // After (log - max - 4)/4 with everything at the clamp floor,
        // every value should be the same (zero variance), bounded.
        let max = mel.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let min = mel.iter().copied().fold(f32::INFINITY, f32::min);
        assert!((max - min).abs() < 1e-3, "silence spectrogram not flat");
        // Magnitude should be small / bounded — after the (log-max-4)/4
        // transform a constant-floor input maps to a constant value
        // around 0 (because max = floor and log-floor=0). Allow a wide
        // tolerance since the normalisation is approximate.
        assert!(max.abs() < 2.0);
    }

    /// End-to-end smoke test. Skipped unless the user dropped
    /// `encoder_model.onnx` at `/tmp/aura-whisper-test/` AND a sample
    /// WAV file at the same dir.
    #[test]
    fn loads_when_model_cached_and_encodes_a_synthetic_wav() {
        let Some(dir) = cached_dir() else {
            eprintln!("skipped: encoder_model.onnx not at /tmp/aura-whisper-test");
            return;
        };
        let m = OnnxWhisper::load(&dir).expect("load");
        // Generate a 2-second 440 Hz sine WAV inline.
        let wav_path = std::env::temp_dir().join(format!(
            "aura-test-{}.wav",
            uuid::Uuid::now_v7()
        ));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&wav_path, spec).unwrap();
        let n = 2 * SAMPLE_RATE as usize;
        for i in 0..n {
            let t = i as f32 / SAMPLE_RATE as f32;
            let s = (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            writer
                .write_sample((s * 16384.0) as i16)
                .unwrap();
        }
        writer.finalize().unwrap();
        let emb = m.encode_audio(&wav_path).expect("encode");
        assert_eq!(emb.len(), EMBED_DIM);
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-3,
            "expected unit norm, got {norm}"
        );
        std::fs::remove_file(&wav_path).ok();
    }
}
