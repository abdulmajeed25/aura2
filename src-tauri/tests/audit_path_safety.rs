//! Audit: hammer VaultState::resolve with adversarial inputs the existing
//! tests don't cover.

use std::path::PathBuf;

use aura_lib::core::vault::VaultState;

#[tokio::test]
async fn audit_path_traversal_attacks() {
    let root = std::env::temp_dir().join(format!("aura-audit-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();
    let vault = VaultState::open(root.clone()).await.unwrap();

    let cases: &[(&str, bool, &str)] = &[
        // expected: false = should be rejected
        ("normal.md", true, "plain relative"),
        ("dir/sub.md", true, "nested relative"),
        ("../escape.md", false, "single ..", ),
        ("..", false, "bare .."),
        ("../../etc/passwd", false, "deep .."),
        ("/etc/passwd", false, "absolute unix"),
        ("/", false, "root only"),
        ("\\windows\\system32", false, "absolute windows"),
        ("C:\\Users", false, "windows drive — currently NOT rejected on unix"),
        ("dir/../escape", false, "mid-path .."),
        ("a/../b", false, "mid-path .. with valid prefix"),
        ("", false, "empty"),
        (".", false, "current dir"),
        ("a/./b", true, "current-dir component should be allowed"),
        ("\0", false, "null byte"),
        ("a\0b", false, "embedded null byte"),
    ];

    let mut surprises = Vec::new();
    for (input, expected_ok, label) in cases {
        let actual_ok = vault.resolve(input).is_ok();
        if actual_ok != *expected_ok {
            surprises.push(format!(
                "{} ({:?}): expected ok={}, actual ok={}",
                label, input, expected_ok, actual_ok
            ));
        }
    }
    std::fs::remove_dir_all(&root).ok();

    if !surprises.is_empty() {
        panic!("path-safety surprises:\n  {}", surprises.join("\n  "));
    }
}

#[tokio::test]
async fn audit_resolve_symlink_traversal() {
    // Build a vault and place a symlink inside it pointing at /etc.
    // resolve() should still accept the relative path (symlinks are
    // user-visible inside the vault), but reading through them needs
    // careful auditing — at minimum we want resolve() to behave consistently.
    let root = std::env::temp_dir().join(format!("aura-audit-sym-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();
    let vault = VaultState::open(root.clone()).await.unwrap();

    // Symlink "outside" -> /etc inside the vault.
    let link = root.join("outside");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc", &link).ok();
    }

    // resolve treats it as a normal relative path, returns the symlink path.
    let resolved = vault.resolve("outside/passwd");
    // Don't panic — just record what happens.
    let outcome = match resolved {
        Ok(p) => format!("ok -> {}", p.display()),
        Err(e) => format!("err -> {}", e),
    };
    let exists = PathBuf::from("/etc/passwd").exists();
    println!(
        "symlink audit: resolve(outside/passwd) = {}; /etc/passwd exists = {}",
        outcome, exists
    );
    // We just want resolve to NOT crash. The fact that the vault would
    // happily read /etc/passwd via a planted symlink is a real concern
    // worth flagging in the audit report — VaultState doesn't canonicalize
    // after resolve, so symlinks escape its containment guarantees.

    std::fs::remove_dir_all(&root).ok();
}
