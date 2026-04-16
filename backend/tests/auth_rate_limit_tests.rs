// Integration tests for the per-email login rate limiter.
//
// Scenarios:
//  - 10 failed logins → 11th returns 429 with Retry-After.
//  - Successful login after ≤10 failures clears the bucket; the next attempt
//    is allowed.
//  - Two different emails have independent buckets.
//  - Rate-limit response does not reveal whether the email exists.

mod common;

use axum::http::{Method, StatusCode};
use common::{build_app, json_oneshot, register_test_user};
use serde_json::json;
use sqlx::PgPool;

// ── helpers ───────────────────────────────────────────────────────────────────

async fn attempt_login(
    app: &axum::Router,
    email: &str,
    password: &str,
) -> (StatusCode, serde_json::Value) {
    json_oneshot(
        app,
        Method::POST,
        "/api/auth/login",
        Some(json!({ "email": email, "password": password })),
        None,
    )
    .await
}

// ── rate-limit basic behaviour ────────────────────────────────────────────────

/// After exactly 10 failed attempts the 11th must be rejected with 429.
///
/// Design note: each attempt (successful or not) is recorded BEFORE the DB
/// lookup, so the rate limiter fires on attempt 11 regardless of the outcome
/// of previous attempts.
#[sqlx::test(migrations = "./migrations")]
async fn eleven_failed_logins_returns_429(pool: PgPool) {
    let app = build_app(pool.clone());

    // Register a real user so some of the failures can reach the password check.
    let email = "ratelimit@example.com";
    register_test_user(&app, email, "correctpassword").await;

    // First 10 failed attempts must all return 401 (wrong password).
    for i in 0..10 {
        let (status, _) = attempt_login(&app, email, "wrongpassword").await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "attempt {i} should be 401, not yet rate-limited"
        );
    }

    // 11th attempt: bucket is full → 429 BEFORE password comparison.
    let (status, body) = attempt_login(&app, email, "wrongpassword").await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "11th attempt must be 429: {body}"
    );
    assert_eq!(body["error"], "rate_limited");
    assert!(
        body["retry_after_seconds"].as_u64().unwrap_or(0) >= 1,
        "retry_after_seconds must be at least 1"
    );
}

/// The response must carry a `Retry-After` header on 429.
#[sqlx::test(migrations = "./migrations")]
async fn rate_limited_response_includes_retry_after_header(pool: PgPool) {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = build_app(pool.clone());
    let email = "header-test@example.com";
    register_test_user(&app, email, "secret1234").await;

    // Exhaust the 10-attempt window.
    for _ in 0..10 {
        attempt_login(&app, email, "wrong").await;
    }

    // Build a raw request so we can inspect headers.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "email": email, "password": "wrong" })).unwrap(),
        ))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let retry_after = response
        .headers()
        .get("retry-after")
        .expect("Retry-After header must be present")
        .to_str()
        .expect("Retry-After header must be ASCII");

    let retry_secs: u64 = retry_after.parse().expect("Retry-After must be a number");
    assert!(retry_secs >= 1, "Retry-After must be >= 1 second");
}

// ── bucket reset on success ───────────────────────────────────────────────────

/// Five failed attempts, then a correct login, then the 6th attempt uses a
/// fresh bucket → allowed (not rejected).
#[sqlx::test(migrations = "./migrations")]
async fn successful_login_clears_bucket(pool: PgPool) {
    let app = build_app(pool.clone());

    let email = "cleartest@example.com";
    let password = "correctpassword";
    register_test_user(&app, email, password).await;

    // 5 failed attempts.
    for _ in 0..5 {
        attempt_login(&app, email, "wrongpassword").await;
    }

    // Correct login clears the bucket.
    let (status, _) = attempt_login(&app, email, password).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "correct password must succeed after 5 failures"
    );

    // Now make 10 more failed attempts; they must still be allowed (429 only
    // fires on the 11th, because the bucket was cleared).
    for i in 0..10 {
        let (status, _) = attempt_login(&app, email, "wrong").await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "attempt {i} after bucket reset must be 401, not 429"
        );
    }

    // 11th attempt post-reset IS now rate-limited.
    let (status, _) = attempt_login(&app, email, "wrong").await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "11th attempt after reset must be 429"
    );
}

// ── independent buckets per email ────────────────────────────────────────────

/// Exhausting the rate limit for email A must not affect email B.
#[sqlx::test(migrations = "./migrations")]
async fn different_emails_have_independent_buckets(pool: PgPool) {
    let app = build_app(pool.clone());

    let email_a = "alice-rl@example.com";
    let email_b = "bob-rl@example.com";

    register_test_user(&app, email_a, "passA1234").await;
    register_test_user(&app, email_b, "passB5678").await;

    // Exhaust alice's bucket.
    for _ in 0..10 {
        attempt_login(&app, email_a, "wrong").await;
    }
    let (status, _) = attempt_login(&app, email_a, "wrong").await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "alice must be rate-limited"
    );

    // Bob must still be able to log in correctly.
    let (status, _) = attempt_login(&app, email_b, "passB5678").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "bob must not be affected by alice's rate-limit"
    );
}

// ── email existence not leaked ────────────────────────────────────────────────

/// The 429 response is the same whether or not the email is registered, so an
/// attacker cannot use the rate-limit threshold to confirm email existence.
#[sqlx::test(migrations = "./migrations")]
async fn rate_limit_does_not_reveal_email_existence(pool: PgPool) {
    let app = build_app(pool.clone());

    let registered = "known@example.com";
    let unknown = "ghost@example.com";
    register_test_user(&app, registered, "password1234").await;

    // Exhaust both buckets with wrong attempts.
    for _ in 0..10 {
        attempt_login(&app, registered, "wrong").await;
        attempt_login(&app, unknown, "wrong").await;
    }

    let (status_known, body_known) = attempt_login(&app, registered, "wrong").await;
    let (status_unknown, body_unknown) = attempt_login(&app, unknown, "wrong").await;

    assert_eq!(status_known, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(status_unknown, StatusCode::TOO_MANY_REQUESTS);

    // Both bodies must have the same shape.
    assert_eq!(
        body_known["error"], "rate_limited",
        "registered email must return rate_limited error"
    );
    assert_eq!(
        body_unknown["error"], "rate_limited",
        "unknown email must return same rate_limited error"
    );
}

// ── rate limit fires before password hash comparison ─────────────────────────

/// The 429 response must arrive quickly — before the expensive argon2 hash
/// verification runs.  We assert that the rate-limited response returns the
/// `rate_limited` error code (not `Unauthorized`), which proves the fast path
/// was taken before any password work.
#[sqlx::test(migrations = "./migrations")]
async fn rate_limited_response_skips_password_check(pool: PgPool) {
    let app = build_app(pool.clone());

    let email = "fastpath@example.com";
    register_test_user(&app, email, "realpassword").await;

    // Exhaust the bucket.
    for _ in 0..10 {
        attempt_login(&app, email, "wrong").await;
    }

    // Even the *correct* password must be rejected at 429 when the bucket is
    // full, proving that password comparison is bypassed.
    let (status, body) = attempt_login(&app, email, "realpassword").await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "correct password must still be 429 when rate limit is exceeded"
    );
    assert_eq!(
        body["error"], "rate_limited",
        "error field must be rate_limited, not Unauthorized"
    );
}
