mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{build_app, json_oneshot, register_test_user};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn register_and_token(app: &Router, email: &str) -> String {
    let body = register_test_user(app, email, "longenoughpass").await;
    body["token"].as_str().unwrap().to_string()
}

async fn create_routine(app: &Router, token: &str) -> String {
    let (status, body) = json_oneshot(
        app,
        Method::POST,
        "/api/routines",
        Some(json!({ "name": "Test Routine" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create_routine failed: {body}");
    body["id"].as_str().unwrap().to_string()
}

async fn create_block(
    app: &Router,
    token: &str,
    routine_id: &str,
    body: Value,
) -> (StatusCode, Value) {
    json_oneshot(
        app,
        Method::POST,
        &format!("/api/routines/{routine_id}/blocks"),
        Some(body),
        Some(token),
    )
    .await
}

fn default_block_body() -> Value {
    json!({
        "day_of_week": 1,
        "start_time": "09:00",
        "end_time": "10:00",
        "title": "Morning Work",
        "type": "trabalho",
        "note": "Focus time",
        "sort_order": 0
    })
}

// ---------------------------------------------------------------------------
// List blocks
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_blocks_empty_for_new_routine(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "list-blocks-empty@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/blocks"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_blocks_requires_auth(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "list-blocks-auth@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/blocks"),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_blocks_unknown_routine_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "list-blocks-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{fake}/blocks"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Create block
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn create_block_happy_path(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-block-happy@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, body) = create_block(&app, &token, &routine_id, default_block_body()).await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert!(Uuid::parse_str(body["id"].as_str().unwrap()).is_ok());
    assert_eq!(body["day_of_week"], 1);
    assert_eq!(body["start_time"], "09:00");
    assert_eq!(body["end_time"], "10:00");
    assert_eq!(body["title"], "Morning Work");
    assert_eq!(body["type"], "trabalho");
    assert_eq!(body["note"], "Focus time");
    assert_eq!(body["sort_order"], 0);
    // Response must include labels (empty) and subtasks (empty).
    assert_eq!(body["labels"], json!([]));
    assert_eq!(body["subtasks"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_without_optional_fields(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-block-min@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, body) = create_block(
        &app,
        &token,
        &routine_id,
        json!({
            "day_of_week": 0,
            "start_time": "08:00",
            "title": "Stand-up",
            "type": "trabalho"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert!(body["end_time"].is_null());
    assert!(body["note"].is_null());
    assert_eq!(body["sort_order"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_invalid_day_of_week_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-block-badday@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, _) = create_block(
        &app,
        &token,
        &routine_id,
        json!({
            "day_of_week": 7,
            "start_time": "09:00",
            "title": "Bad day",
            "type": "trabalho"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_invalid_time_format_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-block-badtime@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, _) = create_block(
        &app,
        &token,
        &routine_id,
        json!({
            "day_of_week": 1,
            "start_time": "9am",
            "title": "Bad time",
            "type": "trabalho"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_empty_title_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-block-emptytitle@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, _) = create_block(
        &app,
        &token,
        &routine_id,
        json!({
            "day_of_week": 1,
            "start_time": "09:00",
            "title": "  ",
            "type": "trabalho"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

// ---------------------------------------------------------------------------
// Day filter
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn day_filter_returns_only_matching_blocks(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "day-filter@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    // Create blocks on day 0, 1, and 2.
    for day in 0i64..3 {
        let (s, _) = create_block(
            &app,
            &token,
            &routine_id,
            json!({
                "day_of_week": day,
                "start_time": "09:00",
                "title": format!("Block day {day}"),
                "type": "trabalho"
            }),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
    }

    // Filter for day 1 only.
    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/blocks?day=1"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["day_of_week"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn day_filter_invalid_value_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "day-filter-invalid@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/blocks?day=7"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

// ---------------------------------------------------------------------------
// Update block
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn update_block_happy_path(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-block@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (_, created) = create_block(&app, &token, &routine_id, default_block_body()).await;
    let block_id = created["id"].as_str().unwrap();

    let (status, body) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/blocks/{block_id}"),
        Some(json!({
            "title": "Updated Work",
            "start_time": "10:00",
            "sort_order": 5
        })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["title"], "Updated Work");
    assert_eq!(body["start_time"], "10:00");
    assert_eq!(body["sort_order"], 5);
    // Unchanged fields.
    assert_eq!(body["day_of_week"], 1);
    assert_eq!(body["type"], "trabalho");
}

#[sqlx::test(migrations = "./migrations")]
async fn update_block_invalid_day_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-block-badday@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (_, created) = create_block(&app, &token, &routine_id, default_block_body()).await;
    let block_id = created["id"].as_str().unwrap();

    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/blocks/{block_id}"),
        Some(json!({ "day_of_week": 8 })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_block_unknown_id_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-block-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/blocks/{fake}"),
        Some(json!({ "title": "Ghost" })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Delete block
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn delete_block_returns_204_and_then_gone(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "delete-block@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (_, created) = create_block(&app, &token, &routine_id, default_block_body()).await;
    let block_id = created["id"].as_str().unwrap();

    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/blocks/{block_id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Second DELETE must be 404.
    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/blocks/{block_id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_block_unknown_id_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "delete-block-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/blocks/{fake}"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Cross-user isolation
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn cross_user_isolation_blocks(pool: PgPool) {
    let app = build_app(pool);
    let token1 = register_and_token(&app, "block-user1@example.com").await;
    let token2 = register_and_token(&app, "block-user2@example.com").await;

    // User 1 creates a routine and block.
    let routine_id = create_routine(&app, &token1).await;
    let (_, created) = create_block(&app, &token1, &routine_id, default_block_body()).await;
    let block_id = created["id"].as_str().unwrap();

    // User 2 tries to list blocks for user 1's routine -> 404.
    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/blocks"),
        None,
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // User 2 tries to update user 1's block -> 404.
    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/blocks/{block_id}"),
        Some(json!({ "title": "Hacked" })),
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // User 2 tries to delete user 1's block -> 404.
    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/blocks/{block_id}"),
        None,
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // User 1's block is still intact.
    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/blocks"),
        None,
        Some(&token1),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
}
