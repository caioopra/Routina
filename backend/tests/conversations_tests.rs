mod common;

use axum::http::{Method, StatusCode};
use common::{build_app, json_oneshot, register_test_user};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn register_and_token(app: &axum::Router, email: &str) -> String {
    let body = register_test_user(app, email, "longenoughpass").await;
    body["token"].as_str().unwrap().to_string()
}

async fn create_routine(app: &axum::Router, token: &str) -> serde_json::Value {
    let (status, body) = json_oneshot(
        app,
        Method::POST,
        "/api/routines",
        Some(json!({ "name": "Semana Teste" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create_routine failed: {body}");
    body
}

async fn create_conversation(
    app: &axum::Router,
    token: &str,
    routine_id: &str,
    title: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut payload = json!({ "routine_id": routine_id });
    if let Some(t) = title {
        payload["title"] = json!(t);
    }
    json_oneshot(
        app,
        Method::POST,
        "/api/conversations",
        Some(payload),
        Some(token),
    )
    .await
}

// ── Auth enforcement ──────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn auth_required_on_all_endpoints(pool: PgPool) {
    let app = build_app(pool);
    let fake_id = Uuid::now_v7();

    let (status, _) = json_oneshot(&app, Method::GET, "/api/conversations", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/conversations",
        Some(json!({ "routine_id": fake_id })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/conversations/{fake_id}/messages"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ── POST /api/conversations ───────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn create_conversation_happy_path(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "conv-create@example.com").await;
    let routine = create_routine(&app, &token).await;
    let routine_id = routine["id"].as_str().unwrap();

    let (status, body) = create_conversation(&app, &token, routine_id, Some("Planning chat")).await;

    assert_eq!(status, StatusCode::CREATED, "unexpected body: {body}");
    assert_eq!(body["routine_id"].as_str().unwrap(), routine_id);
    assert_eq!(body["title"].as_str().unwrap(), "Planning chat");
    assert!(body["id"].is_string());
    assert!(body["created_at"].is_string());
    // user_id must NOT be exposed.
    assert!(body.get("user_id").is_none() || body["user_id"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn create_conversation_without_title(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "conv-notitle@example.com").await;
    let routine = create_routine(&app, &token).await;
    let routine_id = routine["id"].as_str().unwrap();

    let (status, body) = create_conversation(&app, &token, routine_id, None).await;

    assert_eq!(status, StatusCode::CREATED);
    assert!(body["title"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn create_conversation_wrong_routine_owner(pool: PgPool) {
    let app = build_app(pool);
    let token_a = register_and_token(&app, "conv-a@example.com").await;
    let token_b = register_and_token(&app, "conv-b@example.com").await;

    // User A creates a routine.
    let routine = create_routine(&app, &token_a).await;
    let routine_id = routine["id"].as_str().unwrap();

    // User B tries to create a conversation for user A's routine — must 404.
    let (status, _) = create_conversation(&app, &token_b, routine_id, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_conversation_nonexistent_routine(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "conv-noroute@example.com").await;
    let fake_id = Uuid::now_v7();

    let (status, _) = create_conversation(&app, &token, &fake_id.to_string(), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── GET /api/conversations ────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn list_conversations_empty(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "conv-list-empty@example.com").await;

    let (status, body) =
        json_oneshot(&app, Method::GET, "/api/conversations", None, Some(&token)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_conversations_returns_own_only(pool: PgPool) {
    let app = build_app(pool);
    let token_a = register_and_token(&app, "conv-list-a@example.com").await;
    let token_b = register_and_token(&app, "conv-list-b@example.com").await;

    let routine_a = create_routine(&app, &token_a).await;
    let routine_b = create_routine(&app, &token_b).await;

    create_conversation(&app, &token_a, routine_a["id"].as_str().unwrap(), None).await;
    create_conversation(&app, &token_a, routine_a["id"].as_str().unwrap(), None).await;
    create_conversation(&app, &token_b, routine_b["id"].as_str().unwrap(), None).await;

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        "/api/conversations",
        None,
        Some(&token_a),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let list = body.as_array().unwrap();
    assert_eq!(list.len(), 2, "user A should see exactly 2 conversations");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_conversations_filter_by_routine_id(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "conv-filter@example.com").await;
    let routine1 = create_routine(&app, &token).await;
    let routine2 = create_routine(&app, &token).await;

    create_conversation(
        &app,
        &token,
        routine1["id"].as_str().unwrap(),
        Some("For r1"),
    )
    .await;
    create_conversation(
        &app,
        &token,
        routine1["id"].as_str().unwrap(),
        Some("Also r1"),
    )
    .await;
    create_conversation(
        &app,
        &token,
        routine2["id"].as_str().unwrap(),
        Some("For r2"),
    )
    .await;

    let uri = format!(
        "/api/conversations?routine_id={}",
        routine1["id"].as_str().unwrap()
    );
    let (status, body) = json_oneshot(&app, Method::GET, &uri, None, Some(&token)).await;

    assert_eq!(status, StatusCode::OK);
    let list = body.as_array().unwrap();
    assert_eq!(list.len(), 2);
    for conv in list {
        assert_eq!(
            conv["routine_id"].as_str().unwrap(),
            routine1["id"].as_str().unwrap()
        );
    }
}

// ── GET /api/conversations/:id/messages ──────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn get_messages_empty(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "conv-msgs-empty@example.com").await;
    let routine = create_routine(&app, &token).await;
    let (_, conv) = create_conversation(&app, &token, routine["id"].as_str().unwrap(), None).await;
    let conv_id = conv["id"].as_str().unwrap();

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/conversations/{conv_id}/messages"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_messages_not_found_for_other_user(pool: PgPool) {
    let app = build_app(pool);
    let token_a = register_and_token(&app, "msgs-a@example.com").await;
    let token_b = register_and_token(&app, "msgs-b@example.com").await;

    let routine_a = create_routine(&app, &token_a).await;
    let (_, conv) =
        create_conversation(&app, &token_a, routine_a["id"].as_str().unwrap(), None).await;
    let conv_id = conv["id"].as_str().unwrap();

    // User B tries to read user A's messages — must 404.
    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/conversations/{conv_id}/messages"),
        None,
        Some(&token_b),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_messages_nonexistent_conversation(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "msgs-none@example.com").await;
    let fake_id = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/conversations/{fake_id}/messages"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
