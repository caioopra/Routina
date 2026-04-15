// Integration tests for the admin-gated `/api/admin` routes.
//
// Scenarios:
//  - Non-admin user hits `GET /api/admin/dashboard` → 403.
//  - Admin user hits `GET /api/admin/dashboard` → 200 with correct email.
//  - No Bearer header → 401 (auth_middleware fires before AdminUser extractor).
//  - Invalid/tampered Bearer token → 401.

mod common;

use axum::http::{Method, StatusCode};
use common::{build_app, json_oneshot, register_test_user};
use sqlx::PgPool;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Register a fresh user and return their access token.
async fn login_token(app: &axum::Router, email: &str) -> String {
    let body = register_test_user(app, email, "longenoughpass").await;
    body["token"].as_str().unwrap().to_string()
}

/// Promote a user to admin directly in the DB (bypasses the promote binary which
/// is out of scope for this slice).
async fn promote_to_admin(pool: &PgPool, email: &str) {
    sqlx::query("UPDATE users SET role = 'admin' WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await
        .expect("failed to promote user to admin");
}

// ── unauthenticated access ────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn dashboard_without_bearer_returns_401(pool: PgPool) {
    let app = build_app(pool);

    let (status, _) = json_oneshot(&app, Method::GET, "/api/admin/dashboard", None, None).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "missing Bearer must be rejected before AdminUser extractor runs"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn dashboard_with_invalid_token_returns_401(pool: PgPool) {
    let app = build_app(pool);

    let (status, _) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/dashboard",
        None,
        Some("not.a.valid.jwt"),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "tampered token must be rejected with 401, not 403"
    );
}

// ── non-admin access ─────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn dashboard_with_regular_user_returns_403(pool: PgPool) {
    let app = build_app(pool);

    let token = login_token(&app, "regular@example.com").await;

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/dashboard",
        None,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
    assert_eq!(
        body["error"], "forbidden",
        "response body must contain {{\"error\":\"forbidden\"}}"
    );
}

// ── admin access ─────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn dashboard_with_admin_user_returns_200(pool: PgPool) {
    let app = build_app(pool.clone());

    let email = "admin@example.com";
    let token = login_token(&app, email).await;

    // Promote directly in the DB so the next request re-reads role = 'admin'.
    promote_to_admin(&pool, email).await;

    let (status, body) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/dashboard",
        None,
        Some(&token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "admin must reach the dashboard: {body}"
    );
    assert_eq!(body["ok"], true);
    assert_eq!(
        body["admin_email"], email,
        "dashboard must echo the admin's email"
    );
}

// ── error body uniformity ─────────────────────────────────────────────────────

/// The forbidden body must be the same JSON shape whether the user is not an
/// admin or whether a fresh (non-admin) token is used, to prevent enumeration.
#[sqlx::test(migrations = "./migrations")]
async fn forbidden_body_is_same_for_non_admin_and_promoted_user(pool: PgPool) {
    let app = build_app(pool.clone());

    // Register two separate users.
    let user_a_token = login_token(&app, "user-a@example.com").await;
    let user_b_token = login_token(&app, "user-b@example.com").await;

    let (status_a, body_a) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/dashboard",
        None,
        Some(&user_a_token),
    )
    .await;

    let (status_b, body_b) = json_oneshot(
        &app,
        Method::GET,
        "/api/admin/dashboard",
        None,
        Some(&user_b_token),
    )
    .await;

    assert_eq!(status_a, StatusCode::FORBIDDEN);
    assert_eq!(status_b, StatusCode::FORBIDDEN);
    // Both bodies must be identical — no leakage of which user exists.
    assert_eq!(body_a, body_b, "forbidden bodies must match across users");
}
