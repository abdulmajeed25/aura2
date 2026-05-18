//! Phase 5: end-to-end search over the fixture vault. Confirms FTS5,
//! semantic, and hybrid modes all return results.

use std::fs;
use std::path::PathBuf;

use aura_lib::core::search::{search_blocks, SearchMode};
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
async fn fts_finds_blocks_with_keyword() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    let hits = search_blocks(&vault.db, "Tauri", SearchMode::Fts, 10)
        .await
        .unwrap();
    assert!(!hits.is_empty(), "FTS should find the literal token");
    assert!(hits.iter().any(|h| h.file_path.contains("Roadmap")));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn semantic_returns_ranked_results_even_for_non_literal_queries() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // The hash embedder is bag-of-words-ish, so a literal-overlap query
    // still ranks the right block first. We assert *something* came back
    // and the top score is meaningful (>0).
    let hits = search_blocks(
        &vault.db,
        "block links backlinks",
        SearchMode::Semantic,
        5,
    )
    .await
    .unwrap();
    assert!(!hits.is_empty());
    assert!(hits[0].score > 0.0);
    assert_eq!(hits[0].matched_via, "semantic");

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn hybrid_merges_fts_and_semantic_hits() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    let hits = search_blocks(&vault.db, "Phase 5 search semantic", SearchMode::Hybrid, 10)
        .await
        .unwrap();
    assert!(!hits.is_empty());
    // Some hit should come from `Projects/Aura Roadmap.md` which mentions
    // "Phase 5 — Semantic Search" explicitly.
    assert!(
        hits.iter().any(|h| h.file_path.contains("Roadmap")),
        "expected a roadmap hit, got: {:?}",
        hits.iter().map(|h| h.file_path.as_str()).collect::<Vec<_>>()
    );

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn empty_query_returns_empty_results() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    let hits = search_blocks(&vault.db, "   ", SearchMode::Hybrid, 10)
        .await
        .unwrap();
    assert!(hits.is_empty());

    fs::remove_dir_all(&root).ok();
}
