//! Audit: does the Phase 5 "semantic search finds notes by meaning even
//! without keyword overlap" claim actually hold for the shipped hash
//! embedder? Setup: two notes share concept ("productivity") but use
//! completely different vocabulary. Query for one's vocabulary; check
//! whether the other ranks.

use std::fs;
use std::path::PathBuf;

use aura_lib::core::search::{search_blocks, SearchMode};
use aura_lib::core::vault::VaultState;

fn fresh_vault() -> PathBuf {
    let root = std::env::temp_dir().join(format!("aura-audit-sem-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    root
}

#[tokio::test]
async fn audit_semantic_without_keyword_overlap() {
    let root = fresh_vault();
    let vault = VaultState::open(root.clone()).await.unwrap();

    // Note A: productivity concept, vocabulary set 1
    fs::write(
        vault.resolve("note-a.md").unwrap(),
        "# Morning routine\n\nWaking at five-thirty, deep work blocks of \
         ninety minutes, no phone until lunch. Calendar holds my attention.",
    )
    .unwrap();

    // Note B: productivity concept, vocabulary set 2 (NO word overlap with A
    // other than possibly stop-words)
    fs::write(
        vault.resolve("note-b.md").unwrap(),
        "# Getting things done\n\nMy weekly review captures every loose \
         commitment. Tasks live in trusted lists, not in memory.",
    )
    .unwrap();

    // Note C: completely unrelated topic
    fs::write(
        vault.resolve("note-c.md").unwrap(),
        "# Rust async\n\nTokio's executor multiplexes futures onto worker \
         threads. The reactor wakes pending I/O via epoll.",
    )
    .unwrap();

    vault.reindex().await.unwrap();

    // Query using A's distinctive vocabulary. Real semantic search would
    // surface B (same concept) before C (unrelated).
    let hits = search_blocks(
        &vault.db,
        "morning routine deep work calendar",
        SearchMode::Semantic,
        10,
    )
    .await
    .unwrap();

    let positions: std::collections::HashMap<String, usize> = hits
        .iter()
        .enumerate()
        .map(|(i, h)| (h.file_path.clone(), i))
        .collect();
    let pos_a = positions.get("note-a.md").copied();
    let pos_b = positions.get("note-b.md").copied();
    let pos_c = positions.get("note-c.md").copied();

    println!(
        "audit semantic: query='morning routine deep work calendar' -> {:?}",
        hits.iter()
            .map(|h| (h.file_path.as_str(), h.score))
            .collect::<Vec<_>>()
    );
    println!(
        "positions: a={:?}, b(same-concept-no-overlap)={:?}, c(unrelated)={:?}",
        pos_a, pos_b, pos_c
    );

    fs::remove_dir_all(&root).ok();

    // The acid test: does B (concept match, no overlap) rank above C
    // (unrelated)? If hash-feature embedding genuinely captures semantics,
    // yes. If not, this fails.
    match (pos_b, pos_c) {
        (Some(b), Some(c)) => assert!(
            b < c,
            "SEMANTIC CLAIM FAILED: same-concept note (b={}) ranked below unrelated (c={})",
            b,
            c
        ),
        (None, Some(_)) => panic!("SEMANTIC CLAIM FAILED: same-concept note B did not surface at all"),
        (Some(_), None) => {} // only B, no C — actually fine; B beat C by absence
        (None, None) => panic!("SEMANTIC CLAIM FAILED: neither B nor C surfaced"),
    }
}
