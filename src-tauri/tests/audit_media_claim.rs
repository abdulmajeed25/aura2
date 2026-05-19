//! Audit: the Phase 9 *Definition of Done* — "search for sound of clicking
//! AC finds a video that contains it even if not mentioned textually." The
//! shipped encoder is 0.85·text(description) + 0.15·byte_fingerprint. So
//! the spec's DoD cannot hold without a real Whisper/SigLIP swap. Let's
//! make that concrete: an image describing one concept by filename, with
//! bytes that would (in a real model) suggest another concept, gets ranked
//! purely by the filename.

use std::fs;

use aura_lib::core::multimedia::{describe, encode_media, detect_kind};

#[tokio::test]
async fn audit_media_encoder_is_filename_dominated() {
    // Same description, very different "bytes": ranking is filename-driven.
    let e_short = encode_media("audio Recordings interview-with-author",
                               b"FAKE_SHORT_BYTES");
    let e_long = encode_media("audio Recordings interview-with-author",
                              &vec![0u8; 1024 * 256]); // 256KB of zeros
    let cos: f32 = e_short.iter().zip(e_long.iter()).map(|(a, b)| a * b).sum();
    println!("audit media: same-desc/diff-bytes cosine = {:.4}", cos);
    // 0.85 weight goes to text → cosine should be very high.
    assert!(cos > 0.85, "filename dominance expected, got {}", cos);

    // Completely different "audio content" with the SAME description → still
    // very close. This is the user-visible failure of the spec DoD: the model
    // doesn't actually look at the audio.
    let e_silence = encode_media("audio Recordings interview-with-author",
                                 &vec![0u8; 4096]);
    let e_noise: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
    let e_noisy = encode_media("audio Recordings interview-with-author", &e_noise);
    let cos_audio_blind: f32 = e_silence.iter().zip(e_noisy.iter()).map(|(a, b)| a * b).sum();
    println!("audit media: silence-vs-noise (same desc) cosine = {:.4}", cos_audio_blind);
    assert!(cos_audio_blind > 0.85,
            "two files with same name but radically different audio content \
             land >0.85 apart — confirms the encoder is filename-driven");
}

#[tokio::test]
async fn audit_media_kind_detection_is_consistent() {
    let cases = vec![
        ("foo.MP3", Some("audio")),
        ("foo.mp3", Some("audio")),
        ("a/b/c.MOV", Some("video")),
        ("doc.pdf", None),
        ("plain.txt", None),
        ("file_without_extension", None),
        ("trailing_dot.", None),
    ];
    for (input, expected) in cases {
        let got = detect_kind(std::path::Path::new(input))
            .map(|k| k.as_str());
        assert_eq!(got, expected, "kind({}) mismatch", input);
    }
}

#[tokio::test]
async fn audit_media_description_does_not_leak_secrets() {
    // describe() should never embed unbounded raw user input. Path comes
    // from the vault's filesystem, so it's "trusted", but a path containing
    // newlines or control characters should still produce a sane single-line
    // description.
    let d = describe(
        "Folder/has spaces and (parens)/file with\nnewline.mp3",
        aura_lib::core::multimedia::MediaKind::Audio,
        1234,
    );
    assert!(!d.contains('\n'), "description leaked a newline: {:?}", d);
    println!("audit media: description = {:?}", d);
}
