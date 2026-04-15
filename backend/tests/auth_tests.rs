mod common;

use axum::http::{Method, StatusCode};
use common::{build_app, json_oneshot, register_test_user};
use planner_backend::auth::{TokenKind, decode_token};
use serde_json::json;
use sqlx::PgPool;
use std::time::Instant;
use uuid::Uuid;

const JWT_SECRET: &str = "test-secret";

#[sqlx::test(migrations = "./migrations")]
async fn register_creates_user_and_returns_tokens(pool: PgPool) {
    let app = build_app(pool);
    let body = register_test_user(&app, "alice@example.com", "correct horse battery staple").await;

    assert_eq!(body["user"]["email"], "alice@example.com");
    assert_eq!(body["user"]["name"], "Test");
    assert!(Uuid::parse_str(body["user"]["id"].as_str().unwrap()).is_ok());
    assert!(!body["token"].as_str().unwrap().is_empty());
    assert!(!body["refresh_token"].as_str().unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn register_seeds_seven_default_labels(pool: PgPool) {
    let app = build_app(pool.clone());
    let body = register_test_user(&app, "labels@example.com", "longenoughpass").await;
    let user_id = Uuid::parse_str(body["user"]["id"].as_str().unwrap()).unwrap();

    let rows: Vec<(String, bool)> =
        sqlx::query_as("SELECT name, is_default FROM labels WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(rows.len(), 7);
    let mut names: Vec<String> = rows.iter().map(|(n, _)| n.clone()).collect();
    names.sort();
    let mut expected = vec![
        "trabalho",
        "mestrado",
        "aula",
        "exercicio",
        "slides",
        "viagem",
        "livre",
    ];
    expected.sort();
    let expected: Vec<String> = expected.into_iter().map(|s| s.to_string()).collect();
    assert_eq!(names, expected);
    assert!(rows.iter().all(|(_, is_default)| *is_default));
}

#[sqlx::test(migrations = "./migrations")]
async fn register_with_duplicate_email_returns_409(pool: PgPool) {
    let app = build_app(pool);
    register_test_user(&app, "dup@example.com", "longenoughpass").await;

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/register",
        Some(json!({ "email": "dup@example.com", "name": "Other", "password": "longenoughpass" })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn register_with_short_password_returns_422(pool: PgPool) {
    let app = build_app(pool);
    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/register",
        Some(json!({ "email": "short@example.com", "name": "Test", "password": "short" })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn login_with_correct_password_returns_tokens(pool: PgPool) {
    let app = build_app(pool);
    register_test_user(&app, "login@example.com", "longenoughpass").await;

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(json!({ "email": "login@example.com", "password": "longenoughpass" })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["email"], "login@example.com");
    assert!(!body["token"].as_str().unwrap().is_empty());
    assert!(!body["refresh_token"].as_str().unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn login_with_wrong_password_returns_401(pool: PgPool) {
    let app = build_app(pool);
    register_test_user(&app, "wrongpw@example.com", "longenoughpass").await;

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(json!({ "email": "wrongpw@example.com", "password": "wrong-password" })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn login_with_unknown_email_returns_401(pool: PgPool) {
    let app = build_app(pool);
    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(json!({ "email": "ghost@example.com", "password": "longenoughpass" })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn refresh_returns_new_tokens(pool: PgPool) {
    let app = build_app(pool);
    let register = register_test_user(&app, "refresh@example.com", "longenoughpass").await;
    let refresh_token = register["refresh_token"].as_str().unwrap();

    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/refresh",
        Some(json!({ "refresh_token": refresh_token })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let new_access = body["token"].as_str().unwrap();
    let new_refresh = body["refresh_token"].as_str().unwrap();
    assert!(!new_access.is_empty());
    assert!(!new_refresh.is_empty());

    let access_claims = decode_token(new_access, JWT_SECRET).unwrap();
    assert_eq!(access_claims.typ, TokenKind::Access);
    let refresh_claims = decode_token(new_refresh, JWT_SECRET).unwrap();
    assert_eq!(refresh_claims.typ, TokenKind::Refresh);
}

#[sqlx::test(migrations = "./migrations")]
async fn refresh_with_access_token_returns_401(pool: PgPool) {
    let app = build_app(pool);
    let register = register_test_user(&app, "rotate@example.com", "longenoughpass").await;
    let access_token = register["token"].as_str().unwrap();

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/refresh",
        Some(json!({ "refresh_token": access_token })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn me_with_valid_token_returns_user(pool: PgPool) {
    let app = build_app(pool);
    let register = register_test_user(&app, "me@example.com", "longenoughpass").await;
    let token = register["token"].as_str().unwrap();

    let (status, body) = json_oneshot(&app, Method::GET, "/api/auth/me", None, Some(token)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["email"], "me@example.com");
    assert_eq!(body["name"], "Test");
    assert!(body["id"].is_string());
    assert!(body["preferences"].is_object());
}

#[sqlx::test(migrations = "./migrations")]
async fn me_without_token_returns_401(pool: PgPool) {
    let app = build_app(pool);
    let (status, _) = json_oneshot(&app, Method::GET, "/api/auth/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn me_with_refresh_token_returns_401(pool: PgPool) {
    let app = build_app(pool);
    let register = register_test_user(&app, "refuse@example.com", "longenoughpass").await;
    let refresh_token = register["refresh_token"].as_str().unwrap();

    let (status, _) =
        json_oneshot(&app, Method::GET, "/api/auth/me", None, Some(refresh_token)).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Verify that both the "email not found" and "wrong password" paths exercise
/// the argon2 slow hash, preventing a timing side-channel that would allow an
/// attacker to enumerate registered email addresses.
///
/// We assert that each path takes at least 50 ms, which proves argon2 ran.
/// We do not assert equality of the two durations — that is inherently fragile
/// under CI load.  The important property is that the fast short-circuit
/// (returning immediately without any hash work) is not present.
#[sqlx::test(migrations = "./migrations")]
async fn login_timing_equalized_for_unknown_email(pool: PgPool) {
    let app = build_app(pool);

    let email = "timing-test@example.com";
    let password = "correctpassword1234";
    register_test_user(&app, email, password).await;

    // Path A: email exists, wrong password → argon2 verify runs.
    let start_a = Instant::now();
    let (status_a, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(json!({ "email": email, "password": "wrongpassword" })),
        None,
    )
    .await;
    let elapsed_a = start_a.elapsed();

    // Path B: email does not exist → dummy hash verify runs.
    let start_b = Instant::now();
    let (status_b, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(json!({ "email": "nobody@example.com", "password": "wrongpassword" })),
        None,
    )
    .await;
    let elapsed_b = start_b.elapsed();

    assert_eq!(status_a, StatusCode::UNAUTHORIZED);
    assert_eq!(status_b, StatusCode::UNAUTHORIZED);

    // Both paths must have spent time on argon2 (≥ 50 ms is a safe lower bound
    // even on slow CI; the default argon2 params typically take 100–300 ms).
    assert!(
        elapsed_a.as_millis() >= 50,
        "path A (wrong password) took only {}ms — argon2 may have been skipped",
        elapsed_a.as_millis()
    );
    assert!(
        elapsed_b.as_millis() >= 50,
        "path B (unknown email) took only {}ms — dummy hash work may have been skipped",
        elapsed_b.as_millis()
    );
}
