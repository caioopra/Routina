// Integration tests for the admin settings, metrics, and user management endpoints.
//
// Coverage:
//  - GET /api/admin/settings → returns seed values
//  - POST /api/admin/settings → updates a non-sensitive setting
//  - POST /api/admin/settings → sensitive key requires confirm token
//  - GET /api/admin/metrics/usage → returns rows after a chat turn
//  - GET /api/admin/users → returns user list without passwords
//  - POST /api/admin/users/:id/rate-limit → sets per-user override
//  - POST /api/admin/users/:id/rate-limit on unknown user → 404

mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{
    ScriptedMockProvider, build_app, build_app_with_mock, json_oneshot, raw_oneshot,
    register_test_user,
};
use planner_backend::ai::provider::{FinishReason, ProviderEvent, TokenUsage};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn login_token(app: &axum::Router, email: &str) -> String {
    let body = register_test_user(app, email, "longenoughpass").await;
    body["token"].as_str().unwrap().to_string()
}

async fn promote_to_admin(pool: &PgPool, email: &str) {
    sqlx::query("UPDATE users SET role = 'admin' WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await
        .expect("failed to promote user to admin");
}

async fn create_routine(app: &axum::Router, token: &str) -> serde_json::Value {
    let (status, body) = json_oneshot(
        app,
        Method::POST,
        "/api/routines",
        Some(json!({ "name": "Test Routine" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create_routine failed: {body}");
    body
}

// ── GET /api/admin/settings ───────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn settings_list_returns_seed_values(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_email = "admin-settings-list@example.com";
    let token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    let (status, body) =
        json_oneshot(&app, Method::GET, "/api/admin/settings", None, Some(&token)).await;

    assert_eq!(status, StatusCode::OK, "expected 200; got {body}");

    let arr = body.as_array().expect("expected JSON array");
    // Migration 007 seeds 6 keys.
    assert_eq!(
        arr.len(),
        6,
        "expected 6 seeded settings; got {}",
        arr.len()
    );

    // Verify specific seed values.
    let find = |key: &str| arr.iter().find(|r| r["key"].as_str() == Some(key)).cloned();

    let chat_enabled = find("chat_enabled").expect("chat_enabled must be present");
    assert_eq!(chat_enabled["value"], "true");

    let budget = find("budget_monthly_usd").expect("budget_monthly_usd must be present");
    assert_eq!(budget["value"], "5.00");

    let warn_pct = find("budget_warn_pct").expect("budget_warn_pct must be present");
    assert_eq!(warn_pct["value"], "80");
}

#[sqlx::test(migrations = "./migrations")]
async fn settings_list_requires_admin(pool: PgPool) {
    let app = build_app(pool);
    let token = login_token(&app, "nonadmin-settings@example.com").await;

    let (status, _) =
        json_oneshot(&app, Method::GET, "/api/admin/settings", None, Some(&token)).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ── POST /api/admin/settings ──────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn settings_update_with_confirm_token_succeeds(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_email = "admin-settings-update@example.com";
    let token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    // All settings keys are now sensitive (chat_enabled, budget_*, llm_*).
    // Obtain a step-up confirm token first.
    let (confirm_status, confirm_body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/confirm",
        Some(json!({ "password": "longenoughpass", "action": "settings.update" })),
        Some(&token),
    )
    .await;
    assert_eq!(
        confirm_status,
        StatusCode::OK,
        "confirm must succeed; got {confirm_body}"
    );
    let confirm_token = confirm_body["confirm_token"].as_str().unwrap().to_string();

    // Update budget_warn_pct with the confirm token.
    use axum::body::Body as ABody;
    use axum::http::header;
    use axum::http::{Method as HM, Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let request = Request::builder()
        .method(HM::POST)
        .uri("/api/admin/settings")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("x-confirm-token", &confirm_token)
        .body(ABody::from(
            serde_json::to_vec(&json!({ "key": "budget_warn_pct", "value": "90" })).unwrap(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);

    assert_eq!(status, StatusCode::OK, "expected 200; got {body}");
    assert_eq!(body["key"], "budget_warn_pct");
    assert_eq!(body["value"], "90");
    assert!(body["updated_at"].is_string(), "updated_at must be present");

    // Verify the change persisted in the DB.
    let db_value: String =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'budget_warn_pct'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(db_value, "90");
}

#[sqlx::test(migrations = "./migrations")]
async fn settings_update_sensitive_key_without_confirm_token_returns_403(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_email = "admin-settings-no-confirm@example.com";
    let token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    // All budget/chat keys are now sensitive — updating without a confirm
    // token must be rejected.
    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/settings",
        Some(json!({ "key": "budget_warn_pct", "value": "90" })),
        Some(&token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "budget_warn_pct without confirm token must be 403; got {body}"
    );

    // Repeat for the other newly-sensitive keys.
    let (status2, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/settings",
        Some(json!({ "key": "chat_enabled", "value": "false" })),
        Some(&token),
    )
    .await;
    assert_eq!(
        status2,
        StatusCode::FORBIDDEN,
        "chat_enabled without confirm token must be 403"
    );

    let (status3, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/settings",
        Some(json!({ "key": "budget_monthly_usd", "value": "10.00" })),
        Some(&token),
    )
    .await;
    assert_eq!(
        status3,
        StatusCode::FORBIDDEN,
        "budget_monthly_usd without confirm token must be 403"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn settings_update_sensitive_key_requires_confirm_token(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_email = "admin-settings-sensitive@example.com";
    let token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    // Attempt to update llm_default_provider (sensitive) without a confirm token.
    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/settings",
        Some(json!({ "key": "llm_default_provider", "value": "claude" })),
        Some(&token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "sensitive key without confirm token must be 403; got {body}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn settings_update_unknown_key_returns_422(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_email = "admin-settings-bad-key@example.com";
    let token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/settings",
        Some(json!({ "key": "nonexistent_key", "value": "foo" })),
        Some(&token),
    )
    .await;

    // DB CHECK constraint blocks unknown keys; we return 422.
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown key must be 422; got {body}"
    );
}

// ── GET /api/admin/metrics/usage ─────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn metrics_usage_empty_before_any_chat(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_email = "admin-metrics-empty@example.com";
    let token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/metrics/usage",
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.as_array().unwrap().len(),
        0,
        "no usage rows before any chat message"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn metrics_usage_populated_after_chat_with_token_usage(pool: PgPool) {
    // Use a mock provider that reports token usage so the rollup runs.
    let round1 = vec![
        ProviderEvent::Token("Olá!".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: Some(TokenUsage {
                input_tokens: 150,
                output_tokens: 25,
            }),
        },
    ];

    let mock = Arc::new(ScriptedMockProvider::new(vec![round1]).with_name("mock"))
        as Arc<dyn planner_backend::ai::provider::LlmProvider>;

    let app = build_app_with_mock(pool.clone(), mock);
    let user_token = login_token(&app, "user-metrics@example.com").await;
    let routine = create_routine(&app, &user_token).await;

    // Send a chat message to trigger usage rollup.
    let (status, _) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Olá",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&user_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Now query as admin.
    let admin_email = "admin-metrics-after-chat@example.com";
    let admin_token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/metrics/usage",
        None,
        Some(&admin_token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected 200; got {body}");

    let rows = body.as_array().expect("expected array");
    // The mock provider name is "mock" and the config model is empty string.
    // There should be at least one row.
    assert!(
        !rows.is_empty(),
        "expected at least one usage row after chat"
    );

    let row = &rows[0];
    assert!(row["day"].is_string());
    assert!(row["provider"].is_string());
    assert!(row["input_tokens"].as_i64().unwrap() > 0);
    assert!(row["output_tokens"].as_i64().unwrap() > 0);
    assert_eq!(row["request_count"].as_i64().unwrap(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn metrics_usage_days_param_clamps_to_90(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_email = "admin-metrics-days@example.com";
    let token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    // days=200 should be clamped to 90 without error.
    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/metrics/usage?days=200",
        None,
        Some(&token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "clamped days must return 200; got {body}"
    );
}

// ── GET /api/admin/users ─────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn users_list_returns_all_users(pool: PgPool) {
    let app = build_app(pool.clone());

    // Register some users.
    let _ = login_token(&app, "user-list-a@example.com").await;
    let _ = login_token(&app, "user-list-b@example.com").await;

    // Promote one to admin.
    let admin_email = "admin-users-list@example.com";
    let admin_token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/users",
        None,
        Some(&admin_token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected 200; got {body}");

    let users = body.as_array().expect("expected array");
    // There should be at least 3 users (two regular + one admin).
    assert!(
        users.len() >= 3,
        "expected at least 3 users; got {}",
        users.len()
    );

    // Verify no password fields are present.
    for user in users {
        assert!(
            user.get("password_hash").is_none(),
            "password_hash must not be in response"
        );
        assert!(
            user.get("password").is_none(),
            "password must not be in response"
        );
        // Required fields must be present.
        assert!(user["id"].is_string());
        assert!(user["email"].is_string());
        assert!(user["role"].is_string());
        assert!(user["created_at"].is_string());
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn users_list_requires_admin(pool: PgPool) {
    let app = build_app(pool);
    let token = login_token(&app, "regular-user-list@example.com").await;

    let (status, _) = json_oneshot(&app, Method::GET, "/api/admin/users", None, Some(&token)).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ── POST /api/admin/users/:id/rate-limit ─────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn users_set_rate_limit_succeeds(pool: PgPool) {
    let app = build_app(pool.clone());

    let target_email = "target-rl@example.com";
    let target_body = register_test_user(&app, target_email, "longenoughpass").await;
    let target_id = target_body["user"]["id"].as_str().unwrap().to_string();

    let admin_email = "admin-rl-set@example.com";
    let admin_token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        &format!("/api/admin/users/{target_id}/rate-limit"),
        Some(json!({
            "daily_token_limit": 50000,
            "daily_request_limit": 10,
            "override_reason": "test override"
        })),
        Some(&admin_token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected 200; got {body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["daily_token_limit"], 50000);
    assert_eq!(body["daily_request_limit"], 10);

    // Verify DB row was upserted.
    let db_row = sqlx::query!(
        "SELECT daily_token_limit, daily_request_limit, override_reason \
         FROM user_rate_limits WHERE user_id = $1",
        target_id.parse::<Uuid>().unwrap()
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(db_row.daily_token_limit, Some(50000));
    assert_eq!(db_row.daily_request_limit, Some(10));
    assert_eq!(db_row.override_reason.as_deref(), Some("test override"));
}

#[sqlx::test(migrations = "./migrations")]
async fn users_set_rate_limit_unknown_user_returns_404(pool: PgPool) {
    let app = build_app(pool.clone());

    let admin_email = "admin-rl-404@example.com";
    let admin_token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    let nonexistent_id = Uuid::now_v7();

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        &format!("/api/admin/users/{nonexistent_id}/rate-limit"),
        Some(json!({
            "daily_token_limit": 1000,
            "daily_request_limit": null,
            "override_reason": null
        })),
        Some(&admin_token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "unknown user must be 404; got {body}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn users_set_rate_limit_upserts_existing_row(pool: PgPool) {
    let app = build_app(pool.clone());

    let target_email = "target-rl-upsert@example.com";
    let target_body = register_test_user(&app, target_email, "longenoughpass").await;
    let target_id = target_body["user"]["id"].as_str().unwrap().to_string();

    let admin_email = "admin-rl-upsert@example.com";
    let admin_token = login_token(&app, admin_email).await;
    promote_to_admin(&pool, admin_email).await;

    let url = format!("/api/admin/users/{target_id}/rate-limit");

    // First set.
    json_oneshot(
        &app,
        Method::POST,
        &url,
        Some(json!({ "daily_token_limit": 1000, "daily_request_limit": null, "override_reason": "first" })),
        Some(&admin_token),
    )
    .await;

    // Second set — should update.
    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        &url,
        Some(json!({ "daily_token_limit": 9999, "daily_request_limit": 5, "override_reason": "second" })),
        Some(&admin_token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "upsert must succeed; got {body}");

    let db_row = sqlx::query!(
        "SELECT daily_token_limit FROM user_rate_limits WHERE user_id = $1",
        target_id.parse::<Uuid>().unwrap()
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        db_row.daily_token_limit,
        Some(9999),
        "upsert must update existing row"
    );
}
