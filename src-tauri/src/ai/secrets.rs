//! API-key loading + redacting newtype.
//!
//! Threat model: a malicious crash dump, a debug log, or a Tauri event
//! payload must NEVER carry the user's API key. The `ApiKey` newtype
//! enforces this at the type level — its `Debug` and `Display` impls
//! always print `<redacted>`. The `Serialize` impl is **not** provided
//! on purpose so the key can't accidentally be sent across an IPC
//! boundary as JSON.
//!
//! Loading priority:
//! 1. `<vault>/.aura/secrets/<name>.key` (file). On Unix we warn if
//!    permissions are looser than `0600`.
//! 2. `<NAME>_API_KEY` environment variable.
//! 3. Hard `Err(KeyError::NotFound)`.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// Redacted wrapper around an API key. The plaintext is reachable only
/// via [`Self::header_value`], which is `pub(crate)` so it can be used
/// to build an HTTP `Authorization` header but not exfiltrated by code
/// outside this crate's AI module.
#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    /// Construct from a raw string. Trimmed to defend against trailing
    /// newlines that creep in when users paste into a file.
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into().trim().to_string())
    }

    /// **AI-module-only** accessor. The value is written into an
    /// `x-api-key` HTTP header by the provider — nowhere else.
    pub(crate) fn header_value(&self) -> &str {
        &self.0
    }

    /// Number of characters in the key (lets us assert lengths in tests
    /// without exposing content).
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ApiKey(<redacted, {} chars>)", self.0.len())
    }
}

impl std::fmt::Display for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<redacted>")
    }
}

#[derive(Debug, Error)]
pub enum KeyError {
    #[error("API key not found at any of the configured locations")]
    NotFound,
    #[error("API key file at {0} is empty")]
    Empty(PathBuf),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Load the Anthropic key. See module-level doc for priority order.
pub fn load_anthropic_key(vault_root: &Path) -> Result<ApiKey, KeyError> {
    load_named_key(vault_root, "anthropic", "ANTHROPIC_API_KEY")
}

/// Load any named provider key. Convention:
///   - File: `<vault_root>/.aura/secrets/<name>.key`
///   - Env:  `<env_var>`  (e.g. `ANTHROPIC_API_KEY`)
pub fn load_named_key(
    vault_root: &Path,
    name: &str,
    env_var: &str,
) -> Result<ApiKey, KeyError> {
    let secret_path = vault_root
        .join(".aura")
        .join("secrets")
        .join(format!("{name}.key"));

    if secret_path.is_file() {
        check_permissions(&secret_path);
        let raw = std::fs::read_to_string(&secret_path)?;
        let key = ApiKey::from_string(raw);
        if key.is_empty() {
            return Err(KeyError::Empty(secret_path));
        }
        tracing::info!(
            target: "aura::secrets",
            "loaded {} key from {}", name, secret_path.display()
        );
        return Ok(key);
    }

    if let Ok(raw) = std::env::var(env_var) {
        let key = ApiKey::from_string(raw);
        if !key.is_empty() {
            tracing::info!(
                target: "aura::secrets",
                "loaded {} key from {} env var", name, env_var
            );
            return Ok(key);
        }
    }

    Err(KeyError::NotFound)
}

#[cfg(unix)]
fn check_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let mode = meta.permissions().mode() & 0o777;
    // Anything group- or other-readable is suspicious for a secret.
    if mode & 0o077 != 0 {
        tracing::warn!(
            target: "aura::secrets",
            "{} permissions are {:o}; recommend `chmod 0600`",
            path.display(),
            mode
        );
    }
}

#[cfg(not(unix))]
fn check_permissions(_path: &Path) {
    // Windows ACLs are different — we don't second-guess them here.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_vault() -> PathBuf {
        let p = std::env::temp_dir().join(format!("aura-key-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_key(vault: &Path, name: &str, value: &str) {
        let dir = vault.join(".aura").join("secrets");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("{name}.key"));
        std::fs::write(&p, value).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&p, perms).unwrap();
        }
    }

    #[test]
    fn loads_key_from_secrets_file() {
        let vault = fresh_vault();
        write_key(&vault, "anthropic", "sk-test-abc123\n");
        let key = load_anthropic_key(&vault).unwrap();
        // The trim() in `from_string` should have stripped the newline.
        assert_eq!(key.len(), "sk-test-abc123".len());
        std::fs::remove_dir_all(&vault).ok();
    }

    #[test]
    fn loads_key_from_env_var_as_fallback() {
        let vault = fresh_vault();
        // SAFETY: tests share env; use a unique var name per test.
        let var = format!("AURA_TEST_KEY_{}", uuid::Uuid::now_v7().simple());
        std::env::set_var(&var, "sk-env-fallback-xyz");
        let key = load_named_key(&vault, "missing", &var).unwrap();
        std::env::remove_var(&var);
        assert_eq!(key.len(), "sk-env-fallback-xyz".len());
        std::fs::remove_dir_all(&vault).ok();
    }

    #[test]
    fn missing_file_and_env_returns_not_found() {
        let vault = fresh_vault();
        let var = format!("AURA_TEST_KEY_{}", uuid::Uuid::now_v7().simple());
        let err = load_named_key(&vault, "missing", &var).unwrap_err();
        assert!(matches!(err, KeyError::NotFound));
        std::fs::remove_dir_all(&vault).ok();
    }

    #[test]
    fn debug_impl_never_prints_value() {
        let key = ApiKey::from_string("sk-secret-value-do-not-leak");
        let s = format!("{:?}", key);
        assert!(!s.contains("sk-secret"), "Debug leaked key: {s}");
        assert!(!s.contains("do-not-leak"), "Debug leaked key: {s}");
        assert!(s.contains("redacted"), "Debug should say 'redacted': {s}");
    }

    #[test]
    fn display_impl_never_prints_value() {
        let key = ApiKey::from_string("sk-secret-value");
        let s = format!("{}", key);
        assert!(!s.contains("sk-secret"));
        assert_eq!(s, "<redacted>");
    }

    /// Even with `{:#?}` (alternate / pretty Debug), the key must not
    /// appear. Catches code that does `let _ = format!("{:#?}", err)`
    /// on an error that captured the key.
    #[test]
    fn pretty_debug_also_redacts() {
        let key = ApiKey::from_string("sk-leak-target-xyz");
        let s = format!("{:#?}", key);
        assert!(!s.contains("sk-leak"), "pretty-debug leaked: {s}");
    }

    /// Empty-key file is a hard error so a `touch ~/.aura/secrets/anthropic.key`
    /// accident doesn't masquerade as "key loaded".
    #[test]
    fn empty_key_file_returns_err_not_empty_key() {
        let vault = fresh_vault();
        write_key(&vault, "anthropic", "");
        let err = load_anthropic_key(&vault).unwrap_err();
        assert!(matches!(err, KeyError::Empty(_)));
        std::fs::remove_dir_all(&vault).ok();
    }
}
