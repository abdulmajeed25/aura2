//! Audit: hammer VaultState::resolve with adversarial inputs the existing
//! tests don't cover.
//!
//! v5.0 Phase 1 closed the path-safety gaps the v3 audit documented:
//! null-byte rejection, bare `.` rejection, and symlink canonicalisation.

use aura_lib::core::vault::VaultState;
use aura_lib::utils::error::AuraError;

#[tokio::test]
async fn audit_path_traversal_attacks() {
    let root = std::env::temp_dir().join(format!("aura-audit-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();
    let vault = VaultState::open(root.clone()).await.unwrap();

    let cases: &[(&str, bool, &str)] = &[
        // expected: false = should be rejected
        ("normal.md", true, "plain relative"),
        ("dir/sub.md", true, "nested relative"),
        ("../escape.md", false, "single .."),
        ("..", false, "bare .."),
        ("../../etc/passwd", false, "deep .."),
        ("/etc/passwd", false, "absolute unix"),
        ("/", false, "root only"),
        ("\\windows\\system32", false, "absolute windows"),
        // NB: `C:\Users` on Unix is a legitimate (if weird) filename — backslash
        // is not a separator on POSIX. v5.0 Phase 1 considers this a
        // portability hazard (not a security one); accept it.
        ("C:\\Users", true, "windows drive-syntax on unix = literal filename"),
        ("dir/../escape", false, "mid-path .."),
        ("a/../b", false, "mid-path .. with valid prefix"),
        ("", false, "empty"),
        (".", false, "current dir alone"),
        ("./", false, "current dir with trailing slash"),
        ("a/./b", true, "current-dir component mid-path is a no-op"),
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

    assert!(
        surprises.is_empty(),
        "path-safety surprises:\n  {}",
        surprises.join("\n  ")
    );
}

#[tokio::test]
async fn audit_resolve_blocks_symlink_escape() {
    // Plant a symlink inside the vault pointing at `/etc` and try to read
    // through it. v5.0 Phase 1 hardening: resolve() must canonicalise and
    // reject paths whose canonical form lies outside the canonical vault root.
    #[cfg(unix)]
    {
        let root = std::env::temp_dir().join(format!("aura-audit-sym-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root).unwrap();
        let vault = VaultState::open(root.clone()).await.unwrap();

        // /etc/passwd exists on every Unix and is the canonical exfiltration
        // target for path-traversal exploits.
        if !std::path::Path::new("/etc/passwd").exists() {
            // skip if the host doesn't have it (very unusual)
            std::fs::remove_dir_all(&root).ok();
            return;
        }

        // Symlink chain: <vault>/outside -> /etc, then <vault>/outside/passwd
        // would land on /etc/passwd if the canonicaliser missed it.
        let link = root.join("outside");
        std::os::unix::fs::symlink("/etc", &link).unwrap();

        let outcome = vault.resolve("outside/passwd");
        std::fs::remove_dir_all(&root).ok();

        // The hardened resolve must refuse, with PathOutsideVault.
        match outcome {
            Err(AuraError::PathOutsideVault(_)) => {} // pass
            Err(other) => panic!(
                "expected PathOutsideVault, got different error: {:?}",
                other
            ),
            Ok(p) => panic!(
                "SYMLINK ESCAPE: resolve('outside/passwd') succeeded and returned {} — \
                 a planted symlink let a caller read /etc",
                p.display()
            ),
        }
    }
}

#[tokio::test]
async fn audit_resolve_accepts_real_file_through_inner_symlink() {
    // Counterpoint: a symlink that stays *inside* the vault must still resolve
    // successfully — symlinks are legitimate user-visible structure (Obsidian,
    // ln -s for media folders, etc.). Only escapes are rejected.
    #[cfg(unix)]
    {
        let root = std::env::temp_dir().join(format!("aura-audit-syminner-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root).unwrap();
        let target_dir = root.join("real");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("note.md"), "# inside\n\nbody").unwrap();

        let vault = VaultState::open(root.clone()).await.unwrap();
        std::os::unix::fs::symlink(&target_dir, root.join("alias")).unwrap();

        let outcome = vault.resolve("alias/note.md");
        std::fs::remove_dir_all(&root).ok();

        match outcome {
            Ok(_) => {} // pass — inner symlink is fine
            Err(e) => panic!(
                "inner symlink unjustly rejected: {:?} (must allow symlinks that \
                 resolve inside the vault)",
                e
            ),
        }
    }
}
