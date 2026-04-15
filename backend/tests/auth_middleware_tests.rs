// Integration tests for the router-level `auth_middleware`.
//
// These tests verify that the protected sub-router rejects requests without a
// valid JWT before any handler is invoked — the defence-in-depth guarantee.

mod common;

use axum::http::{Method, StatusCode};
use common::{build_app_with_mock, json_oneshot, register_test_user};
use serde_json::json;
use sqlx::PgPool;

fn make_mock() -> std::sync::Arc<dyn planner_backend::ai::provider::LlmProvider> {
    common::MockLlmProvider::new(vec!["ok"]).into_shared()
}

// ── Public routes are still accessible without a token ───────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn public_auth_register_reachable_without_token(pool: PgPool) {
    let app = build_app_with_mock(pool, make_mock());

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/register",
        Some(json!({
            "email": "auth-mw-public@example.com",
            "name": "Test",
            "password": "longenoughpass"
        })),
        None,
    )
    .await;

    // 200 or 409 (email conflict) are both "reached the handler" — not 401.
    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "register must not require auth"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn public_auth_login_reachable_without_token(pool: PgPool) {
    let app = build_app_with_mock(pool.clone(), make_mock());

    // Register first so the login attempt reaches the handler.
    register_test_user(&app, "auth-mw-login@example.com", "longenoughpass").await;

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(json!({
            "email": "auth-mw-login@example.com",
            "password": "longenoughpass"
        })),
        None,
    )
    .await;

    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "login must not require auth"
    );
}

// ── Protected routes reject requests without a token ─────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn protected_routines_rejects_no_token(pool: PgPool) {
    let app = build_app_with_mock(pool, make_mock());

    let (status, _) = json_oneshot(&app, Method::GET, "/api/routines", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn protected_labels_rejects_no_token(pool: PgPool) {
    let app = build_app_with_mock(pool, make_mock());

    let (status, _) = json_oneshot(&app, Method::GET, "/api/labels", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn protected_conversations_rejects_no_token(pool: PgPool) {
    let app = build_app_with_mock(pool, make_mock());

    let (status, _) = json_oneshot(&app, Method::GET, "/api/conversations", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn protected_chat_rejects_no_token(pool: PgPool) {
    use uuid::Uuid;
    let app = build_app_with_mock(pool, make_mock());

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({ "message": "hi", "routine_id": Uuid::now_v7() })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn protected_me_rejects_no_token(pool: PgPool) {
    let app = build_app_with_mock(pool, make_mock());

    let (status, _) = json_oneshot(&app, Method::GET, "/api/me/planner-context", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn protected_settings_rejects_no_token(pool: PgPool) {
    let app = build_app_with_mock(pool, make_mock());

    let (status, _) =
        json_oneshot(&app, Method::GET, "/api/settings/llm-provider", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ── Protected routes accept valid tokens ─────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn protected_routines_accepts_valid_token(pool: PgPool) {
    let app = build_app_with_mock(pool, make_mock());

    let body = register_test_user(&app, "auth-mw-valid@example.com", "longenoughpass").await;
    let token = body["token"].as_str().unwrap();

    let (status, _) = json_oneshot(&app, Method::GET, "/api/routines", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
}

// ── Expired/tampered token is rejected ───────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn protected_route_rejects_tampered_token(pool: PgPool) {
    let app = build_app_with_mock(pool, make_mock());

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        "/api/routines",
        None,
        Some("not.a.valid.jwt"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
