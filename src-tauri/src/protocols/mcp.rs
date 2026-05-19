//! Model Context Protocol (MCP) request handler.
//!
//! The endpoint is JSON-RPC 2.0 over HTTP POST. Clients call:
//! - `initialize` → server capabilities
//! - `tools/list` → tool catalogue
//! - `tools/call` → execute a tool
//! - `ping` → liveness
//!
//! The handler is intentionally pure-data (no axum types) so it can be
//! tested directly. `mcp_dispatch` reads a serde_json::Value request and
//! returns a serde_json::Value response.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::core::graph_rag::query_engine::run_query;
use crate::core::search::{search_blocks, SearchMode};
use crate::core::vault::VaultState;
use crate::db::sqlite::VaultDb;

/// State shared by the MCP HTTP handler. Cloning is cheap (just Arcs).
#[derive(Clone)]
pub struct McpContext {
    pub vault: Arc<Mutex<Option<VaultState>>>,
    pub auth_token: Arc<String>,
    pub request_count: Arc<std::sync::atomic::AtomicU64>,
}

const PROTOCOL_VERSION: &str = "2025-03-26";
const SERVER_NAME: &str = "aura-knowledge-engine";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run an MCP request and return the JSON-RPC response object.
pub async fn mcp_dispatch(ctx: &McpContext, req: Value) -> Value {
    ctx.request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");

    match method {
        "initialize" => respond_ok(id, json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": false, "listChanged": false }
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION
            }
        })),
        "ping" => respond_ok(id, json!({})),
        "tools/list" => respond_ok(id, json!({ "tools": tool_catalogue() })),
        "tools/call" => match handle_tool_call(ctx, &req).await {
            Ok(v) => respond_ok(id, v),
            Err((code, msg)) => respond_err(id, code, &msg),
        },
        _ => respond_err(id, -32601, "Method not found"),
    }
}

fn respond_ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn respond_err(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn tool_catalogue() -> Vec<Value> {
    vec![
        json!({
            "name": "aura_search",
            "description": "Semantic / FTS / hybrid search across all blocks in the vault.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "mode": { "type": "string", "enum": ["semantic", "fts", "hybrid"], "default": "hybrid" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 20 }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "aura_read_note",
            "description": "Read the full Markdown content of a vault-relative note path.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }
        }),
        json!({
            "name": "aura_write_note",
            "description": "Create or overwrite a Markdown note. Reindexes the file immediately.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }
        }),
        json!({
            "name": "aura_list_notes",
            "description": "Enumerate every indexed file in the vault as `(path, title)` pairs.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "aura_get_backlinks",
            "description": "Return every link whose target is the given note.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }
        }),
        json!({
            "name": "aura_graph_rag_query",
            "description": "Run a global GraphRAG query and return a compact context payload of the top communities.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "question": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 20, "default": 3 }
                },
                "required": ["question"]
            }
        }),
    ]
}

type ToolResult = Result<Value, (i32, String)>;

async fn handle_tool_call(ctx: &McpContext, req: &Value) -> ToolResult {
    let params = req.get("params").ok_or_else(|| (-32602, "missing params".to_string()))?;
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (-32602, "missing name".to_string()))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    // Most tools need a vault; acquire it once.
    let guard = ctx.vault.lock().await;
    let vault = guard
        .as_ref()
        .ok_or_else(|| (-32001, "no vault is open".to_string()))?;

    let value = match name {
        "aura_search" => tool_search(&vault.db, vault.encoder.as_ref(), &args).await?,
        "aura_read_note" => tool_read_note(vault, &args).await?,
        "aura_write_note" => tool_write_note(vault, &args).await?,
        "aura_list_notes" => tool_list_notes(&vault.db).await?,
        "aura_get_backlinks" => tool_get_backlinks(&vault.db, &args).await?,
        "aura_graph_rag_query" => {
            tool_graph_rag_query(&vault.db, vault.encoder.as_ref(), &args).await?
        }
        other => return Err((-32601, format!("unknown tool: {}", other))),
    };

    // MCP convention: tools/call returns { content: [{type:"text", text:"..."}] }.
    Ok(json!({
        "content": [
            { "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }
        ],
        "structuredContent": value,
        "isError": false
    }))
}

async fn tool_search(
    db: &VaultDb,
    encoder: &dyn crate::core::embeddings::TextEncoder,
    args: &Value,
) -> ToolResult {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (-32602, "query required".to_string()))?;
    let mode = match args.get("mode").and_then(|v| v.as_str()) {
        Some("semantic") => SearchMode::Semantic,
        Some("fts") => SearchMode::Fts,
        _ => SearchMode::Hybrid,
    };
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;
    let hits = search_blocks(db, encoder, query, mode, limit)
        .await
        .map_err(|e| (-32000, e.to_string()))?;
    Ok(serde_json::to_value(hits).unwrap_or(json!([])))
}

async fn tool_read_note(vault: &VaultState, args: &Value) -> ToolResult {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (-32602, "path required".to_string()))?;
    let abs = vault
        .resolve(path)
        .map_err(|e| (-32602, e.to_string()))?;
    let content = std::fs::read_to_string(&abs).map_err(|e| (-32000, e.to_string()))?;
    Ok(json!({ "path": path, "content": content }))
}

async fn tool_write_note(vault: &VaultState, args: &Value) -> ToolResult {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (-32602, "path required".to_string()))?;
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (-32602, "content required".to_string()))?;
    let abs = vault.resolve(path).map_err(|e| (-32602, e.to_string()))?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| (-32000, e.to_string()))?;
    }
    std::fs::write(&abs, content).map_err(|e| (-32000, e.to_string()))?;
    vault
        .index_one(&abs)
        .await
        .map_err(|e| (-32000, e.to_string()))?;
    Ok(json!({ "path": path, "bytes_written": content.len() }))
}

async fn tool_list_notes(db: &VaultDb) -> ToolResult {
    let cands = db.list_link_candidates().await.map_err(|e| (-32000, e.to_string()))?;
    Ok(serde_json::to_value(cands).unwrap_or(json!([])))
}

async fn tool_get_backlinks(db: &VaultDb, args: &Value) -> ToolResult {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (-32602, "path required".to_string()))?;
    let rows = db.get_backlinks(path).await.map_err(|e| (-32000, e.to_string()))?;
    Ok(serde_json::to_value(rows).unwrap_or(json!([])))
}

async fn tool_graph_rag_query(
    db: &VaultDb,
    encoder: &dyn crate::core::embeddings::TextEncoder,
    args: &Value,
) -> ToolResult {
    let q = args
        .get("question")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (-32602, "question required".to_string()))?;
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
    let answer = run_query(db, encoder, q, limit)
        .await
        .map_err(|e| (-32000, e.to_string()))?;
    Ok(serde_json::to_value(answer).unwrap_or(json!({})))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_ctx() -> McpContext {
        McpContext {
            vault: Arc::new(Mutex::new(None)),
            auth_token: Arc::new("test-token".into()),
            request_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    #[tokio::test]
    async fn initialize_returns_protocol_version_and_capabilities() {
        let ctx = empty_ctx();
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
        let resp = mcp_dispatch(&ctx, req).await;
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(resp["result"]["serverInfo"]["name"], SERVER_NAME);
    }

    #[tokio::test]
    async fn ping_returns_empty_result() {
        let ctx = empty_ctx();
        let req = json!({ "jsonrpc": "2.0", "id": 99, "method": "ping" });
        let resp = mcp_dispatch(&ctx, req).await;
        assert_eq!(resp["result"], json!({}));
    }

    #[tokio::test]
    async fn tools_list_includes_all_seven_tools() {
        let ctx = empty_ctx();
        let req = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
        let resp = mcp_dispatch(&ctx, req).await;
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        for expected in &[
            "aura_search",
            "aura_read_note",
            "aura_write_note",
            "aura_list_notes",
            "aura_get_backlinks",
            "aura_graph_rag_query",
        ] {
            assert!(names.contains(expected), "missing tool: {}", expected);
        }
    }

    #[tokio::test]
    async fn unknown_method_returns_jsonrpc_error() {
        let ctx = empty_ctx();
        let req = json!({ "jsonrpc": "2.0", "id": 3, "method": "nonexistent" });
        let resp = mcp_dispatch(&ctx, req).await;
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn tool_call_without_open_vault_errors() {
        let ctx = empty_ctx();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "aura_search",
                "arguments": { "query": "anything" }
            }
        });
        let resp = mcp_dispatch(&ctx, req).await;
        assert_eq!(resp["error"]["code"], -32001);
        assert!(resp["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no vault"));
    }
}
