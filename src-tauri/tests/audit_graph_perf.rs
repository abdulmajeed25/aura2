//! Audit: time the Rust force-directed layout at 100, 500, 1000, 2000 nodes.
//! The frontend claim is "Canvas 2D handles ≤ ~2k nodes at 60 FPS" — but
//! the layout itself has to fit in a frame budget too (16.6ms for 60 FPS).
//! At O(n²) per iter, 2k nodes × 250 iter = 10⁹ ops per layout. Let's see.

use std::time::Instant;

use aura_lib::core::graph_engine::{compute_graph, LayoutParams};
use aura_lib::core::vault::VaultState;

#[tokio::test]
async fn audit_layout_time_scales() {
    let root = std::env::temp_dir().join(format!("aura-audit-perf-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();
    let vault = VaultState::open(root.clone()).await.unwrap();

    for n in [100usize, 500, 1000, 2000] {
        // Create N notes, link each to its successor in a chain.
        for i in 0..n {
            let path = format!("note-{:05}.md", i);
            let next = format!("note-{:05}", (i + 1) % n);
            let content = format!("# Note {}\n\nlinks to [[{}]].\n", i, next);
            std::fs::write(vault.resolve(&path).unwrap(), content).unwrap();
        }
        vault.reindex().await.unwrap();

        let params = LayoutParams {
            iterations: 50, // representative single-frame budget
            width: 2000.0,
            height: 2000.0,
            seed: 7,
        };
        let t0 = Instant::now();
        let snap = compute_graph(&vault.db, params).await.unwrap();
        let elapsed = t0.elapsed();
        println!(
            "audit perf: n={} nodes, edges={}, iter=50, layout took {:?}",
            n,
            snap.edges.len(),
            elapsed
        );

        // Clean up notes so the next iteration is fresh.
        for i in 0..n {
            let path = format!("note-{:05}.md", i);
            let abs = vault.resolve(&path).unwrap();
            std::fs::remove_file(abs).ok();
        }
        vault.reindex().await.ok();
    }

    std::fs::remove_dir_all(&root).ok();
}
