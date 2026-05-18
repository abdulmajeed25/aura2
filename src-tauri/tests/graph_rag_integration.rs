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
        .map(|(level, members, summary, emb)| ReplaceCommunity {
            level: *level,
            member_file_ids: members.iter().map(String::as_str).collect(),
            summary_text: summary,
            embedding: emb,
            dim: EMBED_DIM as i64,
        })
        .collect();
    vault.db.replace_communities(&borrowed).await.unwrap();

    // The fixture has Welcome → Daily → Roadmap all interlinked, so they
    // should collapse into a single community.
    let answer = run_query(&vault.db, "phase semantic search FTS", 3)
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

    let answer = run_query(&vault.db, "anything", 3).await.unwrap();
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
        .map(|(level, members, summary, emb)| ReplaceCommunity {
            level: *level,
            member_file_ids: members.iter().map(String::as_str).collect(),
            summary_text: summary,
            embedding: emb,
            dim: EMBED_DIM as i64,
        })
        .collect();
    vault.db.replace_communities(&borrowed).await.unwrap();

    let answer = run_query(&vault.db, "phase markdown live preview", 3)
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
