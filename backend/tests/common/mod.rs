#![allow(dead_code)]

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use futures_core::Stream;
use futures_util::stream;
use http_body_util::BodyExt;
use planner_backend::ai::error::ProviderError;
use planner_backend::ai::provider::{
    FinishReason, LlmProvider, Message, ProviderEvent, ToolSchema,
};
use planner_backend::{config::Config, routes};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Test config
// ---------------------------------------------------------------------------

pub fn test_config() -> Config {
    Config {
        database_url: String::new(),
        jwt_secret: "test-secret".to_string(),
        jwt_expiration_hours: 24,
        refresh_token_expiration_days: 30,
        host: "127.0.0.1".to_string(),
        port: 0,
        cors_origin: "http://localhost:5173".to_string(),
        llm_default_provider: "gemini".to_string(),
        llm_gemini_api_key: None,
        llm_gemini_model: String::new(),
        llm_claude_api_key: None,
        llm_claude_model: String::new(),
    }
}

// ---------------------------------------------------------------------------
// MockLlmProvider — simple token-list variant
// ---------------------------------------------------------------------------

/// Deterministic mock provider that returns a canned token sequence then Done.
///
/// Used in integration tests so they never hit a real LLM API.
/// The `captured_messages` field records every call's message slice for
/// assertion purposes (e.g. verifying system prompt injection).
pub struct MockLlmProvider {
    /// Tokens returned in sequence on every `stream_completion` call.
    pub tokens: Vec<String>,
    /// Accumulates the messages passed to each `stream_completion` call.
    /// Wrapped in a mutex so the mock can be shared via `Arc`.
    pub captured_messages: std::sync::Mutex<Vec<Vec<Message>>>,
}

impl MockLlmProvider {
    pub fn new(tokens: Vec<&str>) -> Self {
        Self {
            tokens: tokens.into_iter().map(str::to_owned).collect(),
            captured_messages: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Return an `Arc<Self>` — useful when you want to both inject the provider
    /// into `AppState` and retain a handle for inspecting captured messages.
    pub fn into_shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        _tools: &[ToolSchema],
    ) -> Result<Pin<Box<dyn Stream<Item = ProviderEvent> + Send>>, ProviderError> {
        // Capture a clone of the messages for test assertions.
        self.captured_messages
            .lock()
            .unwrap()
            .push(messages.to_vec());

        let mut events: Vec<ProviderEvent> = self
            .tokens
            .iter()
            .map(|t| ProviderEvent::Token(t.clone()))
            .collect();
        events.push(ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        });
        Ok(Box::pin(stream::iter(events)))
    }
}

// ---------------------------------------------------------------------------
// ScriptedMockProvider — per-round scripted event sequences
// ---------------------------------------------------------------------------

/// A scripted mock provider that returns different event sequences on each call.
///
/// On the first `stream_completion` call it returns `rounds[0]`, on the
/// second it returns `rounds[1]`, etc.  Once all rounds are exhausted it
/// returns a single `Done(Stop)` for any subsequent calls.
pub struct ScriptedMockProvider {
    /// Pre-programmed event sequences, one per invocation.
    pub rounds: std::sync::Mutex<Vec<Vec<ProviderEvent>>>,
    /// Accumulates the messages passed to each `stream_completion` call.
    pub captured_messages: std::sync::Mutex<Vec<Vec<Message>>>,
    /// Provider name returned by `name()`.
    pub provider_name: &'static str,
}

impl ScriptedMockProvider {
    /// Create a new `ScriptedMockProvider` with the given rounds.
    /// `provider_name` defaults to `"mock_scripted"`.
    pub fn new(rounds: Vec<Vec<ProviderEvent>>) -> Self {
        Self {
            rounds: std::sync::Mutex::new(rounds),
            captured_messages: std::sync::Mutex::new(Vec::new()),
            provider_name: "mock_scripted",
        }
    }

    pub fn with_name(mut self, name: &'static str) -> Self {
        self.provider_name = name;
        self
    }

    /// Wrap in `Arc<dyn LlmProvider>` for injection into `AppState`.
    pub fn into_shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[async_trait]
impl LlmProvider for ScriptedMockProvider {
    fn name(&self) -> &'static str {
        self.provider_name
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        _tools: &[ToolSchema],
    ) -> Result<Pin<Box<dyn Stream<Item = ProviderEvent> + Send>>, ProviderError> {
        self.captured_messages
            .lock()
            .unwrap()
            .push(messages.to_vec());

        let mut guard = self.rounds.lock().unwrap();
        let events = if guard.is_empty() {
            vec![ProviderEvent::Done {
                finish_reason: FinishReason::Stop,
                usage: None,
            }]
        } else {
            guard.remove(0)
        };
        Ok(Box::pin(stream::iter(events)))
    }
}

// ---------------------------------------------------------------------------
// App builders
// ---------------------------------------------------------------------------

/// Build app with no LLM provider (chat returns 503).
pub fn build_app(pool: PgPool) -> Router {
    routes::create_router_with_providers(pool, test_config(), HashMap::new())
}

/// Build app with the given mock as the sole provider (keyed as `"mock"`).
pub fn build_app_with_mock(pool: PgPool, mock: Arc<dyn LlmProvider>) -> Router {
    let mut providers = HashMap::new();
    let name = mock.name().to_string();
    providers.insert(name, mock);
    routes::create_router_with_providers(pool, test_config(), providers)
}

/// Build app with a map of named providers — lets tests inject multiple mocks.
pub fn build_app_with_providers(
    pool: PgPool,
    providers: HashMap<String, Arc<dyn LlmProvider>>,
) -> Router {
    routes::create_router_with_providers(pool, test_config(), providers)
}

/// Build app with a single mock provider and a custom per-user chat rate limit.
/// Used to test the 429 path without sending 20+ requests.
pub fn build_app_with_rate_limit(
    pool: PgPool,
    mock: Arc<dyn LlmProvider>,
    chat_rate_limit: usize,
) -> Router {
    let mut providers = HashMap::new();
    let name = mock.name().to_string();
    providers.insert(name, mock);
    routes::create_router_with_rate_limit(pool, test_config(), providers, chat_rate_limit)
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

pub async fn json_oneshot(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    bearer: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);

    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(token) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let request_body = match body {
        Some(v) => Body::from(serde_json::to_vec(&v).unwrap()),
        None => Body::empty(),
    };

    let request = builder.body(request_body).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

/// Send a request and return (status, raw_bytes) — useful for SSE responses.
pub async fn raw_oneshot(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    bearer: Option<&str>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);

    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(token) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let request_body = match body {
        Some(v) => Body::from(serde_json::to_vec(&v).unwrap()),
        None => Body::empty(),
    };

    let request = builder.body(request_body).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, bytes)
}

pub async fn register_test_user(app: &Router, email: &str, password: &str) -> Value {
    let (status, body) = json_oneshot(
        app,
        Method::POST,
        "/api/auth/register",
        Some(json!({ "email": email, "name": "Test", "password": password })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "register failed: {body}");
    body
}

// ---------------------------------------------------------------------------
// SSE parsing helpers
// ---------------------------------------------------------------------------

/// Parse raw SSE bytes into ordered `(event_name, parsed_json_data)` tuples.
///
/// Each SSE frame has the shape:
///
/// ```text
/// event: <name>
/// data: <json>
/// <blank line>
/// ```
///
/// Frames that have no `event:` line are skipped.  The returned `Value` is
/// the parsed JSON value of the `data:` field; if the data is not valid JSON
/// an `Value::Null` is returned rather than panicking.
pub fn parse_sse_body(bytes: &[u8]) -> Vec<(String, Value)> {
    let text = std::str::from_utf8(bytes).expect("SSE body must be UTF-8");
    let mut frames = Vec::new();
    let mut current_event: Option<String> = None;
    let mut current_data: Option<String> = None;

    for line in text.lines() {
        if let Some(ev) = line.strip_prefix("event: ") {
            current_event = Some(ev.to_string());
        } else if let Some(data) = line.strip_prefix("data: ") {
            current_data = Some(data.to_string());
        } else if line.is_empty() {
            if let (Some(ev), Some(data)) = (current_event.take(), current_data.take()) {
                let json_value: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
                frames.push((ev, json_value));
            }
            current_event = None;
            current_data = None;
        }
    }

    // Flush a trailing frame that was not terminated by a blank line.
    if let (Some(ev), Some(data)) = (current_event, current_data) {
        let json_value: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        frames.push((ev, json_value));
    }

    frames
}
