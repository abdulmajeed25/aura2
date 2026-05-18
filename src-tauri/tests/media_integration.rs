//! Phase 9: end-to-end media ingestion + unified-space search.

use std::fs;
use std::path::PathBuf;

use aura_lib::core::embeddings::{embedding_to_bytes, EMBED_DIM};
use aura_lib::core::multimedia::{describe, detect_kind, encode_media, MediaKind};
use aura_lib::core::multimedia::tools::ToolsStatus;
use aura_lib::core::search::{search_blocks, SearchMode};
use aura_lib::core::vault::VaultState;
use aura_lib::db::schemas::MediaRow;
use chrono::Utc;
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
async fn tools_status_reports_missing_binaries_without_panicking() {
    let s = ToolsStatus::probe();
    // Sandbox has none of these; we only assert the type is well-formed.
    let _ = s.yt_dlp.is_some();
    let _ = s.ffmpeg.is_some();
    let _ = s.ffprobe.is_some();
    assert!(!s.url_ingestion_ready() || s.yt_dlp.is_some());
}

#[tokio::test]
async fn ingested_media_shows_up_in_unified_search() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    // Drop a small synthetic mp3 into the vault. We don't need real audio
    // — the stand-in encoder works on bytes + description, and the search
    // engine treats it as a media row in the unified space.
    let rel_path = "Recordings/aura-jam-session.mp3";
    let abs = vault.resolve(rel_path).unwrap();
    fs::create_dir_all(abs.parent().unwrap()).unwrap();
    fs::write(&abs, b"FAKE_MP3_BYTES_FOR_TESTING").unwrap();

    let kind = detect_kind(&abs).unwrap();
    assert_eq!(kind, MediaKind::Audio);
    let bytes = fs::read(&abs).unwrap();
    let metadata = fs::metadata(&abs).unwrap();
    let description = describe(rel_path, kind, metadata.len());
    let emb = encode_media(&description, &bytes);
    let emb_bytes = embedding_to_bytes(&emb);

    let row = MediaRow {
        id: Uuid::now_v7().to_string(),
        path: rel_path.to_string(),
        kind: kind.as_str().to_string(),
        size_bytes: metadata.len() as i64,
        duration_ms: None,
        description: description.clone(),
        indexed_at: Utc::now().timestamp_millis(),
    };
    vault
        .db
        .upsert_media(&row, &emb_bytes, EMBED_DIM as i64)
        .await
        .unwrap();

    assert_eq!(vault.db.count_media().await.unwrap(), 1);

    // Query terms that match the media description should surface the
    // media hit alongside text blocks. We rank in semantic mode so the
    // shared 384-dim space is exercised.
    let hits = search_blocks(&vault.db, "jam session recording audio", SearchMode::Semantic, 10)
        .await
        .unwrap();
    assert!(
        hits.iter().any(|h| h.block_type.starts_with("media:")),
        "media hit should appear in semantic search results: {:?}",
        hits.iter().map(|h| h.block_type.as_str()).collect::<Vec<_>>()
    );

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn delete_removes_media_row() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();

    let rel = "img/diagram.png";
    let abs = vault.resolve(rel).unwrap();
    fs::create_dir_all(abs.parent().unwrap()).unwrap();
    fs::write(&abs, b"FAKE_PNG").unwrap();
    let kind = detect_kind(&abs).unwrap();
    let desc = describe(rel, kind, 8);
    let emb = encode_media(&desc, b"FAKE_PNG");
    let row = MediaRow {
        id: Uuid::now_v7().to_string(),
        path: rel.to_string(),
        kind: kind.as_str().to_string(),
        size_bytes: 8,
        duration_ms: None,
        description: desc,
        indexed_at: Utc::now().timestamp_millis(),
    };
    vault
        .db
        .upsert_media(&row, &embedding_to_bytes(&emb), EMBED_DIM as i64)
        .await
        .unwrap();
    assert_eq!(vault.db.count_media().await.unwrap(), 1);

    vault.db.delete_media_by_path(rel).await.unwrap();
    assert_eq!(vault.db.count_media().await.unwrap(), 0);

    fs::remove_dir_all(&root).ok();
}
