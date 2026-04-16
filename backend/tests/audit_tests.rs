// Integration tests for Phase 3 Slice B: audit system + step-up auth.
//
// Covers:
//   1. POST /api/admin/confirm — unauthenticated → 401/403
//   2. POST /api/admin/confirm — admin + wrong password → 401
//   3. POST /api/admin/confirm — admin + correct password → 200 + confirm_token
//   4. Confirm token for wrong action → 403
//   5. POST /api/auth/login success → audit_log has auth.login.success row
//   6. POST /api/auth/login failure → audit_log has auth.login.fail row
//   7. GET /api/admin/audit — returns logged events
//   8. GET /api/admin/audit?action=auth.login — filters correctly

mod common;

use axum::http::{Method, StatusCode};
use common::{build_app, json_oneshot, register_test_user};
use planner_backend::auth::{decode_confirm_token, encode_confirm_token};
use sqlx::PgPool;
use uuid::Uuid;

// ── shared helpers ────────────────────────────────────────────────────────────

const ADMIN_PASS: &str = "adminpass123";
const USER_PASS: &str = "userpass123";

/// Register a user and return their Bearer token.
async fn register_and_token(app: &axum::Router, email: &str, password: &str) -> String {
    let body = register_test_user(app, email, password).await;
    body["token"].as_str().unwrap().to_string()
}

/// Promote a user to admin directly in the database.
async fn promote_to_admin(pool: &PgPool, email: &str) {
    sqlx::query("UPDATE users SET role = 'admin' WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await
        .expect("failed to promote user to admin");
}

/// Helper: register a user, promote to admin, and return their token.
async fn setup_admin(app: &axum::Router, pool: &PgPool, email: &str) -> String {
    let token = register_and_token(app, email, ADMIN_PASS).await;
    promote_to_admin(pool, email).await;
    token
}

// ── 1. POST /api/admin/confirm without authentication ────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn confirm_without_bearer_returns_401(pool: PgPool) {
    let app = build_app(pool);

    let (status, _body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/confirm",
        Some(serde_json::json!({ "password": ADMIN_PASS, "action": "provider.update" })),
        None,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "missing Bearer must produce 401 before AdminUser extractor runs"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn confirm_with_non_admin_token_returns_403(pool: PgPool) {
    let app = build_app(pool);

    let token = register_and_token(&app, "regular@example.com", USER_PASS).await;

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/confirm",
        Some(serde_json::json!({ "password": USER_PASS, "action": "provider.update" })),
        Some(&token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "non-admin must receive 403, got {body}"
    );
}

// ── 2. POST /api/admin/confirm — admin + wrong password ──────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn confirm_with_wrong_password_returns_401(pool: PgPool) {
    let app = build_app(pool.clone());
    let token = setup_admin(&app, &pool, "admin@example.com").await;

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/confirm",
        Some(serde_json::json!({ "password": "wrong-password!", "action": "provider.update" })),
        Some(&token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "wrong password must be 401, got {body}"
    );
}

// ── 3. POST /api/admin/confirm — admin + correct password ────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn confirm_with_correct_password_returns_confirm_token(pool: PgPool) {
    let app = build_app(pool.clone());
    let token = setup_admin(&app, &pool, "admin@example.com").await;

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/confirm",
        Some(serde_json::json!({ "password": ADMIN_PASS, "action": "provider.update" })),
        Some(&token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "correct password must produce 200, body: {body}"
    );
    assert!(
        body["confirm_token"].is_string(),
        "response must contain a confirm_token string, got: {body}"
    );
    let confirm_token = body["confirm_token"].as_str().unwrap();
    assert!(!confirm_token.is_empty(), "confirm_token must not be empty");
}

// ── 4. Confirm token for wrong action rejected ────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn confirm_token_wrong_action_returns_forbidden(pool: PgPool) {
    // Mint a confirm token for "provider.update"
    let id = Uuid::now_v7();
    let token = encode_confirm_token(id, "provider.update", "test-secret").unwrap();

    // Validate against a different action — must return Forbidden.
    let err = decode_confirm_token(&token, "test-secret", "kill_switch.toggle").unwrap_err();
    assert!(
        matches!(err, planner_backend::middleware::error::AppError::Forbidden),
        "wrong action must return Forbidden"
    );

    // We need a pool reference to satisfy sqlx::test — do a trivial query
    // to avoid the unused-variable warning.
    let _: i32 = sqlx::query_scalar("SELECT 1::int4")
        .fetch_one(&pool)
        .await
        .unwrap();
}

// ── 5. Login success emits auth.login.success audit row ──────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn login_success_creates_audit_log_row(pool: PgPool) {
    let app = build_app(pool.clone());
    let email = "logme@example.com";

    // Register first.
    register_test_user(&app, email, USER_PASS).await;

    // Login.
    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": email, "password": USER_PASS })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login must succeed, got {body}");

    // Check audit_log.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE action = 'auth.login.success' AND actor_email = $1",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        count, 1,
        "exactly one auth.login.success audit row must exist"
    );
}

// ── 6. Login failure emits auth.login.fail audit row ─────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn login_failure_creates_audit_log_row(pool: PgPool) {
    let app = build_app(pool.clone());
    let email = "victim@example.com";

    // Register user.
    register_test_user(&app, email, USER_PASS).await;

    // Attempt login with wrong password.
    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": email, "password": "wrong!" })),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "wrong password must be 401, got {body}"
    );

    // Check audit_log.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE action = 'auth.login.fail' AND actor_email = $1",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(count, 1, "exactly one auth.login.fail audit row must exist");
}

#[sqlx::test(migrations = "./migrations")]
async fn login_failure_unknown_email_creates_audit_log_row(pool: PgPool) {
    let app = build_app(pool.clone());
    let email = "ghost@example.com"; // never registered

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": email, "password": "whatever" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE action = 'auth.login.fail' AND actor_email = $1",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        count, 1,
        "auth.login.fail must be logged even for unknown email"
    );
}

// ── 7. GET /api/admin/audit returns logged events ─────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn audit_list_returns_logged_events(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_email = "auditor@example.com";

    // Setup admin and generate some audit rows via login.
    let admin_token = setup_admin(&app, &pool, admin_email).await;
    let user_email = "audited@example.com";
    register_test_user(&app, user_email, USER_PASS).await;
    // Do a successful login to produce an auth.login.success row.
    json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": user_email, "password": USER_PASS })),
        None,
    )
    .await;

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/audit",
        None,
        Some(&admin_token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "admin audit list must return 200, got {body}"
    );
    assert!(
        body.as_array().is_some(),
        "response must be a JSON array, got: {body}"
    );
    let rows = body.as_array().unwrap();
    assert!(!rows.is_empty(), "audit log must have at least one row");
    // Verify expected fields exist on first row.
    let first = &rows[0];
    assert!(first["id"].is_string(), "row must have an id field");
    assert!(first["action"].is_string(), "row must have an action field");
    assert!(
        first["created_at"].is_string(),
        "row must have a created_at field"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn audit_list_without_token_returns_401(pool: PgPool) {
    let app = build_app(pool);

    let (status, _) = json_oneshot(&app, Method::GET, "/api/admin/audit", None, None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn audit_list_non_admin_returns_403(pool: PgPool) {
    let app = build_app(pool);
    let token = register_and_token(&app, "plain@example.com", USER_PASS).await;

    let (status, _) = json_oneshot(&app, Method::GET, "/api/admin/audit", None, Some(&token)).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ── 8. GET /api/admin/audit?action=auth.login filters correctly ───────────────

#[sqlx::test(migrations = "./migrations")]
async fn audit_list_action_filter_returns_matching_rows(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_email = "filter-admin@example.com";
    let admin_token = setup_admin(&app, &pool, admin_email).await;

    // Produce some audit rows: 1 login.success + 1 admin.confirm.
    let user_email = "filter-user@example.com";
    register_test_user(&app, user_email, USER_PASS).await;
    json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": user_email, "password": USER_PASS })),
        None,
    )
    .await;

    // Also call /confirm to generate an admin.confirm audit row.
    json_oneshot(
        &app,
        Method::POST,
        "/api/admin/confirm",
        Some(serde_json::json!({ "password": ADMIN_PASS, "action": "provider.update" })),
        Some(&admin_token),
    )
    .await;

    // Filter by "auth.login" prefix — should include auth.login.success only.
    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/audit?action=auth.login",
        None,
        Some(&admin_token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "filter request must succeed, got {body}"
    );
    let rows = body.as_array().expect("must be array");
    assert!(
        !rows.is_empty(),
        "auth.login filter must return at least one row"
    );
    for row in rows {
        let action = row["action"].as_str().unwrap_or("");
        assert!(
            action.starts_with("auth.login"),
            "all rows must have action starting with 'auth.login', got '{action}'"
        );
    }

    // admin.confirm rows must not appear.
    let has_admin_confirm = rows
        .iter()
        .any(|r| r["action"].as_str() == Some("admin.confirm"));
    assert!(
        !has_admin_confirm,
        "admin.confirm must not appear in auth.login filter results"
    );
}

// ── Fix 6: rate-limited login emits audit row ────────────────────────────────

/// After exceeding the login rate limit, a `auth.login.rate_limited` audit row
/// must be created for the attempt that triggered the 429.
#[sqlx::test(migrations = "./migrations")]
async fn rate_limited_login_creates_audit_log_row(pool: PgPool) {
    let app = build_app(pool.clone());
    let email = "rl-audit@example.com";

    register_test_user(&app, email, USER_PASS).await;

    // Exhaust the 10-attempt window.
    for _ in 0..10 {
        json_oneshot(
            &app,
            Method::POST,
            "/api/auth/login",
            Some(serde_json::json!({ "email": email, "password": "wrong" })),
            None,
        )
        .await;
    }

    // 11th attempt triggers 429 AND the audit log.
    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": email, "password": "wrong" })),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "must be 429, got {body}"
    );

    // Give the spawned task a moment to flush.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE action = 'auth.login.rate_limited' AND actor_email = $1",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(
        count >= 1,
        "at least one auth.login.rate_limited audit row must exist"
    );
}

// ── Fix 3: confirm endpoint rate limit ───────────────────────────────────────

/// After 5 confirm attempts the 6th must return a validation error (rate
/// limited) regardless of password correctness.
#[sqlx::test(migrations = "./migrations")]
async fn confirm_rate_limited_after_five_attempts(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_email = "confirm-rl@example.com";
    let admin_token = setup_admin(&app, &pool, admin_email).await;

    // 5 successful confirms should all return 200.
    for i in 0..5 {
        let (status, body) = json_oneshot(
            &app,
            Method::POST,
            "/api/admin/confirm",
            Some(serde_json::json!({ "password": ADMIN_PASS, "action": "provider.update" })),
            Some(&admin_token),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "attempt {i} must succeed, got {body}"
        );
    }

    // 6th attempt must be rate-limited.
    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/confirm",
        Some(serde_json::json!({ "password": ADMIN_PASS, "action": "provider.update" })),
        Some(&admin_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "6th confirm attempt must be rate-limited (422), got {body}"
    );
}

// ── Fix 4: action validation in confirm ──────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn confirm_rejects_action_with_wildcard_chars(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_token = setup_admin(&app, &pool, "action-val@example.com").await;

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/confirm",
        Some(serde_json::json!({ "password": ADMIN_PASS, "action": "bad%action" })),
        Some(&admin_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "action with '%' must be rejected with 422, got {body}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn confirm_rejects_oversized_action(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_token = setup_admin(&app, &pool, "action-size@example.com").await;

    let long_action = "a".repeat(129);
    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/admin/confirm",
        Some(serde_json::json!({ "password": ADMIN_PASS, "action": long_action })),
        Some(&admin_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "action >128 bytes must be rejected with 422, got {body}"
    );
}

// ── Fix 1: action filter rejects wildcard characters in audit query ───────────

#[sqlx::test(migrations = "./migrations")]
async fn audit_list_action_filter_rejects_wildcard_chars(pool: PgPool) {
    let app = build_app(pool.clone());
    let admin_token = setup_admin(&app, &pool, "wc-admin@example.com").await;

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/audit?action=auth%25login", // '%25' is URL-encoded '%'
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "action param with '%' must return 422, got {body}"
    );
}

// ── Bonus: token refresh emits audit row ─────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn token_refresh_creates_audit_log_row(pool: PgPool) {
    let app = build_app(pool.clone());
    let email = "refreshme@example.com";

    let reg_body = register_test_user(&app, email, USER_PASS).await;
    let refresh_token = reg_body["refresh_token"].as_str().unwrap().to_owned();

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/refresh",
        Some(serde_json::json!({ "refresh_token": refresh_token })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "refresh must succeed, got {body}");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE action = 'auth.token.refresh' AND actor_email = $1",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        count, 1,
        "exactly one auth.token.refresh audit row must exist"
    );
}
