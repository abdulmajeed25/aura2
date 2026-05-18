//! Phase 11: agent-suggestion + orphan-detection end-to-end test.

use std::fs;
use std::path::PathBuf;

use aura_lib::core::agent::optimization::find_orphans;
use aura_lib::core::agent::suggestions::{compute_suggestions, SuggestParams};
use aura_lib::core::vault::VaultState;
use uuid::Uuid;

fn copy_fixture_to_temp() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest
        .parent()
        .unwrap()
        .join("tests/fixtures/sample-vault");
    let temp = std::env::temp_dir().join(format!("aura-test-{}", Uuid::now_v7()));
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
async fn find_orphans_returns_truly_disconnected_notes() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // No orphans in the fixture — all three notes are interlinked.
    let orphans = find_orphans(&vault.db).await.unwrap();
    assert!(
        orphans.is_empty(),
        "fixture should have no orphans, got {:?}",
        orphans
    );

    // Add an explicit orphan and assert it surfaces.
    let abs = vault.resolve("Orphan.md").unwrap();
    fs::write(&abs, "# Orphan\n\nNobody links to me.\n").unwrap();
    vault.index_one(&abs).await.unwrap();

    let orphans = find_orphans(&vault.db).await.unwrap();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].path, "Orphan.md");

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn suggest_links_proposes_unlinked_related_notes() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // Add two extra notes that share vocabulary with the existing roadmap
    // but don't link to it yet.
    let a_path = "Notes/phase-discussion-1.md";
    let b_path = "Notes/phase-discussion-2.md";
    let a_abs = vault.resolve(a_path).unwrap();
    let b_abs = vault.resolve(b_path).unwrap();
    fs::create_dir_all(a_abs.parent().unwrap()).unwrap();
    fs::write(
        &a_abs,
        "# Phase 5 thoughts\n\nSemantic search FTS hybrid markdown indexing.\n",
    )
    .unwrap();
    fs::write(
        &b_abs,
        "# Phase 4 thoughts\n\nGraph view force-directed layout rust.\n",
    )
    .unwrap();
    vault.index_one(&a_abs).await.unwrap();
    vault.index_one(&b_abs).await.unwrap();

    let params = SuggestParams {
        min_score: 0.0,
        limit_per_source: 5,
        total_limit: 50,
    };
    let suggs = compute_suggestions(&vault.db, params).await.unwrap();

    assert!(!suggs.is_empty(), "expected at least one suggestion");
    // The discussion notes should appear as source candidates whose target
    // is some other note in the vault (anything not already linked).
    assert!(
        suggs.iter().any(|s| s.source_path.contains("phase-discussion")),
        "phase-discussion notes should appear as suggestion sources"
    );
    // Suggestions should always be sorted descending by score.
    for win in suggs.windows(2) {
        assert!(
            win[0].score >= win[1].score,
            "suggestions must be sorted by score desc"
        );
    }

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn suggestions_skip_already_linked_pairs() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // The fixture's Daily/2026-05-18.md → Welcome.md is an existing link.
    // Suggestions must never re-propose that pair.
    let params = SuggestParams {
        min_score: 0.0,
        limit_per_source: 20,
        total_limit: 100,
    };
    let suggs = compute_suggestions(&vault.db, params).await.unwrap();
    for s in &suggs {
        let pair_existing = s.source_path == "Daily/2026-05-18.md"
            && s.target_path == "Welcome.md";
        assert!(
            !pair_existing,
            "suggestion duplicates an existing link: {:?}",
            s
        );
    }

    fs::remove_dir_all(&root).ok();
}
