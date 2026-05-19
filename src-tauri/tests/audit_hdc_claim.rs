//! Audit: does `find_related` actually surface a same-neighbour note above
//! an unrelated note, using the public command path (not just the unit
//! test fixture)?

use std::fs;
use std::path::PathBuf;

use aura_lib::core::hdc::encoder::encode_text;
use aura_lib::core::hdc::graph_encoder::encode_note_combined;
use aura_lib::core::hdc::{Hypervector, HV_DIM};
use aura_lib::core::vault::VaultState;

fn fresh_vault() -> PathBuf {
    let root = std::env::temp_dir().join(format!("aura-audit-hdc-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    root
}

#[tokio::test]
async fn audit_find_related_ranks_cluster_above_orphan() {
    let root = fresh_vault();
    let vault = VaultState::open(root.clone()).await.unwrap();

    // Hub note that everything else points at.
    fs::write(
        vault.resolve("Hub.md").unwrap(),
        "# Hub\n\nCentral topic page.",
    )
    .unwrap();
    // Two cluster members — different vocabulary, both linking to Hub.
    fs::write(
        vault.resolve("ClusterA.md").unwrap(),
        "# Member A\n\nalpha bravo charlie. See [[Hub]].",
    )
    .unwrap();
    fs::write(
        vault.resolve("ClusterB.md").unwrap(),
        "# Member B\n\nxylophone yankee zulu. See [[Hub]].",
    )
    .unwrap();
    // Orphan with no links at all and disjoint vocabulary.
    fs::write(
        vault.resolve("Orphan.md").unwrap(),
        "# Orphan\n\nmango nectarine orange.",
    )
    .unwrap();

    vault.reindex().await.unwrap();

    // From the command path's perspective: build the combined HVs the
    // way `find_related` would, then check the ranking.
    let rows = vault.db.all_note_text_hvs().await.unwrap();
    let neighbours = vault.db.neighbour_map().await.unwrap();

    let by_path: std::collections::HashMap<String, (String, Hypervector)> = rows
        .into_iter()
        .map(|(id, path, _title, _dim, packed)| {
            (path, (id, Hypervector::from_packed_bytes(&packed, HV_DIM)))
        })
        .collect();

    let combine = |path: &str| -> Hypervector {
        let (id, text_hv) = by_path.get(path).expect("path not indexed");
        let n = neighbours.get(id).cloned().unwrap_or_default();
        encode_note_combined(text_hv, &n.0, &n.1)
    };

    let q = combine("ClusterA.md");
    let sim_b = q.similarity(&combine("ClusterB.md"));
    let sim_orphan = q.similarity(&combine("Orphan.md"));
    let sim_hub = q.similarity(&combine("Hub.md"));

    println!(
        "audit hdc: A-vs-B={:.4}, A-vs-Orphan={:.4}, A-vs-Hub={:.4}",
        sim_b, sim_orphan, sim_hub
    );

    fs::remove_dir_all(&root).ok();

    assert!(
        sim_b > sim_orphan,
        "HDC FAIL: cluster sibling sim={} not above orphan sim={}",
        sim_b,
        sim_orphan
    );
}

#[tokio::test]
async fn audit_text_hv_packing_is_lossless() {
    let root = fresh_vault();
    let vault = VaultState::open(root.clone()).await.unwrap();
    fs::write(
        vault.resolve("X.md").unwrap(),
        "# Test\n\nAny content. \u{1F600} unicode \u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}.",
    )
    .unwrap();
    vault.reindex().await.unwrap();

    let stored = vault.db.all_note_text_hvs().await.unwrap();
    let row = stored.iter().find(|r| r.1 == "X.md").unwrap();
    let unpacked = Hypervector::from_packed_bytes(&row.4, HV_DIM);
    let abs = vault.resolve("X.md").unwrap();
    let fresh = encode_text(&std::fs::read_to_string(&abs).unwrap());
    assert_eq!(unpacked, fresh, "stored HV != freshly-encoded HV");

    fs::remove_dir_all(&root).ok();
}
