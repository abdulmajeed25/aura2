//! Phase 10: end-to-end MCP server test. Spins up the axum server bound
//! to an OS-assigned localhost port, opens a real vault, then exercises
//! the JSON-RPC envelope (tools/list, tools/call) over raw HTTP/1.1.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use aura_lib::core::vault::VaultState;
use aura_lib::protocols::mcp::McpContext;
use aura_lib::protocols::server::start_server;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
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

async fn post(
    url: &str,
    token: &str,
    body: &Value,
) -> std::io::Result<(u16, Value)> {
    let url = url.strip_prefix("http://").unwrap_or(url);
    let (host_port, path) = match url.split_once('/') {
        Some((a, b)) => (a.to_string(), format!("/{}", b)),
        None => (url.to_string(), "/".to_string()),
    };
    let body_str = serde_json::to_string(body).unwrap();
    let mut stream: Option<TcpStream> = None;
    for _ in 0..10 {
        match TcpStream::connect(&host_port).await {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(25)).await,
        }
    }
    let mut stream = stream.unwrap();
    let req = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n\
         Content-Type: application/json\r\nAuthorization: Bearer {}\r\n\r\n{}",
        path,
        host_port,
        body_str.len(),
        token,
        body_str
    );
    stream.write_all(req.as_bytes()).await?;
    stream.flush().await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let s = String::from_utf8_lossy(&buf);
    let status: u16 = s
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let body_idx = s.find("\r\n\r\n").map(|i| i + 4).unwrap_or(s.len());
    let parsed: Value = serde_json::from_str(s[body_idx..].trim()).unwrap_or(Value::Null);
    Ok((status, parsed))
}

#[tokio::test]
async fn tools_list_returns_aura_tools_against_open_vault() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();
    let vault_slot = Arc::new(Mutex::new(Some(vault)));

    let token = "integration-test-token".to_string();
    let ctx = McpContext {
        vault: vault_slot.clone(),
        auth_token: Arc::new(token.clone()),
        request_count: Arc::new(AtomicU64::new(0)),
    };
    let handle = start_server(ctx, 0).await.unwrap();
    let url = format!("http://127.0.0.1:{}/mcp", handle.port);

    let (status, body) = post(
        &url,
        &token,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
    )
    .await
    .unwrap();
    assert_eq!(status, 200);
    let names: Vec<String> = body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str().map(String::from))
        .collect();
    assert!(names.contains(&"aura_search".to_string()));
    assert!(names.contains(&"aura_read_note".to_string()));

    handle.shutdown().await;
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn search_tool_call_returns_hits_from_real_vault() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();
    let vault_slot = Arc::new(Mutex::new(Some(vault)));

    let token = "token-2".to_string();
    let ctx = McpContext {
        vault: vault_slot.clone(),
        auth_token: Arc::new(token.clone()),
        request_count: Arc::new(AtomicU64::new(0)),
    };
    let handle = start_server(ctx, 0).await.unwrap();
    let url = format!("http://127.0.0.1:{}/mcp", handle.port);

    let (status, body) = post(
        &url,
        &token,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "aura_search",
                "arguments": { "query": "Tauri Phase", "mode": "hybrid", "limit": 5 }
            }
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, 200);
    let hits = body["result"]["structuredContent"].as_array().unwrap();
    assert!(!hits.is_empty(), "expected at least one hit, got {:?}", hits);

    handle.shutdown().await;
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn read_note_tool_returns_file_contents() {
    let root = copy_fixture_to_temp();
    let vault = VaultState::open(root.clone()).await.unwrap();
    vault.reindex().await.unwrap();
    let vault_slot = Arc::new(Mutex::new(Some(vault)));

    let token = "token-3".to_string();
    let ctx = McpContext {
        vault: vault_slot.clone(),
        auth_token: Arc::new(token.clone()),
        request_count: Arc::new(AtomicU64::new(0)),
    };
    let handle = start_server(ctx, 0).await.unwrap();
    let url = format!("http://127.0.0.1:{}/mcp", handle.port);

    let (status, body) = post(
        &url,
        &token,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "aura_read_note",
                "arguments": { "path": "Welcome.md" }
            }
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, 200);
    assert_eq!(
        body["result"]["structuredContent"]["path"],
        Value::String("Welcome.md".to_string())
    );
    let content = body["result"]["structuredContent"]["content"].as_str().unwrap();
    assert!(content.contains("Welcome to Aura"));

    handle.shutdown().await;
    fs::remove_dir_all(&root).ok();
}
