mod common;

use axum::http::{Method, StatusCode};
use common::{build_app, json_oneshot, register_test_user};
use serde_json::json;
use sqlx::PgPool;

async fn register_and_token(app: &axum::Router, email: &str) -> String {
    let body = register_test_user(app, email, "longenoughpass").await;
    body["token"].as_str().unwrap().to_string()
}

// ── GET /api/auth/me — planner_context field ──────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn me_includes_planner_context_null_by_default(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "me-ctx-default@example.com").await;

    let (status, body) = json_oneshot(&app, Method::GET, "/api/auth/me", None, Some(&token)).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    // Newly registered user has no planner_context set.
    assert!(
        body["planner_context"].is_null(),
        "expected planner_context to be null initially, got: {}",
        body["planner_context"]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn me_includes_preferences(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "me-prefs@example.com").await;

    let (status, body) = json_oneshot(&app, Method::GET, "/api/auth/me", None, Some(&token)).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.get("preferences").is_some(),
        "preferences field missing"
    );
}

// ── PUT /api/me/planner-context ───────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn put_planner_context_stores_value(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "me-put@example.com").await;

    let ctx = "Sou engenheiro de software, trabalho das 9h às 18h.";

    let (status, body) = json_oneshot(
        &app,
        Method::PUT,
        "/api/me/planner-context",
        Some(json!({ "planner_context": ctx })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "unexpected: {body}");
    assert_eq!(body["planner_context"].as_str().unwrap(), ctx);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_planner_context_roundtrip_via_me(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "me-roundtrip@example.com").await;

    let ctx = "Objetivo: correr 5km três vezes por semana.";

    // Set the context.
    let (put_status, _) = json_oneshot(
        &app,
        Method::PUT,
        "/api/me/planner-context",
        Some(json!({ "planner_context": ctx })),
        Some(&token),
    )
    .await;
    assert_eq!(put_status, StatusCode::OK);

    // Read back via GET /api/auth/me.
    let (get_status, body) =
        json_oneshot(&app, Method::GET, "/api/auth/me", None, Some(&token)).await;
    assert_eq!(get_status, StatusCode::OK, "{body}");
    assert_eq!(
        body["planner_context"].as_str().unwrap(),
        ctx,
        "planner_context not persisted correctly"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_planner_context_empty_string_clears_to_null(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "me-clear@example.com").await;

    // First set a value.
    json_oneshot(
        &app,
        Method::PUT,
        "/api/me/planner-context",
        Some(json!({ "planner_context": "Something" })),
        Some(&token),
    )
    .await;

    // Now clear it with empty string.
    let (status, body) = json_oneshot(
        &app,
        Method::PUT,
        "/api/me/planner-context",
        Some(json!({ "planner_context": "" })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "unexpected: {body}");
    assert!(
        body["planner_context"].is_null(),
        "expected null after clearing, got: {}",
        body["planner_context"]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_planner_context_whitespace_only_clears_to_null(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "me-ws@example.com").await;

    let (status, body) = json_oneshot(
        &app,
        Method::PUT,
        "/api/me/planner-context",
        Some(json!({ "planner_context": "   \t  " })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "unexpected: {body}");
    assert!(
        body["planner_context"].is_null(),
        "expected null for whitespace-only input"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_planner_context_requires_auth(pool: PgPool) {
    let app = build_app(pool);

    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        "/api/me/planner-context",
        Some(json!({ "planner_context": "test" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn me_requires_auth(pool: PgPool) {
    let app = build_app(pool);

    let (status, _) = json_oneshot(&app, Method::GET, "/api/auth/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_planner_context_response_includes_all_fields(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "me-fields@example.com").await;

    let (status, body) = json_oneshot(
        &app,
        Method::PUT,
        "/api/me/planner-context",
        Some(json!({ "planner_context": "hello" })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.get("id").is_some(), "id missing");
    assert!(body.get("email").is_some(), "email missing");
    assert!(body.get("name").is_some(), "name missing");
    assert!(
        body.get("planner_context").is_some(),
        "planner_context missing"
    );
    assert!(body.get("preferences").is_some(), "preferences missing");
}
