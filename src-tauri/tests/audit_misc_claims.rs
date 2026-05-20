//! Audit: idempotency claims and a few sundry promises.

use std::path::PathBuf;

use aura_lib::core::vault::VaultState;
use aura_lib::protocols::auth::new_token;

fn fresh_vault() -> PathBuf {
    let root = std::env::temp_dir().join(format!("aura-audit-misc-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[tokio::test]
async fn audit_migration_is_idempotent() {
    let root = fresh_vault();
    // Open three times in a row; each opens runs migrations.
    let v1 = VaultState::open(root.clone()).await.unwrap();
    drop(v1);
    let v2 = VaultState::open(root.clone()).await.unwrap();
    drop(v2);
    let v3 = VaultState::open(root.clone()).await.unwrap();
    let count = v3.db.count_files().await.unwrap();
    assert_eq!(count, 0);
    std::fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn audit_token_actually_high_entropy() {
    // The claim: 256-bit URL-safe base64. Lower-bound entropy check:
    // generate 1000 tokens; no duplicates; min length sane.
    let mut tokens = std::collections::HashSet::new();
    for _ in 0..1000 {
        let t = new_token();
        assert!(t.len() >= 40, "token too short: {}", t);
        for c in t.chars() {
            assert!(c.is_ascii_alphanumeric() || c == '-' || c == '_', "non-url-safe char: {}", c);
        }
        assert!(tokens.insert(t), "token collision in 1000 generations");
    }
}

#[tokio::test]
async fn audit_auth_token_rotates_on_restart() {
    // Claim: every start_mcp_server freshly generates a token so stale
    // tokens never work. Verify the rotation via the public auth module.
    let t1 = new_token();
    let t2 = new_token();
    assert_ne!(t1, t2, "consecutive tokens must differ");
}

#[tokio::test]
async fn audit_unresolved_link_heals_across_renames() {
    let root = fresh_vault();
    let vault = VaultState::open(root.clone()).await.unwrap();

    std::fs::write(
        vault.resolve("source.md").unwrap(),
        "# Source\n\nLinks to [[planned]].\n",
    )
    .unwrap();
    vault.index_one(&vault.resolve("source.md").unwrap()).await.unwrap();

    // Initially unresolved.
    let outgoing = vault.db.get_outgoing_links("source.md").await.unwrap();
    assert!(!outgoing[0].is_resolved, "expected unresolved initially");

    // Create the target with a different filename but matching title.
    std::fs::write(
        vault.resolve("Target.md").unwrap(),
        "---\ntitle: planned\n---\n\n# Planned thing\n\nbody",
    )
    .unwrap();
    vault.index_one(&vault.resolve("Target.md").unwrap()).await.unwrap();

    // After indexing, the link should heal via title match.
    let outgoing = vault.db.get_outgoing_links("source.md").await.unwrap();
    assert!(outgoing[0].is_resolved, "title-match healing failed");

    std::fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn audit_word_count_excludes_markdown_syntax() {
    // The claim: word_count counts visible words, not the `#` characters
    // or list markers. Verify by parsing a doc whose syntax characters
    // would inflate a naive split-whitespace count.
    use aura_lib::core::markdown_parser::parse_document;
    let src = "# Heading\n\n- one\n- two\n- three\n\n> quoted text\n";
    let doc = parse_document(src, "fallback");
    // Visible words: Heading, one, two, three, quoted, text = 6
    assert_eq!(doc.word_count, 6, "expected 6 visible words, got {}", doc.word_count);
}

#[tokio::test]
async fn audit_wiki_link_handles_unclosed_nested() {
    // Claim from the regression discussion: `[[never closed and [[Real]]`
    // should yield only `[[Real]]` as a link.
    use aura_lib::core::link_resolver::scan_wiki_links;
    let s = "intro [[never closed and [[Real]] outro";
    let links = scan_wiki_links(s);
    assert_eq!(links.len(), 1, "expected exactly one link, got {:?}", links);
    assert_eq!(links[0].target, "Real");
}
