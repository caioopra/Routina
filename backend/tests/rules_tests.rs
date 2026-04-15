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

async fn create_rule(
    app: &Router,
    token: &str,
    routine_id: &str,
    body: Value,
) -> (StatusCode, Value) {
    json_oneshot(
        app,
        Method::POST,
        &format!("/api/routines/{routine_id}/rules"),
        Some(body),
        Some(token),
    )
    .await
}

// ---------------------------------------------------------------------------
// List rules
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_rules_requires_auth(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "list-rules-auth@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/rules"),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_rules_empty_for_new_routine(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "list-rules-empty@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/rules"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(body.as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn list_rules_unknown_routine_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "list-rules-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{fake}/rules"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Create rule
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn create_rule_happy_path(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-rule-happy@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, body) = create_rule(
        &app,
        &token,
        &routine_id,
        json!({ "text": "No meetings before 10am", "sort_order": 1 }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert!(Uuid::parse_str(body["id"].as_str().unwrap()).is_ok());
    assert_eq!(body["text"], "No meetings before 10am");
    assert_eq!(body["sort_order"], 1);
    assert_eq!(body["routine_id"].as_str().unwrap(), routine_id);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rule_without_sort_order_defaults_to_zero(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-rule-default-order@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, body) = create_rule(
        &app,
        &token,
        &routine_id,
        json!({ "text": "Exercise daily" }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(body["sort_order"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rule_empty_text_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-rule-empty@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, _) = create_rule(&app, &token, &routine_id, json!({ "text": "  " })).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rule_unknown_routine_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-rule-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = create_rule(
        &app,
        &token,
        &fake.to_string(),
        json!({ "text": "Ghost rule" }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Update rule
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn update_rule_happy_path(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-rule@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (_, created) = create_rule(
        &app,
        &token,
        &routine_id,
        json!({ "text": "Original", "sort_order": 0 }),
    )
    .await;
    let rule_id = created["id"].as_str().unwrap();

    let (status, body) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/rules/{rule_id}"),
        Some(json!({ "text": "Updated rule", "sort_order": 5 })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["text"], "Updated rule");
    assert_eq!(body["sort_order"], 5);
    assert_eq!(body["id"], rule_id);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_rule_partial_update_preserves_other_fields(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-rule-partial@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (_, created) = create_rule(
        &app,
        &token,
        &routine_id,
        json!({ "text": "Keep this text", "sort_order": 3 }),
    )
    .await;
    let rule_id = created["id"].as_str().unwrap();

    let (status, body) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/rules/{rule_id}"),
        Some(json!({ "sort_order": 10 })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["text"], "Keep this text");
    assert_eq!(body["sort_order"], 10);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_rule_empty_text_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-rule-empty@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (_, created) = create_rule(&app, &token, &routine_id, json!({ "text": "Original" })).await;
    let rule_id = created["id"].as_str().unwrap();

    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/rules/{rule_id}"),
        Some(json!({ "text": "" })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_rule_unknown_id_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-rule-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/rules/{fake}"),
        Some(json!({ "text": "Ghost" })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Delete rule
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn delete_rule_happy_path(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "delete-rule@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (_, created) = create_rule(&app, &token, &routine_id, json!({ "text": "To delete" })).await;
    let rule_id = created["id"].as_str().unwrap();

    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/rules/{rule_id}"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_rule_twice_returns_404_second_time(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "delete-rule-twice@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (_, created) =
        create_rule(&app, &token, &routine_id, json!({ "text": "Doomed rule" })).await;
    let rule_id = created["id"].as_str().unwrap();

    let (s1, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/rules/{rule_id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(s1, StatusCode::NO_CONTENT);

    let (s2, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/rules/{rule_id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(s2, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_rule_unknown_id_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "delete-rule-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/rules/{fake}"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Length bounds
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn create_rule_text_too_long_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-rule-longtext@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (status, _) = create_rule(
        &app,
        &token,
        &routine_id,
        json!({ "text": "a".repeat(2001) }),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_rule_text_too_long_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-rule-longtext@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    let (_, created) =
        create_rule(&app, &token, &routine_id, json!({ "text": "Short text" })).await;
    let rule_id = created["id"].as_str().unwrap();

    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/rules/{rule_id}"),
        Some(json!({ "text": "a".repeat(2001) })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

// ---------------------------------------------------------------------------
// List rules — sort order
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_rules_sorted_by_sort_order(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "rules-sorted@example.com").await;
    let routine_id = create_routine(&app, &token).await;

    // Insert in reverse order.
    for (text, order) in [("Third", 3), ("First", 1), ("Second", 2)] {
        let (s, _) = create_rule(
            &app,
            &token,
            &routine_id,
            json!({ "text": text, "sort_order": order }),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
    }

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/rules"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0]["text"], "First");
    assert_eq!(arr[1]["text"], "Second");
    assert_eq!(arr[2]["text"], "Third");
}

// ---------------------------------------------------------------------------
// Cross-user isolation
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn cross_user_isolation_rules(pool: PgPool) {
    let app = build_app(pool);
    let token1 = register_and_token(&app, "rule-user1@example.com").await;
    let token2 = register_and_token(&app, "rule-user2@example.com").await;

    // User 1 creates a routine and rule.
    let routine_id = create_routine(&app, &token1).await;
    let (_, created) =
        create_rule(&app, &token1, &routine_id, json!({ "text": "User 1 rule" })).await;
    let rule_id = created["id"].as_str().unwrap();

    // User 2 cannot list rules for user 1's routine.
    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/rules"),
        None,
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // User 2 cannot update user 1's rule.
    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/rules/{rule_id}"),
        Some(json!({ "text": "Hacked" })),
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // User 2 cannot delete user 1's rule.
    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/rules/{rule_id}"),
        None,
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // User 1's rule is still intact.
    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{routine_id}/rules"),
        None,
        Some(&token1),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["text"], "User 1 rule");
}
