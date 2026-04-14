#![allow(dead_code)]

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use planner_backend::{config::Config, routes};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

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

pub fn build_app(pool: PgPool) -> Router {
    routes::create_router(pool, test_config())
}

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

pub async fn register_test_user(app: &Router, email: &str, password: &str) -> Value {
    let (status, body) = json_oneshot(
        app,
        Method::POST,
        "/auth/register",
        Some(json!({ "email": email, "name": "Test", "password": password })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "register failed: {body}");
    body
}
