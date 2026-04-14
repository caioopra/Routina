mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{build_app, json_oneshot, register_test_user};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn register_and_token(app: &Router, email: &str) -> String {
    let body = register_test_user(app, email, "longenoughpass").await;
    body["token"].as_str().unwrap().to_string()
}

async fn create_routine(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    json_oneshot(app, Method::POST, "/api/routines", Some(body), Some(token)).await
}

#[sqlx::test(migrations = "./migrations")]
async fn auth_required_on_all_endpoints(pool: PgPool) {
    let app = build_app(pool);
    let fake_id = Uuid::now_v7();

    let (status, _) = json_oneshot(&app, Method::GET, "/api/routines", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/routines",
        Some(json!({ "name": "x" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{fake_id}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        &format!("/api/routines/{fake_id}/activate"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/routines/{fake_id}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_routine_happy_path(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-happy@example.com").await;

    let (status, body) = create_routine(
        &app,
        &token,
        json!({
            "name": "Weekly",
            "period": "weekly",
            "meta": { "note": "first" }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert!(Uuid::parse_str(body["id"].as_str().unwrap()).is_ok());
    assert_eq!(body["is_active"], true);
    assert_eq!(body["name"], "Weekly");
    assert_eq!(body["period"], "weekly");
    assert_eq!(body["meta"]["note"], "first");
}

#[sqlx::test(migrations = "./migrations")]
async fn create_routine_without_optional_fields(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-min@example.com").await;

    let (status, body) = create_routine(&app, &token, json!({ "name": "Only Name" })).await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(body["name"], "Only Name");
    assert!(body["period"].is_null());
    assert_eq!(body["meta"], json!({}));
    assert_eq!(body["is_active"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_routine_with_empty_name_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-empty@example.com").await;

    let (status, _) = create_routine(&app, &token, json!({ "name": "   " })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_routines_returns_caller_items(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "list@example.com").await;

    let (s1, _) = create_routine(&app, &token, json!({ "name": "A" })).await;
    assert_eq!(s1, StatusCode::CREATED);
    let (s2, _) = create_routine(&app, &token, json!({ "name": "B" })).await;
    assert_eq!(s2, StatusCode::CREATED);

    let (status, body) = json_oneshot(&app, Method::GET, "/api/routines", None, Some(&token)).await;

    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().expect("expected array");
    assert_eq!(arr.len(), 2);
    let names: Vec<&str> = arr.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"A"));
    assert!(names.contains(&"B"));
}

#[sqlx::test(migrations = "./migrations")]
async fn get_routine_by_id_returns_full_shape(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "get-shape@example.com").await;

    let (_, created) =
        create_routine(&app, &token, json!({ "name": "Shape", "period": "weekly" })).await;
    let id = created["id"].as_str().unwrap();

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{id}"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);
    assert_eq!(body["name"], "Shape");
    assert_eq!(body["period"], "weekly");
    assert_eq!(body["is_active"], true);
    assert_eq!(body["blocks"], json!([]));
    assert_eq!(body["rules"], json!([]));
    assert_eq!(body["summary"], json!([]));
    assert!(body["created_at"].is_string());
    assert!(body["updated_at"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn get_routine_by_unknown_id_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "get-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{fake}"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_routine_changes_fields_and_ignores_is_active(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update@example.com").await;

    let (_, created) = create_routine(
        &app,
        &token,
        json!({ "name": "Old", "period": "weekly", "meta": { "v": 1 } }),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["is_active"], true);

    // Client tries to send is_active: false — should be ignored by PUT.
    let (status, body) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/routines/{id}"),
        Some(json!({
            "name": "New",
            "period": "biweekly",
            "meta": { "v": 2 },
            "is_active": false
        })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["name"], "New");
    assert_eq!(body["period"], "biweekly");
    assert_eq!(body["meta"]["v"], 2);
    assert_eq!(
        body["is_active"], true,
        "PUT must not change is_active field"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn update_routine_unknown_id_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/routines/{fake}"),
        Some(json!({ "name": "New" })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn activate_routine_deactivates_previous_active(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "activate@example.com").await;

    let (_, a) = create_routine(&app, &token, json!({ "name": "A" })).await;
    let a_id = a["id"].as_str().unwrap();
    assert_eq!(a["is_active"], true);

    let (_, b) = create_routine(&app, &token, json!({ "name": "B" })).await;
    let b_id = b["id"].as_str().unwrap();
    assert_eq!(b["is_active"], false);

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        &format!("/api/routines/{b_id}/activate"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["is_active"], true);
    assert_eq!(body["id"], b_id);

    // Assert A is now inactive and B is active via GET.
    let (_, a_after) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{a_id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(a_after["is_active"], false);

    let (_, b_after) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{b_id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(b_after["is_active"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn activate_unknown_id_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "activate-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        &format!("/api/routines/{fake}/activate"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_routine_returns_204_and_then_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "delete@example.com").await;

    let (_, created) = create_routine(&app, &token, json!({ "name": "Doomed" })).await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/routines/{id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Deleting again should also be 404.
    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/routines/{id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn cross_user_isolation(pool: PgPool) {
    let app = build_app(pool);
    let token1 = register_and_token(&app, "user1-iso@example.com").await;
    let token2 = register_and_token(&app, "user2-iso@example.com").await;

    // user1 creates a routine
    let (_, created) = create_routine(&app, &token1, json!({ "name": "Mine" })).await;
    let id = created["id"].as_str().unwrap();

    // user2 cannot see it in list
    let (status, body) =
        json_oneshot(&app, Method::GET, "/api/routines", None, Some(&token2)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);

    // user2 GET by id -> 404
    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{id}"),
        None,
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // user2 PUT -> 404
    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/routines/{id}"),
        Some(json!({ "name": "Hacked" })),
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // user2 activate -> 404
    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        &format!("/api/routines/{id}/activate"),
        None,
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // user2 DELETE -> 404
    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/routines/{id}"),
        None,
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // And user1's routine still exists untouched.
    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        &format!("/api/routines/{id}"),
        None,
        Some(&token1),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Mine");
}
