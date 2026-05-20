//! Phase 3 coverage: embed resolution for `![[file]]`, `![[file#Heading]]`,
//! `![[file#^anchor]]` via the parser helpers (the Tauri command itself is a
//! thin wrapper over these and over `VaultDb::resolve_link_target`).

use std::fs;
use std::path::PathBuf;

use aura_lib::core::markdown_parser::{
    extract_block_by_user_ref, extract_section_by_heading,
};
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
async fn embed_returns_section_under_heading_from_real_vault() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    let roadmap = vault.resolve("Projects/Aura Roadmap.md").unwrap();
    let content = fs::read_to_string(&roadmap).unwrap();
    let section = extract_section_by_heading(&content, "Phase 1 — Foundation").unwrap();
    assert!(section.starts_with("## Phase 1 — Foundation"));
    assert!(section.contains("Tauri 2"));
    assert!(!section.contains("## Phase 2"));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn embed_returns_block_for_user_anchor() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();

    let target_rel = "EmbedTarget.md";
    let target_abs = vault.resolve(target_rel).unwrap();
    fs::write(
        &target_abs,
        "# Target\n\nThis is the referenced paragraph.\n^anchor1\n\nAnother.",
    )
    .unwrap();

    let content = fs::read_to_string(&target_abs).unwrap();
    let block = extract_block_by_user_ref(&content, "anchor1").unwrap();
    assert!(block.contains("This is the referenced paragraph."));
    assert!(!block.contains("Another."));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn embed_marker_is_distinguished_from_plain_wiki_link() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // The fixture's Welcome.md uses `![[Projects/Aura Roadmap#Phase 1 — Foundation]]`
    // (embed) alongside `[[Daily/2026-05-18]]` (regular).
    let outgoing = vault
        .db
        .get_outgoing_links("Welcome.md")
        .await
        .unwrap();
    assert!(outgoing.iter().any(|l| l.is_resolved
        && l.target_path.as_deref() == Some("Projects/Aura Roadmap.md")
        && l.target_heading.as_deref() == Some("Phase 1 — Foundation")));

    fs::remove_dir_all(&root).ok();
}
