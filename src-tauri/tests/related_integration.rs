//! Phase 6: HDC-based related-notes integration test.
//!
//! Verifies that (a) the text HVs are populated by indexing and (b) a note
//! sharing both vocabulary and neighbours with another scores higher than
//! an unrelated note.

use std::fs;
use std::path::PathBuf;

use aura_lib::core::hdc::encoder::encode_text;
use aura_lib::core::hdc::graph_encoder::encode_note_combined;
use aura_lib::core::hdc::{Hypervector, HV_DIM};
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
async fn indexing_populates_text_hypervectors_for_every_file() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    let rows = vault.db.all_note_text_hvs().await.unwrap();
    assert_eq!(rows.len(), 3, "fixture has three notes, all should be encoded");
    for (_id, _path, _title, dim, packed) in &rows {
        assert_eq!(*dim, HV_DIM as i64);
        assert_eq!(packed.len(), HV_DIM / 8, "10k bipolar dims pack into 1250 bytes");
    }

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn shared_neighbours_pull_combined_hvs_together() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // Add two notes that both link to the same two existing notes but use
    // completely different vocabularies. HDC should still mark them related.
    let n1_rel = "Cluster/Member-A.md";
    let n2_rel = "Cluster/Member-B.md";
    let n3_rel = "Isolated.md";

    let n1 = vault.resolve(n1_rel).unwrap();
    fs::create_dir_all(n1.parent().unwrap()).unwrap();
    fs::write(
        &n1,
        "# Member A\n\napple banana cherry date eggplant.\n\nSee [[Welcome]] and [[Daily/2026-05-18]].\n",
    )
    .unwrap();
    vault.index_one(&n1).await.unwrap();

    let n2 = vault.resolve(n2_rel).unwrap();
    fs::write(
        &n2,
        "# Member B\n\nxylophone yodel zucchini.\n\nReferences [[Welcome]] and [[Daily/2026-05-18]].\n",
    )
    .unwrap();
    vault.index_one(&n2).await.unwrap();

    let n3 = vault.resolve(n3_rel).unwrap();
    fs::write(&n3, "# Isolated\n\nNo overlap and no links.\n").unwrap();
    vault.index_one(&n3).await.unwrap();

    // Pull the snapshot the command would compute on.
    let neighbours = vault.db.neighbour_map().await.unwrap();
    let hvs: std::collections::HashMap<String, (String, Hypervector)> = vault
        .db
        .all_note_text_hvs()
        .await
        .unwrap()
        .into_iter()
        .map(|(id, path, _title, _dim, packed)| {
            (id, (path, Hypervector::from_packed_bytes(&packed, HV_DIM)))
        })
        .collect();

    let id_for = |path: &str| -> String {
        hvs.iter()
            .find_map(|(id, (p, _))| if p == path { Some(id.clone()) } else { None })
            .unwrap_or_else(|| panic!("missing {}", path))
    };

    let id_a = id_for(n1_rel);
    let id_b = id_for(n2_rel);
    let id_iso = id_for(n3_rel);

    let combine = |id: &str| -> Hypervector {
        let (_path, text_hv) = hvs.get(id).unwrap();
        let n = neighbours.get(id).cloned().unwrap_or_default();
        encode_note_combined(text_hv, &n.0, &n.1)
    };

    let sim_ab = combine(&id_a).similarity(&combine(&id_b));
    let sim_a_iso = combine(&id_a).similarity(&combine(&id_iso));
    assert!(
        sim_ab > sim_a_iso,
        "HDC should rank co-linked notes above isolated ones: ab={} iso={}",
        sim_ab,
        sim_a_iso
    );

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn text_hv_roundtrip_matches_freshly_encoded() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    let stored = vault.db.all_note_text_hvs().await.unwrap();
    let row = stored
        .iter()
        .find(|r| r.1 == "Welcome.md")
        .expect("welcome row");
    let unpacked = Hypervector::from_packed_bytes(&row.4, HV_DIM);

    let abs = vault.resolve("Welcome.md").unwrap();
    let content = std::fs::read_to_string(&abs).unwrap();
    let fresh = encode_text(&content);

    assert_eq!(unpacked, fresh, "stored HV must equal freshly-encoded HV");

    fs::remove_dir_all(&root).ok();
}
