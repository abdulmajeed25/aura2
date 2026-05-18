use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::protocols::auth::new_token;
use crate::protocols::mcp::McpContext;
use crate::protocols::server::start_server;
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct McpStatus {
    pub running: bool,
    pub url: Option<String>,
    pub port: Option<u16>,
    pub auth_token: Option<String>,
    pub started_at: Option<i64>,
    pub request_count: u64,
}

/// Start (or restart) the MCP server bound to `127.0.0.1`. Passing `port=0`
/// auto-picks a free port. The auth token is freshly generated every start
/// so a stale token from a previous run never works.
#[tauri::command]
pub async fn start_mcp_server(
    state: State<'_, AppState>,
    port: Option<u16>,
) -> CmdResult<McpStatus> {
    // If already running, shut down first so callers get a clean restart.
    let prev = {
        let mut guard = state.mcp.lock().await;
        guard.take()
    };
    if let Some(handle) = prev {
        handle.shutdown().await;
    }

    let token = new_token();
    let ctx = McpContext {
        vault: state.vault.clone(),
        auth_token: Arc::new(token.clone()),
        request_count: Arc::new(AtomicU64::new(0)),
    };

    let handle = start_server(ctx, port.unwrap_or(47820))
        .await
        .map_err(AuraError::from)?;
    let url = Some(format!("http://127.0.0.1:{}/mcp", handle.port));
    let status = McpStatus {
        running: true,
        url,
        port: Some(handle.port),
        auth_token: Some(token),
        started_at: Some(handle.started_at),
        request_count: 0,
    };
    let mut guard = state.mcp.lock().await;
    *guard = Some(handle);
    Ok(status)
}

#[tauri::command]
pub async fn stop_mcp_server(state: State<'_, AppState>) -> CmdResult<McpStatus> {
    let prev = {
        let mut guard = state.mcp.lock().await;
        guard.take()
    };
    if let Some(handle) = prev {
        handle.shutdown().await;
    }
    Ok(McpStatus {
        running: false,
        url: None,
        port: None,
        auth_token: None,
        started_at: None,
        request_count: 0,
    })
}

#[tauri::command]
pub async fn mcp_status(state: State<'_, AppState>) -> CmdResult<McpStatus> {
    let guard = state.mcp.lock().await;
    match guard.as_ref() {
        Some(h) => Ok(McpStatus {
            running: true,
            url: Some(format!("http://127.0.0.1:{}/mcp", h.port)),
            port: Some(h.port),
            auth_token: Some(h.auth_token.clone()),
            started_at: Some(h.started_at),
            request_count: h.request_count.load(Ordering::Relaxed),
        }),
        None => Ok(McpStatus {
            running: false,
            url: None,
            port: None,
            auth_token: None,
            started_at: None,
            request_count: 0,
        }),
    }
}
