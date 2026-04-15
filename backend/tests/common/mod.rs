#![allow(dead_code)]

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
// MockLlmProvider
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
        });
        Ok(Box::pin(stream::iter(events)))
    }
}

// ---------------------------------------------------------------------------
// App builders
// ---------------------------------------------------------------------------

/// Build app with no LLM provider (chat returns 503).
pub fn build_app(pool: PgPool) -> Router {
    routes::create_router_with_provider(pool, test_config(), None)
}

/// Build app with the given `MockLlmProvider`.
pub fn build_app_with_mock(pool: PgPool, mock: Arc<dyn LlmProvider>) -> Router {
    routes::create_router_with_provider(pool, test_config(), Some(mock))
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
