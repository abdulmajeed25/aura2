//! `MockProvider` — a deterministic `AIProvider` for tests and offline
//! demos. Holds a `Vec<ChatResponse>` and pops one per call. When
//! exhausted, returns the **last** response repeatedly (so a test can
//! configure "all calls return X" with a single-element vec).
//!
//! Useful when:
//! - The real Anthropic key isn't available (CI, sandbox).
//! - You want a script-of-responses to drive a deterministic test.
//!
//! The mock records every received request so tests can assert "the
//! provider was called with this prompt", "the system prompt carried a
//! cache breakpoint", etc.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::ai::providers::{AIProvider, AiError, ChatRequest, ChatResponse, Usage};

pub struct MockProvider {
    inner: Mutex<Inner>,
}

struct Inner {
    /// Scripted responses, popped front-to-back. Last one repeats when
    /// the queue is empty.
    responses: Vec<ChatResponse>,
    /// Every received request, in call order. Tests inspect this to
    /// assert prompts / cache_control / etc.
    requests: Vec<ChatRequest>,
}

impl MockProvider {
    /// Build with a canned response. The same response is returned for
    /// every call until the queue is exhausted (then it repeats).
    pub fn with_response(content: impl Into<String>) -> Self {
        Self::with_responses(vec![ChatResponse {
            content: content.into(),
            model: "mock-claude".to_string(),
            stop_reason: Some("end_turn".to_string()),
            usage: Usage {
                input_tokens: 50,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                output_tokens: 25,
                cost_usd_cents: 0.0,
            },
        }])
    }

    pub fn with_responses(responses: Vec<ChatResponse>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                responses,
                requests: Vec::new(),
            }),
        }
    }

    /// Snapshot of every request received so far, in call order.
    pub fn received_requests(&self) -> Vec<ChatRequest> {
        self.inner
            .lock()
            .map(|i| i.requests.clone())
            .unwrap_or_default()
    }

    /// Number of `chat` calls made so far.
    pub fn call_count(&self) -> usize {
        self.inner
            .lock()
            .map(|i| i.requests.len())
            .unwrap_or(0)
    }
}

#[async_trait]
impl AIProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AiError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| AiError::Network(format!("mock mutex poisoned: {e}")))?;
        g.requests.push(req);
        if g.responses.len() > 1 {
            Ok(g.responses.remove(0))
        } else if let Some(last) = g.responses.first() {
            Ok(last.clone())
        } else {
            Err(AiError::BadResponse(
                "MockProvider has no scripted responses".into(),
            ))
        }
    }
}
