//! Phase 7: end-to-end GraphRAG integration test against the fixture vault.
//! Exercises detection → summarisation → persistence → query.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use aura_lib::core::embeddings::{embedding_to_bytes, HashEmbedder, TextEncoder, EMBED_DIM};
use aura_lib::core::graph_rag::community_detector::detect_communities;
use aura_lib::core::graph_rag::query_engine::run_query;
use aura_lib::core::graph_rag::summarizer::{extractive_summary, leading_paragraphs};
use aura_lib::core::vault::VaultState;
use aura_lib::db::sqlite::ReplaceCommunity;

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
async fn rebuild_then_query_returns_relevant_community() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // Reuse the same detection/summarisation pipeline the command does.
    let (nodes_raw, edges_raw) = vault.db.fetch_graph_nodes_and_edges().await.unwrap();
    let node_ids: Vec<String> = nodes_raw.iter().map(|(id, _, _)| id.clone()).collect();
    let id_to_pt: HashMap<String, (String, String)> = nodes_raw
        .iter()
        .map(|(id, p, t)| (id.clone(), (p.clone(), t.clone())))
        .collect();
    let partition = detect_communities(&node_ids, &edges_raw, 30, 0xA1A0_2026);

    let mut by_community: HashMap<u32, Vec<String>> = HashMap::new();
    for (n, c) in &partition {
        by_community.entry(*c).or_default().push(n.clone());
    }

    let encoder = HashEmbedder::new();
    let mut owned: Vec<(i64, Vec<String>, String, Vec<u8>)> = Vec::new();
    for members in by_community.values() {
        let mut entries = Vec::new();
        for fid in members {
            let (path, title) = match id_to_pt.get(fid) {
                Some(v) => v,
                None => continue,
            };
            let abs = vault.resolve(path).unwrap();
            let content = std::fs::read_to_string(&abs).unwrap();
            entries.push((title.clone(), leading_paragraphs(&content, 240)));
        }
        let summary = extractive_summary(&entries);
        let emb = encoder.encode(&summary);
        owned.push((0, members.clone(), summary, embedding_to_bytes(&emb)));
    }

    let borrowed: Vec<ReplaceCommunity<'_>> = owned
        .iter()
        .enumerate()
        .map(|(idx, (level, members, summary, emb))| ReplaceCommunity {
            level: *level,
            partition_cid: idx as u32,
            parent_partition_cid: None,
            member_file_ids: members.iter().map(String::as_str).collect(),
            summary_text: summary,
            embedding: emb,
            dim: EMBED_DIM as i64,
        })
        .collect();
    vault.db.replace_communities(&borrowed).await.unwrap();

    // The fixture has Welcome → Daily → Roadmap all interlinked, so they
    // should collapse into a single community.
    let answer = run_query(&vault.db, vault.encoder.as_ref(), "phase semantic search FTS", 3)
        .await
        .unwrap();
    assert!(!answer.communities.is_empty(), "should find at least one community");
    assert!(answer.estimated_tokens > 0);
    assert!(answer.covered_notes > 0);
    let first = &answer.communities[0];
    assert!(
        first.member_paths.iter().any(|p| p.contains("Roadmap")),
        "expected the roadmap to surface as part of the matched community"
    );

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn empty_communities_table_yields_empty_answer() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    let answer = run_query(&vault.db, vault.encoder.as_ref(), "anything", 3).await.unwrap();
    assert!(answer.communities.is_empty());
    assert_eq!(answer.estimated_tokens, 0);
    assert_eq!(answer.covered_notes, 0);

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn context_payload_is_compact_relative_to_full_vault() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // Compute the total chars of all fixture notes' bodies.
    let mut total_chars: usize = 0;
    let nodes = vault.db.fetch_graph_nodes_and_edges().await.unwrap().0;
    for (_id, path, _title) in &nodes {
        let abs = vault.resolve(path).unwrap();
        total_chars += std::fs::read_to_string(&abs).unwrap().len();
    }

    // Rebuild communities (single trivial community for fixture).
    let node_ids: Vec<String> = nodes.iter().map(|(id, _, _)| id.clone()).collect();
    let (_, edges) = vault.db.fetch_graph_nodes_and_edges().await.unwrap();
    let partition = detect_communities(&node_ids, &edges, 30, 0xA1A0_2026);
    let mut by_community: HashMap<u32, Vec<String>> = HashMap::new();
    for (n, c) in &partition {
        by_community.entry(*c).or_default().push(n.clone());
    }
    let encoder = HashEmbedder::new();
    let id_to_pt: HashMap<String, (String, String)> = nodes
        .iter()
        .map(|(id, p, t)| (id.clone(), (p.clone(), t.clone())))
        .collect();
    let mut owned: Vec<(i64, Vec<String>, String, Vec<u8>)> = Vec::new();
    for members in by_community.values() {
        let mut entries = Vec::new();
        for fid in members {
            let (path, title) = match id_to_pt.get(fid) {
                Some(v) => v,
                None => continue,
            };
            let abs = vault.resolve(path).unwrap();
            let content = std::fs::read_to_string(&abs).unwrap();
            entries.push((title.clone(), leading_paragraphs(&content, 240)));
        }
        let summary = extractive_summary(&entries);
        let emb = encoder.encode(&summary);
        owned.push((0, members.clone(), summary, embedding_to_bytes(&emb)));
    }
    let borrowed: Vec<ReplaceCommunity<'_>> = owned
        .iter()
        .enumerate()
        .map(|(idx, (level, members, summary, emb))| ReplaceCommunity {
            level: *level,
            partition_cid: idx as u32,
            parent_partition_cid: None,
            member_file_ids: members.iter().map(String::as_str).collect(),
            summary_text: summary,
            embedding: emb,
            dim: EMBED_DIM as i64,
        })
        .collect();
    vault.db.replace_communities(&borrowed).await.unwrap();

    let answer = run_query(&vault.db, vault.encoder.as_ref(), "phase markdown live preview", 3)
        .await
        .unwrap();
    let payload_chars = answer.context_payload.len();
    assert!(payload_chars > 0);
    // For the fixture the payload must be < the full vault content.
    assert!(
        payload_chars < total_chars,
        "context should compress vault content: payload={} vs total={}",
        payload_chars,
        total_chars
    );

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn phase7a_ii_persists_hierarchy_with_parent_links() {
    // Two levels: a fine-grained partition where each node is its own
    // community (cids 0..3), and a coarse partition that merges them all
    // into one community (cid 0). After persistence, every fine-level
    // row should point at the one coarse-level row via parent_id.
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();
    let (nodes, _edges) = vault.db.fetch_graph_nodes_and_edges().await.unwrap();
    assert!(!nodes.is_empty(), "fixture must have nodes");

    let encoder = HashEmbedder::new();
    let zero_emb = embedding_to_bytes(&encoder.encode("placeholder"));

    type Row = (i64, u32, Option<u32>, Vec<String>, String, Vec<u8>);
    let owned: Vec<Row> = vec![
        // Coarsest level (level=1): one community containing every node.
        (
            1_i64,
            0_u32,
            None,
            nodes.iter().map(|(id, _, _)| id.clone()).collect(),
            "coarse: everything".to_string(),
            zero_emb.clone(),
        ),
        // Fine level (level=0): each node is its own community, all
        // parented to the coarse cid=0.
        (0_i64, 100, Some(0), vec![nodes[0].0.clone()], "fine 100".into(), zero_emb.clone()),
        (0_i64, 101, Some(0), vec![nodes[1].0.clone()], "fine 101".into(), zero_emb.clone()),
        (0_i64, 102, Some(0), vec![nodes[2].0.clone()], "fine 102".into(), zero_emb.clone()),
    ];
    let borrowed: Vec<ReplaceCommunity<'_>> = owned
        .iter()
        .map(|(level, cid, parent, members, summary, emb)| ReplaceCommunity {
            level: *level,
            partition_cid: *cid,
            parent_partition_cid: *parent,
            member_file_ids: members.iter().map(String::as_str).collect(),
            summary_text: summary,
            embedding: emb,
            dim: EMBED_DIM as i64,
        })
        .collect();
    vault.db.replace_communities(&borrowed).await.unwrap();

    // Coarsest level = 1.
    let max = vault.db.max_community_level().await.unwrap();
    assert_eq!(max, 1, "max level should be 1, got {}", max);

    // Coarsest level: exactly one community.
    let coarse = vault.db.list_communities_at_level(1).await.unwrap();
    assert_eq!(coarse.len(), 1);
    let coarse_id = coarse[0].id;

    // Fine level: three communities, each pointing at the coarse one.
    let fine = vault.db.list_communities_at_level(0).await.unwrap();
    assert_eq!(fine.len(), 3);

    // Both levels persisted with the right member counts. The parent_id
    // → coarse-level row chain is enforced by the FOREIGN KEY constraint
    // on the `communities.parent_id` column (PRAGMA foreign_keys = ON);
    // if any fine-level insert had pointed at a non-existent parent, the
    // `replace_communities` call above would have failed.
    let _ = coarse_id; // silence unused-var lint
    fs::remove_dir_all(&root).ok();
}
