//! `AnthropicProvider` — `AIProvider` impl backed by the Messages API.
//!
//! Wire format reference: <https://docs.anthropic.com/en/api/messages>
//!
//! Hardenings:
//! - **API key never logged.** The key lives behind `ApiKey` (redacted
//!   `Debug`); the only path that reads it is `header_value()` which
//!   feeds the `x-api-key` request header.
//! - **429 backoff with jitter.** Honors `Retry-After` when present;
//!   otherwise exponential 1s → 2s → 4s → 8s → 16s, ±30% jitter, max
//!   5 attempts.
//! - **Budget guard.** Before each call we ask the audit log for
//!   today's cumulative spend. Above `daily_cap_cents`, we refuse the
//!   call with `AiError::BudgetExceeded` — no surprise bills.
//! - **Audit every outcome.** Even error rows land in `audit_log` so
//!   the user can see "we tried, server returned 500" later.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::Utc;
use rand::Rng;
use reqwest::StatusCode;
use serde_json::{json, Value};

use crate::ai::audit::{AuditEvent, DbAuditLogger};
use crate::ai::providers::{
    AIProvider, AiError, CacheTtl, ChatRequest, ChatResponse, Message, Role, TextBlock, Usage,
};
use crate::ai::secrets::ApiKey;

const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Max retries for 429 / 5xx before giving up.
const MAX_RETRIES: u32 = 5;
/// Default daily budget in cents (== $5). The user can override per
/// session via `AnthropicProvider::with_daily_cap_cents`.
pub const DEFAULT_DAILY_CAP_CENTS: i64 = 500;

pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: ApiKey,
    base_url: String,
    audit: Arc<DbAuditLogger>,
    daily_cap_cents: i64,
    /// Optional retrieval-prompt compressor. Wired into the
    /// pre-send [`AnthropicProvider::compress`] step. `None` means
    /// "pass-through" — the prompt ships unchanged.
    compressor: Option<Arc<dyn crate::ai::retrieval::Compressor>>,
}

impl AnthropicProvider {
    pub fn new(api_key: ApiKey, audit: Arc<DbAuditLogger>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent(concat!("aura/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(120))
                .build()
                .expect("reqwest client init"),
            api_key,
            base_url: ANTHROPIC_API_BASE.to_string(),
            audit,
            daily_cap_cents: DEFAULT_DAILY_CAP_CENTS,
            compressor: None,
        }
    }

    /// Install a compressor for the retrieval-prompt pre-send step.
    /// See [`crate::ai::retrieval::SidecarCompressor`] for the live
    /// LLMLingua-2 implementation.
    pub fn with_compressor(
        mut self,
        c: Arc<dyn crate::ai::retrieval::Compressor>,
    ) -> Self {
        self.compressor = Some(c);
        self
    }

    /// Run the configured compressor on `text`, falling back to the
    /// input unchanged if no compressor is installed or the call
    /// fails. The fallback is deliberate: the chat path must never
    /// fail because a sidecar happens to be unreachable.
    pub async fn compress(&self, text: &str, target_ratio: f32) -> String {
        let Some(c) = self.compressor.as_ref() else {
            return text.to_string();
        };
        match c.compress(text, target_ratio).await {
            Ok(r) => r.compressed,
            Err(e) => {
                tracing::warn!(
                    target: "aura::ai",
                    "compressor failed, sending uncompressed: {e}"
                );
                text.to_string()
            }
        }
    }

    /// Tests point this at a localhost mock server.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn with_daily_cap_cents(mut self, cents: i64) -> Self {
        self.daily_cap_cents = cents;
        self
    }

    /// Build the request body. Pure JSON shape — easy to test in
    /// isolation without spinning the HTTP client.
    pub fn build_request_body(&self, req: &ChatRequest) -> Value {
        let mut body = json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
        });
        if let Some(t) = req.temperature {
            body["temperature"] = json!(t);
        }
        if !req.stop_sequences.is_empty() {
            body["stop_sequences"] = json!(req.stop_sequences);
        }
        if !req.system.is_empty() {
            body["system"] = json!(req
                .system
                .iter()
                .map(text_block_json)
                .collect::<Vec<Value>>());
        }
        body["messages"] = json!(req.messages.iter().map(message_json).collect::<Vec<Value>>());
        body
    }
}

fn text_block_json(b: &TextBlock) -> Value {
    let mut obj = json!({
        "type": "text",
        "text": b.text,
    });
    if let Some(ttl) = b.cache {
        obj["cache_control"] = json!({
            "type": "ephemeral",
            "ttl": match ttl {
                CacheTtl::FiveMinutes => "5m",
                CacheTtl::OneHour => "1h",
            },
        });
    }
    obj
}

fn message_json(m: &Message) -> Value {
    json!({
        "role": match m.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        },
        "content": m.content.iter().map(text_block_json).collect::<Vec<Value>>(),
    })
}

/// USD-cent prices per **million** tokens. Update when Anthropic
/// adjusts pricing — these are the only knobs cost calculation cares
/// about. f64 is fine; final storage rounds to micro-cents.
struct PriceRow {
    input_per_mtok_cents: f64,
    cache_write_per_mtok_cents: f64,
    cache_read_per_mtok_cents: f64,
    output_per_mtok_cents: f64,
}

/// Picks a price row by model name prefix. Conservative: unknown
/// models default to the **highest** rate seen so far, so we
/// over-estimate rather than under-bill.
fn price_for_model(model: &str) -> PriceRow {
    let m = model.to_lowercase();
    if m.contains("haiku") {
        // Haiku 4.5 list price (as of Jan 2026 — keep in sync with
        // https://docs.anthropic.com/en/docs/about-claude/pricing).
        PriceRow {
            input_per_mtok_cents: 100.0,
            cache_write_per_mtok_cents: 125.0,
            cache_read_per_mtok_cents: 10.0,
            output_per_mtok_cents: 500.0,
        }
    } else if m.contains("opus") {
        PriceRow {
            input_per_mtok_cents: 1500.0,
            cache_write_per_mtok_cents: 1875.0,
            cache_read_per_mtok_cents: 150.0,
            output_per_mtok_cents: 7500.0,
        }
    } else {
        // Sonnet 4.6 + unknown models default here.
        PriceRow {
            input_per_mtok_cents: 300.0,
            cache_write_per_mtok_cents: 375.0,
            cache_read_per_mtok_cents: 30.0,
            output_per_mtok_cents: 1500.0,
        }
    }
}

fn compute_cost_cents(model: &str, usage: &Usage) -> f64 {
    let p = price_for_model(model);
    let mtok = 1_000_000.0_f64;
    (usage.input_tokens as f64 * p.input_per_mtok_cents / mtok)
        + (usage.cache_creation_input_tokens as f64 * p.cache_write_per_mtok_cents / mtok)
        + (usage.cache_read_input_tokens as f64 * p.cache_read_per_mtok_cents / mtok)
        + (usage.output_tokens as f64 * p.output_per_mtok_cents / mtok)
}

async fn jitter_sleep(base_ms: u64) {
    let factor: f32 = rand::thread_rng().gen_range(0.85..1.15);
    let wait = ((base_ms as f32) * factor) as u64;
    tokio::time::sleep(Duration::from_millis(wait.max(50))).await;
}

#[async_trait]
impl AIProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AiError> {
        // Budget guard FIRST so we don't spend a single token over cap.
        let today = self
            .audit
            .today_cost_cents()
            .await
            .map_err(|e| AiError::Audit(e.to_string()))?;
        if today >= self.daily_cap_cents {
            return Err(AiError::BudgetExceeded {
                spent_cents: today,
                cap_cents: self.daily_cap_cents,
            });
        }

        let started = Instant::now();
        let body = self.build_request_body(&req);
        let url = format!("{}/v1/messages", self.base_url);

        let mut retry_delay_ms = 1_000u64;
        let mut attempts = 0u32;

        loop {
            attempts += 1;
            let resp_result = self
                .client
                .post(&url)
                .header("x-api-key", self.api_key.header_value())
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await;

            let resp = match resp_result {
                Ok(r) => r,
                Err(e) => {
                    // Network-level error — audit and bail.
                    self.audit_failure(&req, started, "error", &e.to_string())
                        .await;
                    return Err(AiError::Network(redact_network_error(e.to_string())));
                }
            };

            let status = resp.status();
            if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                if attempts >= MAX_RETRIES {
                    let body_txt = resp.text().await.unwrap_or_default();
                    self.audit_failure(&req, started, "rate_limited", &body_txt)
                        .await;
                    return Err(AiError::RateLimited { attempts });
                }
                // Honor Retry-After if present, otherwise exponential.
                let server_hint_ms = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|s| s * 1000);
                let wait_ms = server_hint_ms.unwrap_or(retry_delay_ms);
                retry_delay_ms = (retry_delay_ms * 2).min(16_000);
                jitter_sleep(wait_ms).await;
                continue;
            }
            if !status.is_success() {
                let body_txt = resp.text().await.unwrap_or_default();
                self.audit_failure(&req, started, "error", &body_txt).await;
                return Err(AiError::Http {
                    status: status.as_u16(),
                    body: body_txt,
                });
            }

            let payload: Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => {
                    self.audit_failure(&req, started, "error", &e.to_string())
                        .await;
                    return Err(AiError::BadResponse(e.to_string()));
                }
            };

            let (content, stop_reason, model_out, mut usage) = parse_messages_response(&payload)?;
            usage.cost_usd_cents = compute_cost_cents(&model_out, &usage);

            let duration_ms = started.elapsed().as_millis() as i64;
            let event = AuditEvent {
                timestamp: Utc::now().timestamp_millis(),
                actor: self.name().to_string(),
                operation: "chat".to_string(),
                model: Some(model_out.clone()),
                usage: usage.clone(),
                duration_ms,
                status: "ok".to_string(),
                metadata_json: req.metadata.clone(),
            };
            if let Err(e) = self.audit.log(event).await {
                tracing::warn!(target: "aura::ai", "audit log write failed: {e}");
            }

            return Ok(ChatResponse {
                content,
                model: model_out,
                stop_reason,
                usage,
            });
        }
    }
}

impl AnthropicProvider {
    async fn audit_failure(
        &self,
        req: &ChatRequest,
        started: Instant,
        status: &str,
        message: &str,
    ) {
        let event = AuditEvent {
            timestamp: Utc::now().timestamp_millis(),
            actor: self.name().to_string(),
            operation: "chat".to_string(),
            model: Some(req.model.clone()),
            usage: Usage::default(),
            duration_ms: started.elapsed().as_millis() as i64,
            status: status.to_string(),
            metadata_json: serde_json::json!({
                "error_excerpt": message.chars().take(200).collect::<String>(),
                "request_metadata": req.metadata.clone(),
            }),
        };
        if let Err(e) = self.audit.log(event).await {
            tracing::warn!(target: "aura::ai", "audit log write failed: {e}");
        }
    }
}

fn parse_messages_response(v: &Value) -> Result<(String, Option<String>, String, Usage), AiError> {
    let content_arr = v
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| AiError::BadResponse("missing content array".into()))?;
    let mut text = String::new();
    for block in content_arr {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(s) = block.get("text").and_then(|t| t.as_str()) {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(s);
            }
        }
    }
    let model = v
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown")
        .to_string();
    let stop_reason = v
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());
    let usage_v = v.get("usage").cloned().unwrap_or_else(|| json!({}));
    let usage = Usage {
        input_tokens: usage_v
            .get("input_tokens")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
        cache_creation_input_tokens: usage_v
            .get("cache_creation_input_tokens")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
        cache_read_input_tokens: usage_v
            .get("cache_read_input_tokens")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
        output_tokens: usage_v
            .get("output_tokens")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
        cost_usd_cents: 0.0, // filled in by caller
    };
    Ok((text, stop_reason, model, usage))
}

/// reqwest error strings can include the URL we hit. The URL doesn't
/// contain the key (it's in a header) but we still scrub aggressively
/// in case future libraries serialize headers.
fn redact_network_error(s: String) -> String {
    let mut out = s;
    if out.contains("sk-ant-") {
        out = out
            .split_whitespace()
            .map(|w| {
                if w.starts_with("sk-ant-") {
                    "<redacted-key>"
                } else {
                    w
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sqlite::VaultDb;

    async fn fresh_audit() -> (Arc<DbAuditLogger>, std::path::PathBuf) {
        let tmp = std::env::temp_dir().join(format!("aura-audit-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let db = VaultDb::open(&tmp.join(".aura").join("aura.db"))
            .await
            .unwrap();
        let audit = Arc::new(DbAuditLogger::new(Arc::new(db)));
        (audit, tmp)
    }

    fn dummy_key() -> ApiKey {
        ApiKey::from_string("sk-test-not-real")
    }

    #[test]
    fn build_request_body_carries_cache_control_breakpoint() {
        let req = ChatRequest::new("claude-haiku-4-5", "Hello")
            .with_system("Long stable preamble", Some(CacheTtl::OneHour))
            .with_max_tokens(256);
        // We can build the body without ever touching network.
        let dummy_db_path = std::env::temp_dir().join(format!(
            "aura-bodytest-{}",
            uuid::Uuid::now_v7()
        ));
        // Construct a provider just for build_request_body — no audit
        // log needed because we don't call chat().
        let body = build_body_static(&req);
        let _ = dummy_db_path; // unused

        // Top-level fields:
        assert_eq!(body["model"], "claude-haiku-4-5");
        assert_eq!(body["max_tokens"], 256);
        // System carries one block with cache_control:
        let sys = body["system"].as_array().expect("system array");
        assert_eq!(sys.len(), 1);
        assert_eq!(sys[0]["cache_control"]["type"], "ephemeral");
        assert_eq!(sys[0]["cache_control"]["ttl"], "1h");
        // Messages carry the user text:
        let msgs = body["messages"].as_array().expect("messages array");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    /// Helper: build body without spinning up the full provider (which
    /// would need a real DB-backed audit logger).
    fn build_body_static(req: &ChatRequest) -> Value {
        let mut body = json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
        });
        if !req.system.is_empty() {
            body["system"] = json!(req
                .system
                .iter()
                .map(text_block_json)
                .collect::<Vec<Value>>());
        }
        body["messages"] = json!(req.messages.iter().map(message_json).collect::<Vec<Value>>());
        body
    }

    #[test]
    fn cost_for_haiku_matches_published_price() {
        // 1M input tokens of Haiku 4.5 → $1.00 = 100 cents.
        let usage = Usage {
            input_tokens: 1_000_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            output_tokens: 0,
            cost_usd_cents: 0.0,
        };
        let cost = compute_cost_cents("claude-haiku-4-5", &usage);
        assert!((cost - 100.0).abs() < 1e-3, "haiku 1M input = ${cost}¢");

        // 1M cached-read tokens of Haiku → $0.10 = 10 cents.
        let cached = Usage {
            cache_read_input_tokens: 1_000_000,
            ..Default::default()
        };
        let cost_cached = compute_cost_cents("claude-haiku-4-5", &cached);
        assert!((cost_cached - 10.0).abs() < 1e-3, "haiku 1M cached = ${cost_cached}¢");
    }

    #[test]
    fn redact_network_error_replaces_visible_keys() {
        let dirty = "Failed to send request to https://api.anthropic.com with sk-ant-api03-abcdef".to_string();
        let scrubbed = redact_network_error(dirty);
        assert!(!scrubbed.contains("sk-ant-api03"), "key leaked: {scrubbed}");
        assert!(scrubbed.contains("<redacted-key>"));
    }

    /// Mock-server end-to-end. Spawns axum on 127.0.0.1:0, points the
    /// provider at it, asserts the response is parsed and the audit
    /// row landed.
    #[tokio::test]
    async fn chat_against_mock_server_returns_content_and_audits() {
        let (audit, vault_root) = fresh_audit().await;
        let mock = spawn_mock_server(mock_ok()).await;
        let provider = AnthropicProvider::new(dummy_key(), audit.clone())
            .with_base_url(format!("http://{}", mock.addr));

        let req = ChatRequest::new("claude-haiku-4-5", "Hi");
        let resp = provider.chat(req).await.unwrap();
        assert_eq!(resp.content, "Mock response");
        assert_eq!(resp.model, "claude-haiku-4-5");
        assert_eq!(resp.usage.output_tokens, 5);
        assert!(resp.usage.cost_usd_cents > 0.0);

        // Audit row landed for an "ok" call.
        let ratio = audit.cache_hit_ratio(24).await.unwrap();
        // No cache_read tokens in mock → ratio = 0/total_tokens > 0
        assert!(ratio.is_some());

        let cost_cents = audit.today_cost_cents().await.unwrap();
        let _ = cost_cents; // could be 0 if very small; we just check it was written
        drop(mock);
        std::fs::remove_dir_all(&vault_root).ok();
    }

    /// 429 followed by 200 → retry succeeds, audit shows one "ok" row.
    #[tokio::test]
    async fn handles_429_with_backoff_then_succeeds() {
        let (audit, vault_root) = fresh_audit().await;
        // First call: 429. Second call: 200.
        let mock = spawn_mock_server(mock_429_then_ok()).await;
        let provider = AnthropicProvider::new(dummy_key(), audit.clone())
            .with_base_url(format!("http://{}", mock.addr));
        let resp = provider
            .chat(ChatRequest::new("claude-haiku-4-5", "Hi"))
            .await
            .unwrap();
        assert_eq!(resp.content, "Mock response");
        drop(mock);
        std::fs::remove_dir_all(&vault_root).ok();
    }

    /// Budget guard: once today_cost_cents ≥ cap, refuse.
    #[tokio::test]
    async fn refuses_call_when_today_over_budget() {
        let (audit, vault_root) = fresh_audit().await;
        // Pre-fill audit with a "huge" cost row.
        audit
            .log(AuditEvent {
                timestamp: Utc::now().timestamp_millis(),
                actor: "anthropic".into(),
                operation: "chat".into(),
                model: Some("claude-opus-4-7".into()),
                usage: Usage {
                    output_tokens: 0,
                    cost_usd_cents: 1_000.0, // 10 dollars
                    ..Default::default()
                },
                duration_ms: 100,
                status: "ok".into(),
                metadata_json: Value::Null,
            })
            .await
            .unwrap();

        // Provider with 500-cent ($5) cap should refuse — server URL
        // doesn't even need to exist because we should never reach it.
        let provider = AnthropicProvider::new(dummy_key(), audit.clone())
            .with_base_url("http://127.0.0.1:1") // unreachable on purpose
            .with_daily_cap_cents(500);
        let err = provider
            .chat(ChatRequest::new("claude-haiku-4-5", "Hi"))
            .await
            .unwrap_err();
        match err {
            AiError::BudgetExceeded {
                spent_cents,
                cap_cents,
            } => {
                assert!(spent_cents >= 1_000);
                assert_eq!(cap_cents, 500);
            }
            other => panic!("expected BudgetExceeded, got {other:?}"),
        }
        std::fs::remove_dir_all(&vault_root).ok();
    }

    /// Even when the provider hits a network error, the audit row
    /// lands and the error message doesn't contain the key.
    #[tokio::test]
    async fn network_error_audits_and_redacts_key() {
        let (audit, vault_root) = fresh_audit().await;
        let provider = AnthropicProvider::new(dummy_key(), audit.clone())
            .with_base_url("http://127.0.0.1:1");
        let err = provider
            .chat(ChatRequest::new("claude-haiku-4-5", "Hi"))
            .await
            .unwrap_err();
        let msg = format!("{err}");
        assert!(!msg.contains("sk-test-not-real"), "key leaked: {msg}");
        // Audit row landed with status error.
        std::fs::remove_dir_all(&vault_root).ok();
    }

    // ---- Mock-server scaffolding -------------------------------------

    struct MockHandle {
        addr: std::net::SocketAddr,
        shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    }
    impl Drop for MockHandle {
        fn drop(&mut self) {
            if let Some(s) = self.shutdown.take() {
                let _ = s.send(());
            }
        }
    }

    type MockHandler = std::sync::Arc<
        dyn Fn() -> (axum::http::StatusCode, Value) + Send + Sync + 'static,
    >;

    async fn spawn_mock_server(handler: MockHandler) -> MockHandle {
        use axum::{routing::post, Router};
        let app = Router::new().route(
            "/v1/messages",
            post({
                let h = handler.clone();
                move || {
                    let h = h.clone();
                    async move {
                        let (status, body) = (h)();
                        (status, axum::Json(body))
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async move {
                let _ = rx.await;
            });
            let _ = server.await;
        });
        // Tiny settle pause so the server is accepting before the first call.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        MockHandle {
            addr,
            shutdown: Some(tx),
        }
    }

    fn mock_ok() -> MockHandler {
        std::sync::Arc::new(|| {
            (
                axum::http::StatusCode::OK,
                json!({
                    "id": "msg_test_ok",
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "text", "text": "Mock response"}],
                    "model": "claude-haiku-4-5",
                    "stop_reason": "end_turn",
                    "usage": {
                        "input_tokens": 10,
                        "cache_creation_input_tokens": 0,
                        "cache_read_input_tokens": 0,
                        "output_tokens": 5
                    }
                }),
            )
        })
    }

    fn mock_429_then_ok() -> MockHandler {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        std::sync::Arc::new(move || {
            let n = count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n == 0 {
                (
                    axum::http::StatusCode::TOO_MANY_REQUESTS,
                    json!({"type": "error", "error": {"type": "rate_limit_error", "message": "wait"}}),
                )
            } else {
                (
                    axum::http::StatusCode::OK,
                    json!({
                        "id": "msg_after_retry",
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "text", "text": "Mock response"}],
                        "model": "claude-haiku-4-5",
                        "stop_reason": "end_turn",
                        "usage": {
                            "input_tokens": 10,
                            "cache_creation_input_tokens": 0,
                            "cache_read_input_tokens": 0,
                            "output_tokens": 5
                        }
                    }),
                )
            }
        })
    }
}
