//! Integration coverage for Phase 2: block extraction, link resolution,
//! backlinks, outline, and unresolved-link healing.

use std::fs;
use std::path::PathBuf;

use aura_lib::core::vault::VaultState;

fn copy_fixture_to_temp() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest
        .parent()
        .unwrap()
        .join("tests/fixtures/sample-vault");
    let temp = std::env::temp_dir().join(format!("aura-test-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&temp).unwrap();
    copy_dir(&fixture, &temp).unwrap();
    temp
}

fn copy_dir(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn fixture_vault_resolves_wiki_links_to_existing_files() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    let report = vault.reindex().await.unwrap();
    assert_eq!(report.indexed, 3);

    // Daily/2026-05-18.md → [[Welcome]] AND [[Projects/Aura Roadmap]]
    let outgoing = vault
        .db
        .get_outgoing_links("Daily/2026-05-18.md")
        .await
        .unwrap();
    assert_eq!(outgoing.len(), 2);
    assert!(outgoing.iter().all(|l| l.is_resolved));
    assert!(outgoing
        .iter()
        .any(|l| l.target_path.as_deref() == Some("Welcome.md")));
    assert!(outgoing
        .iter()
        .any(|l| l.target_path.as_deref() == Some("Projects/Aura Roadmap.md")));

    // Backlinks: Welcome.md is linked from both Daily and Projects/...?
    // The fixture's Welcome.md has a forward link to Daily/2026-05-18, so
    // that file is a back-link source of Daily. We assert the reverse.
    let backlinks = vault.db.get_backlinks("Welcome.md").await.unwrap();
    assert!(backlinks.iter().any(|b| b.source_path == "Daily/2026-05-18.md"));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn unresolved_link_heals_when_target_is_created_later() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // Add a new note that links to a target that doesn't exist yet.
    let source_rel = "NewSource.md";
    let source_abs = vault.resolve(source_rel).unwrap();
    fs::write(&source_abs, "# Source\n\nReferences [[Ghost Note]] here.\n").unwrap();
    vault.index_one(&source_abs).await.unwrap();

    let outgoing_before = vault.db.get_outgoing_links(source_rel).await.unwrap();
    assert_eq!(outgoing_before.len(), 1);
    assert!(!outgoing_before[0].is_resolved);

    // Now create the target. The healing pass should pick it up.
    let target_rel = "Ghost Note.md";
    let target_abs = vault.resolve(target_rel).unwrap();
    fs::write(&target_abs, "# Ghost Note\n\nNow I exist.\n").unwrap();
    vault.index_one(&target_abs).await.unwrap();

    let outgoing_after = vault.db.get_outgoing_links(source_rel).await.unwrap();
    assert_eq!(outgoing_after.len(), 1);
    assert!(outgoing_after[0].is_resolved);
    assert_eq!(outgoing_after[0].target_path.as_deref(), Some("Ghost Note.md"));

    let backlinks = vault.db.get_backlinks("Ghost Note.md").await.unwrap();
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].source_path, "NewSource.md");

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn block_ref_resolves_to_target_block_id() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // Add a target file with a user-assigned block ref.
    let target_rel = "Target.md";
    let target_abs = vault.resolve(target_rel).unwrap();
    fs::write(
        &target_abs,
        "# Target\n\nThis is the referenced paragraph.\n^anchor1\n\nAnother paragraph.\n",
    )
    .unwrap();
    vault.index_one(&target_abs).await.unwrap();

    let source_rel = "Linker.md";
    let source_abs = vault.resolve(source_rel).unwrap();
    fs::write(
        &source_abs,
        "# Linker\n\nSee [[Target#^anchor1]] for the anchor.\n",
    )
    .unwrap();
    vault.index_one(&source_abs).await.unwrap();

    let outgoing = vault.db.get_outgoing_links(source_rel).await.unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].target_block_ref.as_deref(), Some("anchor1"));
    assert_eq!(outgoing[0].target_path.as_deref(), Some("Target.md"));
    assert!(outgoing[0].is_resolved);

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn list_link_candidates_returns_all_indexed_files() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    let candidates = vault.db.list_link_candidates().await.unwrap();
    assert_eq!(candidates.len(), 3);
    let paths: Vec<_> = candidates.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&"Welcome.md"));
    assert!(paths.contains(&"Daily/2026-05-18.md"));
    assert!(paths.contains(&"Projects/Aura Roadmap.md"));

    fs::remove_dir_all(&root).ok();
}
