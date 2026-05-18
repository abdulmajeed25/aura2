//! Phase 4 coverage: build a positioned snapshot from the fixture vault and
//! assert the nodes/edges line up with the link table.

use std::fs;
use std::path::PathBuf;

use aura_lib::core::graph_engine::{compute_graph, LayoutParams};
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
async fn fixture_graph_contains_all_files_and_resolved_edges() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    let params = LayoutParams {
        iterations: 60,
        width: 800.0,
        height: 800.0,
        seed: 11,
    };
    let snap = compute_graph(&vault.db, params).await.unwrap();

    // All three fixture files become nodes.
    assert_eq!(snap.nodes.len(), 3);
    let paths: Vec<_> = snap.nodes.iter().map(|n| n.path.as_str()).collect();
    assert!(paths.contains(&"Welcome.md"));
    assert!(paths.contains(&"Daily/2026-05-18.md"));
    assert!(paths.contains(&"Projects/Aura Roadmap.md"));

    // Daily links to Welcome and to Roadmap; Welcome embeds Roadmap.
    // That's 3 directed edges, deduped to 3 undirected pairs:
    //   Daily-Welcome, Daily-Roadmap, Welcome-Roadmap
    assert_eq!(snap.edges.len(), 3);

    // Layout must produce finite positions inside the frame.
    for n in &snap.nodes {
        assert!(n.x.is_finite() && n.y.is_finite());
        assert!(n.x.abs() <= 400.0 + 1e-3 && n.y.abs() <= 400.0 + 1e-3);
    }

    // Degree must reflect the dedup.
    for n in &snap.nodes {
        assert_eq!(n.degree, 2, "every fixture node connects to two others");
    }

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn graph_includes_isolated_files_as_floating_nodes() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // Add an orphan with no incoming/outgoing links.
    let orphan = vault.resolve("Orphan.md").unwrap();
    fs::write(&orphan, "# Orphan\n\nLeaving nobody nodding.\n").unwrap();
    vault.index_one(&orphan).await.unwrap();

    let snap = compute_graph(
        &vault.db,
        LayoutParams {
            iterations: 30,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(snap.nodes.len(), 4);
    let orphan_node = snap.nodes.iter().find(|n| n.path == "Orphan.md").unwrap();
    assert_eq!(orphan_node.degree, 0);

    fs::remove_dir_all(&root).ok();
}
