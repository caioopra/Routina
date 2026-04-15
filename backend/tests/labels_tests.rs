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

async fn create_label(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    json_oneshot(app, Method::POST, "/api/labels", Some(body), Some(token)).await
}

fn default_label_body() -> Value {
    json!({
        "name": "Custom Label",
        "color_bg": "#1e1836",
        "color_text": "#c4b5fd",
        "color_border": "#7c3aed",
        "icon": "star"
    })
}

// ---------------------------------------------------------------------------
// List labels
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_labels_requires_auth(pool: PgPool) {
    let app = build_app(pool);

    let (status, _) = json_oneshot(&app, Method::GET, "/api/labels", None, None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_labels_returns_default_labels_for_new_user(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "list-labels-defaults@example.com").await;

    let (status, body) = json_oneshot(&app, Method::GET, "/api/labels", None, Some(&token)).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    // Registration seeds 7 default labels.
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 7);
    // All are marked as default.
    assert!(arr.iter().all(|l| l["is_default"] == true));
}

// ---------------------------------------------------------------------------
// Create label
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn create_label_happy_path(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-label-happy@example.com").await;

    let (status, body) = create_label(&app, &token, default_label_body()).await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert!(Uuid::parse_str(body["id"].as_str().unwrap()).is_ok());
    assert_eq!(body["name"], "Custom Label");
    assert_eq!(body["color_bg"], "#1e1836");
    assert_eq!(body["color_text"], "#c4b5fd");
    assert_eq!(body["color_border"], "#7c3aed");
    assert_eq!(body["icon"], "star");
    assert_eq!(body["is_default"], false);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_label_without_icon(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-label-noicon@example.com").await;

    let (status, body) = create_label(
        &app,
        &token,
        json!({
            "name": "No Icon",
            "color_bg": "#000",
            "color_text": "#fff",
            "color_border": "#888"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert!(body["icon"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn create_label_empty_name_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-label-emptyname@example.com").await;

    let (status, _) = create_label(
        &app,
        &token,
        json!({
            "name": "  ",
            "color_bg": "#000",
            "color_text": "#fff",
            "color_border": "#888"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_label_duplicate_name_returns_409(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "create-label-dup@example.com").await;

    let (s1, _) = create_label(&app, &token, default_label_body()).await;
    assert_eq!(s1, StatusCode::CREATED);

    // Second label with same name -> 409.
    let (s2, body) = create_label(&app, &token, default_label_body()).await;
    assert_eq!(s2, StatusCode::CONFLICT, "body: {body}");
}

// ---------------------------------------------------------------------------
// Update label
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn update_label_happy_path(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-label@example.com").await;

    let (_, created) = create_label(&app, &token, default_label_body()).await;
    let id = created["id"].as_str().unwrap();

    let (status, body) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/labels/{id}"),
        Some(json!({
            "name": "Renamed Label",
            "color_bg": "#111"
        })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["name"], "Renamed Label");
    assert_eq!(body["color_bg"], "#111");
    // Unchanged fields.
    assert_eq!(body["color_text"], "#c4b5fd");
    assert_eq!(body["color_border"], "#7c3aed");
}

#[sqlx::test(migrations = "./migrations")]
async fn update_label_empty_name_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-label-emptyname@example.com").await;

    let (_, created) = create_label(&app, &token, default_label_body()).await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/labels/{id}"),
        Some(json!({ "name": "" })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_label_unknown_id_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "update-label-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/labels/{fake}"),
        Some(json!({ "name": "Ghost" })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Delete label
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn delete_label_happy_path(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "delete-label@example.com").await;

    let (_, created) = create_label(&app, &token, default_label_body()).await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/labels/{id}"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_label_unknown_id_returns_404(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "delete-label-404@example.com").await;
    let fake = Uuid::now_v7();

    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/labels/{fake}"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_label_twice_returns_404_second_time(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "delete-label-twice@example.com").await;

    let (_, created) = create_label(&app, &token, default_label_body()).await;
    let id = created["id"].as_str().unwrap();

    let (s1, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/labels/{id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(s1, StatusCode::NO_CONTENT);

    let (s2, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/labels/{id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(s2, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Default label protection
// ---------------------------------------------------------------------------

/// Registration seeds 7 default labels. Verify that those seeded default labels
/// cannot be deleted via the REST API (returns 409 Conflict).
#[sqlx::test(migrations = "./migrations")]
async fn default_label_cannot_be_deleted(pool: PgPool) {
    let pool2 = pool.clone();
    let app = build_app(pool);
    let body = register_test_user(&app, "default-label-delete@example.com", "longenoughpass").await;
    let token = body["token"].as_str().unwrap().to_string();
    let user_id: uuid::Uuid = uuid::Uuid::parse_str(body["user"]["id"].as_str().unwrap()).unwrap();

    // Fetch one of the seeded default label IDs directly from the DB.
    let (label_id,): (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM labels WHERE user_id = $1 AND is_default = true LIMIT 1")
            .bind(user_id)
            .fetch_one(&pool2)
            .await
            .expect("should have a default label");

    // DELETE should return 409.
    let (status, resp_body) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/labels/{label_id}"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {resp_body}");
}

/// Tests `is_default` protection using a pool that we retain after passing to build_app.
#[sqlx::test(migrations = "./migrations")]
async fn default_label_with_pool_cannot_be_deleted(pool: PgPool) {
    let pool2 = pool.clone();
    let app = build_app(pool);
    let token = register_and_token(&app, "default-label-pool@example.com").await;

    // Create a label via the API first, then flip is_default via direct SQL.
    let (_, created) = create_label(&app, &token, default_label_body()).await;
    let label_id: uuid::Uuid = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    sqlx::query("UPDATE labels SET is_default = true WHERE id = $1")
        .bind(label_id)
        .execute(&pool2)
        .await
        .expect("set is_default");

    // DELETE should now return 409.
    let (status, body) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/labels/{label_id}"),
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
}

#[sqlx::test(migrations = "./migrations")]
async fn default_label_with_pool_cannot_be_updated(pool: PgPool) {
    let pool2 = pool.clone();
    let app = build_app(pool);
    let token = register_and_token(&app, "default-label-update-pool@example.com").await;

    let (_, created) = create_label(&app, &token, default_label_body()).await;
    let label_id: uuid::Uuid = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    sqlx::query("UPDATE labels SET is_default = true WHERE id = $1")
        .bind(label_id)
        .execute(&pool2)
        .await
        .expect("set is_default");

    // PUT should now return 409.
    let (status, body) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/labels/{label_id}"),
        Some(json!({ "name": "Renamed" })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
}

// ---------------------------------------------------------------------------
// Cross-user isolation
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn cross_user_isolation_labels(pool: PgPool) {
    let app = build_app(pool);
    let token1 = register_and_token(&app, "label-user1@example.com").await;
    let token2 = register_and_token(&app, "label-user2@example.com").await;

    // User 1 creates a label.
    let (_, created) = create_label(&app, &token1, default_label_body()).await;
    let id = created["id"].as_str().unwrap();

    // User 2 cannot see user 1's custom label in their list.
    // User 2 has their own 7 default labels, but NOT user 1's custom label.
    let (status, body) = json_oneshot(&app, Method::GET, "/api/labels", None, Some(&token2)).await;
    assert_eq!(status, StatusCode::OK);
    let user2_labels = body.as_array().unwrap();
    assert!(
        !user2_labels.iter().any(|l| l["id"] == id),
        "user 2 should not see user 1's custom label"
    );

    // User 2 cannot update it -> 404.
    let (status, _) = json_oneshot(
        &app,
        Method::PUT,
        &format!("/api/labels/{id}"),
        Some(json!({ "name": "Hacked" })),
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // User 2 cannot delete it -> 404.
    let (status, _) = json_oneshot(
        &app,
        Method::DELETE,
        &format!("/api/labels/{id}"),
        None,
        Some(&token2),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // User 1's custom label still exists (7 defaults + 1 custom = 8 total).
    let (status, body) = json_oneshot(&app, Method::GET, "/api/labels", None, Some(&token1)).await;
    assert_eq!(status, StatusCode::OK);
    let user1_labels = body.as_array().unwrap();
    assert_eq!(user1_labels.len(), 8, "7 defaults + 1 custom");
    assert!(
        user1_labels.iter().any(|l| l["id"] == id),
        "user 1's custom label must still be present"
    );
}
