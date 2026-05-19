//! axum-backed MCP server lifecycle: spawn, shut down, query status.
//!
//! The server binds `127.0.0.1` only and gates every request on a Bearer
//! token. There is no remote-access path even if a process on the local
//! machine port-scans loopback — they'd still need the token.
//!
//! Phase 19 hardening (Claw-Chain lessons + RFC 8707 in spirit):
//! - **Request-body size cap** (10 MB) so a malicious local process can't
//!   exhaust memory by sending a multi-GB payload.
//! - **`WWW-Authenticate` challenge** on 401 advertising the server's
//!   `resource_uri` (RFC 6750 + RFC 8707-style audience binding).
//! - **Constant-time token comparison** so a probing process can't
//!   distinguish "right length, wrong byte 5" from "wrong length" via
//!   timing.
//! - **DNS-rebinding mitigation** via Host-header check — only
//!   `127.0.0.1` / `localhost` Host values are accepted, so a malicious
//!   web page that resolves a domain to 127.0.0.1 in step 2 of a
//!   rebinding attack still gets refused.

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::protocols::mcp::{mcp_dispatch, McpContext};

/// Maximum request body size accepted by the MCP endpoint.
pub const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

/// Handle the AppState holds while a server is running.
pub struct McpServerHandle {
    pub port: u16,
    pub auth_token: String,
    pub resource_uri: String,
    pub started_at: i64,
    pub request_count: Arc<AtomicU64>,
    abort: Option<oneshot::Sender<()>>,
    join: JoinHandle<()>,
}

impl McpServerHandle {
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.abort.take() {
            let _ = tx.send(());
        }
        // axum's `with_graceful_shutdown` waits for every in-flight
        // connection to close before the server task exits. Tests using
        // Connection: close don't always trigger that fast enough, so we
        // race the graceful join against a deadline and then abort.
        let deadline = std::time::Duration::from_millis(500);
        if tokio::time::timeout(deadline, &mut self.join).await.is_err() {
            self.join.abort();
            let _ = (&mut self.join).await;
        }
    }
}

/// Start an MCP server bound to `127.0.0.1:port` (port=0 picks a free one).
///
/// Returns the bound port + auth token + canonical resource URI. The caller
/// is expected to store the `McpServerHandle` so the task can be shut down
/// later.
pub async fn start_server(mut ctx: McpContext, port: u16) -> Result<McpServerHandle> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow!("bind {} failed: {}", addr, e))?;
    let bound = listener
        .local_addr()
        .map_err(|e| anyhow!("local_addr failed: {}", e))?;
    let port = bound.port();

    // Set the canonical resource URI now that we know the port. This is what
    // the WWW-Authenticate challenge will advertise.
    let resource_uri = format!("http://127.0.0.1:{}/mcp", port);
    ctx.resource_uri = Arc::new(resource_uri.clone());

    let router = build_router(ctx.clone());
    let (tx, rx) = oneshot::channel::<()>();

    let counter = ctx.request_count.clone();
    let token = (*ctx.auth_token).clone();
    let started_at = chrono::Utc::now().timestamp_millis();

    let join = tokio::spawn(async move {
        let server = axum::serve(listener, router).with_graceful_shutdown(async move {
            let _ = rx.await;
        });
        if let Err(e) = server.await {
            tracing::error!("MCP server crashed: {}", e);
        }
    });

    tracing::info!(target: "aura::mcp", "MCP server listening on {}", bound);

    Ok(McpServerHandle {
        port,
        auth_token: token,
        resource_uri,
        started_at,
        request_count: counter,
        abort: Some(tx),
        join,
    })
}

/// Build the axum router. Exposed for tests so we can call it through
/// `tower::Service` without actually opening a socket.
pub fn build_router(ctx: McpContext) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/mcp", post(mcp_post_handler))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(ctx)
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

/// Constant-time byte-slice equality. Returns `false` on length mismatch
/// without leaking the actual length via early exit.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        // Still XOR with itself for the "expected" path so the timing
        // doesn't differ between mismatched-length and "first byte wrong".
        // (Length is observable from the request anyway; this just keeps
        // the byte-by-byte branch closed.)
        let mut diff: u8 = 1;
        let n = a.len().min(b.len());
        for i in 0..n {
            diff |= a[i] ^ b[i];
        }
        return diff == 0; // can't be 0 because we OR'd 1.
    }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// Is the Host header a legitimate localhost value? Anything else means a
/// DNS rebinding attempt or a misrouted request.
fn host_is_localhost(host: &str) -> bool {
    // Strip optional `:port` suffix.
    let bare = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    matches!(bare, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

fn unauthorized(resource_uri: &str, id: Value, msg: &str) -> impl IntoResponse {
    let challenge = format!(
        "Bearer realm=\"aura\", resource=\"{}\"",
        resource_uri.replace('"', "")
    );
    let challenge_hv =
        HeaderValue::from_str(&challenge).unwrap_or_else(|_| HeaderValue::from_static("Bearer"));
    (
        StatusCode::UNAUTHORIZED,
        [(
            HeaderName::from_static("www-authenticate"),
            challenge_hv,
        )],
        Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32001, "message": msg }
        })),
    )
}

async fn mcp_post_handler(
    State(ctx): State<McpContext>,
    headers: HeaderMap,
    body: Json<Value>,
) -> axum::response::Response {
    let id = body.get("id").cloned().unwrap_or(Value::Null);

    // 1. Host header check (DNS rebinding mitigation).
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !host_is_localhost(host) {
        tracing::warn!(
            target: "aura::mcp",
            "rejecting MCP request with non-loopback Host: {host:?}"
        );
        return unauthorized(&ctx.resource_uri, id, "Untrusted Host").into_response();
    }

    // 2. Bearer token check, constant-time.
    let auth = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let expected = format!("Bearer {}", ctx.auth_token);
    if !constant_time_eq(auth.as_bytes(), expected.as_bytes()) {
        return unauthorized(&ctx.resource_uri, id, "Unauthorized").into_response();
    }

    let resp = mcp_dispatch(&ctx, body.0).await;
    (StatusCode::OK, Json(resp)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn empty_ctx() -> McpContext {
        McpContext {
            vault: Arc::new(Mutex::new(None)),
            auth_token: Arc::new("super-secret".into()),
            request_count: Arc::new(AtomicU64::new(0)),
            resource_uri: Arc::new("http://127.0.0.1:0/mcp".into()),
        }
    }

    #[tokio::test]
    async fn server_binds_and_serves_health() {
        let ctx = empty_ctx();
        let handle = start_server(ctx, 0).await.expect("server start");
        let port = handle.port;

        let resp = reqwest_like_get(&format!("http://127.0.0.1:{}/health", port))
            .await
            .expect("health request");
        assert!(resp.contains("\"ok\""));

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn unauthorised_post_is_rejected_with_401() {
        let ctx = empty_ctx();
        let handle = start_server(ctx, 0).await.unwrap();
        let port = handle.port;

        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" });
        let status = post_status(&format!("http://127.0.0.1:{}/mcp", port), None, &req)
            .await
            .unwrap();
        assert_eq!(status, 401);
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn authorised_post_returns_200_and_pong() {
        let ctx = empty_ctx();
        let handle = start_server(ctx, 0).await.unwrap();
        let port = handle.port;

        let req = json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" });
        let (status, body) = post_full(
            &format!("http://127.0.0.1:{}/mcp", port),
            Some("super-secret"),
            &req,
        )
        .await
        .unwrap();
        assert_eq!(status, 200);
        assert_eq!(body["result"], json!({}));
        assert_eq!(body["id"], 7);
        handle.shutdown().await;
    }

    // Lightweight HTTP client — we don't want to add a reqwest dep just for
    // tests, so the bytes get assembled by hand. Sufficient for localhost
    // round-trips over HTTP/1.1.
    async fn raw_request(
        url: &str,
        method: &str,
        bearer: Option<&str>,
        body: Option<&str>,
    ) -> std::io::Result<(u16, String)> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let url = url.strip_prefix("http://").unwrap_or(url);
        let (host_port, path) = match url.split_once('/') {
            Some((a, b)) => (a.to_string(), format!("/{}", b)),
            None => (url.to_string(), "/".to_string()),
        };

        // The axum server task spins up asynchronously after start_server
        // returns; retry the initial connect for up to ~250ms.
        let mut stream = None;
        for _ in 0..10 {
            match tokio::net::TcpStream::connect(&host_port).await {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(25)).await,
            }
        }
        let mut stream = stream.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "no connect")
        })?;

        let body_str = body.unwrap_or("");
        let mut req = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n",
            method,
            path,
            host_port,
            body_str.len()
        );
        if body.is_some() {
            req.push_str("Content-Type: application/json\r\n");
        }
        if let Some(b) = bearer {
            req.push_str(&format!("Authorization: Bearer {}\r\n", b));
        }
        req.push_str("\r\n");
        req.push_str(body_str);
        stream.write_all(req.as_bytes()).await?;
        stream.flush().await?;
        // Do NOT call shutdown(Write) here — some HTTP servers treat the
        // half-close as a signal to abort the connection before producing
        // the response. The Content-Length on the request already tells the
        // server when our body ends.

        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await?;
        let buf = String::from_utf8_lossy(&bytes).to_string();
        let status_line = buf.lines().next().unwrap_or("");
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body_idx = buf.find("\r\n\r\n").map(|i| i + 4).unwrap_or(buf.len());
        Ok((status, buf[body_idx..].to_string()))
    }

    async fn reqwest_like_get(url: &str) -> std::io::Result<String> {
        let (_, body) = raw_request(url, "GET", None, None).await?;
        Ok(body)
    }

    async fn post_status(url: &str, bearer: Option<&str>, body: &Value) -> std::io::Result<u16> {
        let body_str = serde_json::to_string(body).unwrap();
        let (status, _) = raw_request(url, "POST", bearer, Some(&body_str)).await?;
        Ok(status)
    }

    async fn post_full(
        url: &str,
        bearer: Option<&str>,
        body: &Value,
    ) -> std::io::Result<(u16, Value)> {
        let body_str = serde_json::to_string(body).unwrap();
        let (status, resp) = raw_request(url, "POST", bearer, Some(&body_str)).await?;
        let parsed: Value = serde_json::from_str(resp.trim()).unwrap_or(Value::Null);
        Ok((status, parsed))
    }
}
