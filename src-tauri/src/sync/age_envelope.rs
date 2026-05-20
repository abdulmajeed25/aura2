//! `age` envelope encryption — Phase 17 polish, autonomous-batch step 6.
//!
//! Two flavours:
//! - [`encrypt`] / [`decrypt`] over an `age` passphrase (user-friendly,
//!   single-secret). Memory-hard scrypt KDF so a leaked file isn't
//!   trivially crackable.
//!
//! Not yet wired to any remote. The intended consumers are:
//! 1. Local "exported vault snapshot" feature — a single `.age` blob
//!    the user can drop into iCloud / Drive / Dropbox.
//! 2. Future Aura Cloud sync — chunks the vault, age-encrypts each
//!    chunk, ships the cipher-blob, server stores opaque bytes.
//!
//! The trait surface lives at the [`crate::sync`] module level so a
//! `recipient`-based variant (asymmetric, public-key) lands here later
//! as `encrypt_to_recipients` without breaking callers.

use std::io::{Read, Write};

use age::secrecy::SecretString;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EnvelopeError {
    #[error("age encrypt: {0}")]
    Encrypt(String),
    #[error("age decrypt: {0}")]
    Decrypt(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("passphrase too short ({len} < 12 chars)")]
    PassphraseTooShort { len: usize },
}

/// Encrypt `plaintext` under a passphrase. Returns an age-armoured
/// (ASCII) ciphertext so callers can paste it into a text field.
///
/// The 12-char passphrase floor is the same one the age CLI suggests;
/// shorter passphrases trigger [`EnvelopeError::PassphraseTooShort`]
/// rather than silently producing a weak envelope.
pub fn encrypt(plaintext: &[u8], passphrase: &str) -> Result<Vec<u8>, EnvelopeError> {
    if passphrase.chars().count() < 12 {
        return Err(EnvelopeError::PassphraseTooShort {
            len: passphrase.chars().count(),
        });
    }
    let secret = SecretString::from(passphrase.to_string());
    let encryptor = age::Encryptor::with_user_passphrase(secret);

    let mut output = Vec::with_capacity(plaintext.len() + 256);
    let armored = age::armor::ArmoredWriter::wrap_output(
        &mut output,
        age::armor::Format::AsciiArmor,
    )
    .map_err(|e| EnvelopeError::Encrypt(e.to_string()))?;

    let mut writer = encryptor
        .wrap_output(armored)
        .map_err(|e| EnvelopeError::Encrypt(e.to_string()))?;
    writer.write_all(plaintext)?;
    let armored = writer
        .finish()
        .map_err(|e| EnvelopeError::Encrypt(e.to_string()))?;
    armored
        .finish()
        .map_err(|e| EnvelopeError::Encrypt(e.to_string()))?;
    Ok(output)
}

/// Decrypt the inverse of [`encrypt`].
pub fn decrypt(ciphertext: &[u8], passphrase: &str) -> Result<Vec<u8>, EnvelopeError> {
    let secret = SecretString::from(passphrase.to_string());
    let armored = age::armor::ArmoredReader::new(ciphertext);
    let decryptor = age::Decryptor::new(armored)
        .map_err(|e| EnvelopeError::Decrypt(e.to_string()))?;
    let identity = age::scrypt::Identity::new(secret);
    let identities: [&dyn age::Identity; 1] = [&identity];
    let mut reader = decryptor
        .decrypt(identities.into_iter())
        .map_err(|e| EnvelopeError::Decrypt(e.to_string()))?;
    let mut out = Vec::new();
    reader.read_to_end(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASS: &str = "correct-horse-battery-staple-99";

    #[test]
    fn roundtrip_small_payload() {
        let plain = b"hello, vault";
        let cipher = encrypt(plain, PASS).expect("encrypt");
        assert_ne!(cipher.as_slice(), plain.as_slice());
        let decrypted = decrypt(&cipher, PASS).expect("decrypt");
        assert_eq!(decrypted, plain);
    }

    #[test]
    fn roundtrip_64kb_payload() {
        let plain: Vec<u8> = (0..65_536u32).map(|i| (i % 256) as u8).collect();
        let cipher = encrypt(&plain, PASS).expect("encrypt");
        let decrypted = decrypt(&cipher, PASS).expect("decrypt");
        assert_eq!(decrypted, plain);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let cipher = encrypt(b"secret", PASS).expect("encrypt");
        let bad = decrypt(&cipher, "definitely-not-right");
        assert!(matches!(bad, Err(EnvelopeError::Decrypt(_))));
    }

    #[test]
    fn short_passphrase_rejected() {
        let err = encrypt(b"x", "short").unwrap_err();
        assert!(matches!(err, EnvelopeError::PassphraseTooShort { .. }));
    }

    /// The armoured output is ASCII (no random bytes), so it round-trips
    /// through a text channel (clipboard, gist, email body).
    #[test]
    fn armored_output_is_ascii() {
        let cipher = encrypt(b"x", PASS).expect("encrypt");
        for &b in &cipher {
            assert!(
                b.is_ascii(),
                "non-ASCII byte 0x{:02x} in armoured ciphertext",
                b
            );
        }
        let s = std::str::from_utf8(&cipher).expect("valid UTF-8");
        assert!(s.starts_with("-----BEGIN AGE ENCRYPTED FILE-----"));
        assert!(s.trim_end().ends_with("-----END AGE ENCRYPTED FILE-----"));
    }
}
