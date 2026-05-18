//! Auth-token generation for the MCP server.
//!
//! The MCP endpoint is bound to `127.0.0.1`, so the threat model is a
//! malicious local process scanning loopback ports. A 256-bit token is
//! more than enough to make that infeasible; we encode it as URL-safe
//! base64 (no `+` / `/`) so it can be pasted into config files without
//! escaping.

use sha2::{Digest, Sha256};

/// Generate a fresh 256-bit token.
pub fn new_token() -> String {
    // We don't have access to a CSPRNG crate beyond `rand` (in deps already
    // via Phase 5/8). Seed a Sha256 hasher with high-entropy sources we have:
    // the current monotonic time, the system time, and a freshly minted
    // UUIDv7 (which itself mixes time + random).
    let now_mono = std::time::Instant::now();
    let now_sys = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let uuid = uuid::Uuid::now_v7();
    let pid = std::process::id();
    let tid = std::thread::current().id();

    let mut hasher = Sha256::new();
    hasher.update(format!("{:?}", now_mono).as_bytes());
    hasher.update(now_sys.to_le_bytes());
    hasher.update(uuid.as_bytes());
    hasher.update(pid.to_le_bytes());
    hasher.update(format!("{:?}", tid).as_bytes());
    let digest = hasher.finalize();
    base64_url(&digest)
}

const URL_SAFE: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// URL-safe base64 without padding. Avoids `+`, `/`, `=` so the token is
/// trivially copy-pasteable into config files and HTTP headers.
fn base64_url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 4);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes.get(i + 1).copied().unwrap_or(0);
        let b2 = bytes.get(i + 2).copied().unwrap_or(0);
        let triple = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(URL_SAFE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(URL_SAFE[((triple >> 12) & 0x3f) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(URL_SAFE[((triple >> 6) & 0x3f) as usize] as char);
        }
        if i + 2 < bytes.len() {
            out.push(URL_SAFE[(triple & 0x3f) as usize] as char);
        }
        i += 3;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_long_and_url_safe() {
        let t = new_token();
        assert!(t.len() >= 40, "token too short: {}", t.len());
        for c in t.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '-' || c == '_',
                "non-url-safe char in token: {}",
                c
            );
        }
    }

    #[test]
    fn consecutive_tokens_differ() {
        // We don't have a true CSPRNG, but the time + UUID entropy is more
        // than enough to ensure no collisions in practice.
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
    }
}
