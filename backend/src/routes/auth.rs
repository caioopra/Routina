use std::sync::OnceLock;

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::{Json, Router, extract::State, routing::get, routing::post};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::auth::{TokenKind, decode_token, encode_token, hash_password, verify_password};
use crate::middleware::error::AppError;
use crate::middleware::{CurrentUser, emit_audit};
use crate::models::user::{AuthResponse, CreateUser, LoginRequest, User, UserPublic};

use super::AppState;

/// A dummy password hash computed once at first use.
///
/// When a login request arrives for an email that does not exist in the
/// database, the handler calls `verify_password` against this hash with the
/// supplied (wrong) password.  This ensures the slow argon2 path runs
/// regardless of whether the email exists, preventing a timing side-channel
/// that would allow an attacker to enumerate registered addresses by measuring
/// response latency.
///
/// The hash is generated from a hardcoded placeholder string so it is always
/// well-formed (argon2 encoded format).  The verification will always fail —
/// the result is intentionally discarded.
static DUMMY_HASH: OnceLock<String> = OnceLock::new();

fn dummy_hash() -> &'static str {
    DUMMY_HASH.get_or_init(|| {
        hash_password("placeholder-dummy-constant").expect("dummy hash generation must not fail")
    })
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/me", get(me))
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub token: String,
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub planner_context: Option<String>,
    pub preferences: serde_json::Value,
}

async fn register(
    State(state): State<AppState>,
    Json(body): Json<CreateUser>,
) -> Result<Json<AuthResponse>, AppError> {
    if body.password.len() < 8 {
        return Err(AppError::Validation(
            "password must be at least 8 characters".into(),
        ));
    }

    let password_hash = hash_password(&body.password)?;
    let user_id = Uuid::now_v7();

    let mut tx = state.pool.begin().await?;

    let insert_result = sqlx::query_as::<_, User>(
        "INSERT INTO users (id, email, name, password_hash, preferences) \
         VALUES ($1, $2, $3, $4, '{}'::jsonb) \
         RETURNING id, email, name, password_hash, preferences, created_at, updated_at, role",
    )
    .bind(user_id)
    .bind(&body.email)
    .bind(&body.name)
    .bind(&password_hash)
    .fetch_one(&mut *tx)
    .await;

    let user = match insert_result {
        Ok(user) => user,
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => {
            return Err(AppError::Conflict("email already registered".into()));
        }
        Err(e) => return Err(AppError::Database(e)),
    };

    seed_default_labels(&mut tx, user.id).await?;
    tx.commit().await?;

    let (token, refresh_token) = mint_token_pair(&state, user.id)?;

    Ok(Json(AuthResponse {
        user: UserPublic::from(user),
        token,
        refresh_token,
    }))
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Normalize email before rate-limit check so that "User@Example.com" and
    // "user@example.com" share the same bucket.
    let normalized_email = body.email.trim().to_lowercase();

    // Extract IP for audit logging from the X-Forwarded-For header.
    // We don't have direct socket access in this layer, so we rely on the
    // proxy-injected header.  None is acceptable — audit rows may have no IP.
    let ip: Option<String> = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_owned());

    // Check the per-email rate limit BEFORE any DB lookup or password
    // comparison.  This prevents timing-oracle attacks: the 429 response is
    // returned unconditionally regardless of whether the email exists in the
    // database, so an attacker cannot use the 429 threshold to confirm that an
    // email is registered.
    if let Err(retry_after) = state.login_rate_limit.check_and_record(&normalized_email) {
        // Fix 6: audit-log rate-limited attempts so credential-stuffing attacks
        // are visible in the audit trail.  Fire-and-forget — if the insert fails
        // we still return 429 immediately without blocking.
        let pool = state.pool.clone();
        let email_clone = normalized_email.clone();
        let ip_clone = ip.clone();
        tokio::spawn(async move {
            let _ = emit_audit(
                &pool,
                None,
                &email_clone,
                "auth.login.rate_limited",
                Some("email"),
                Some(&email_clone),
                None,
                ip_clone.as_deref(),
                None,
            )
            .await;
        });

        let body = json!({
            "error": "rate_limited",
            "retry_after_seconds": retry_after,
        });
        return Ok((
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, retry_after.to_string())],
            Json(body),
        )
            .into_response());
    }

    let user_opt = sqlx::query_as::<_, User>(
        "SELECT id, email, name, password_hash, preferences, created_at, updated_at, role \
         FROM users WHERE email = $1",
    )
    .bind(&normalized_email)
    .fetch_optional(&state.pool)
    .await?;

    // When the email is not found, run `verify_password` against the static
    // dummy hash anyway.  This equalizes the response time for the "email not
    // found" and "wrong password" cases, preventing a timing side-channel that
    // would allow an attacker to enumerate which addresses are registered.
    // The result is always `false` (wrong password); we discard it and fall
    // through to the same `Unauthorized` error returned for a wrong password.
    let Some(user) = user_opt else {
        let _ = verify_password(&body.password, dummy_hash());
        // Emit audit for login failure (email not found).
        // No actor_id since there is no authenticated user.
        let _ = emit_audit(
            &state.pool,
            None,
            &normalized_email,
            "auth.login.fail",
            Some("email"),
            Some(&normalized_email),
            None,
            ip.as_deref(),
            None,
        )
        .await;
        return Err(AppError::Unauthorized);
    };

    if !verify_password(&body.password, &user.password_hash)? {
        // Emit audit for login failure (wrong password).
        let _ = emit_audit(
            &state.pool,
            Some(user.id),
            &user.email,
            "auth.login.fail",
            Some("user"),
            Some(&user.id.to_string()),
            None,
            ip.as_deref(),
            None,
        )
        .await;
        return Err(AppError::Unauthorized);
    }

    // Successful login: clear the rate-limit bucket so the next N attempts
    // start from zero.  This avoids locking out a legitimate user who had
    // earlier failures (e.g., mistyped their password a few times).
    state.login_rate_limit.clear(&normalized_email);

    let (token, refresh_token) = mint_token_pair(&state, user.id)?;

    // Emit audit for login success.
    let _ = emit_audit(
        &state.pool,
        Some(user.id),
        &user.email,
        "auth.login.success",
        Some("user"),
        Some(&user.id.to_string()),
        None,
        ip.as_deref(),
        None,
    )
    .await;

    Ok(Json(AuthResponse {
        user: UserPublic::from(user),
        token,
        refresh_token,
    })
    .into_response())
}

async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, AppError> {
    let claims = decode_token(&body.refresh_token, &state.config.jwt_secret)?;
    if claims.typ != TokenKind::Refresh {
        return Err(AppError::Unauthorized);
    }

    let (token, refresh_token) = mint_token_pair(&state, claims.sub)?;

    // Emit audit for successful token refresh.  Fetch email for the audit row —
    // ignore failures so a missing user doesn't block the refresh response.
    let email_opt: Option<String> = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(claims.sub)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();

    if let Some(email) = email_opt {
        let _ = emit_audit(
            &state.pool,
            Some(claims.sub),
            &email,
            "auth.token.refresh",
            Some("user"),
            Some(&claims.sub.to_string()),
            None,
            None,
            None,
        )
        .await;
    }

    Ok(Json(RefreshResponse {
        token,
        refresh_token,
    }))
}

async fn me(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<MeResponse>, AppError> {
    let row = sqlx::query!(
        "SELECT preferences, planner_context FROM users WHERE id = $1",
        user.id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    Ok(Json(MeResponse {
        id: user.id,
        email: user.email,
        name: user.name,
        role: user.role,
        planner_context: row.planner_context,
        preferences: row.preferences,
    }))
}

fn mint_token_pair(state: &AppState, user_id: Uuid) -> Result<(String, String), AppError> {
    let token = encode_token(
        user_id,
        TokenKind::Access,
        &state.config.jwt_secret,
        state.config.jwt_expiration_hours * 3600,
    )?;
    let refresh_token = encode_token(
        user_id,
        TokenKind::Refresh,
        &state.config.jwt_secret,
        state.config.refresh_token_expiration_days * 86400,
    )?;
    Ok((token, refresh_token))
}

async fn seed_default_labels(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<(), AppError> {
    let defaults: [(&str, &str, &str, &str); 7] = [
        ("trabalho", "#1e3a5f", "#93c5fd", "#2563eb"),
        ("mestrado", "#3b1f4a", "#d8b4fe", "#7c3aed"),
        ("aula", "#4a2c1b", "#fdba74", "#ea580c"),
        ("exercicio", "#1a3a2a", "#86efac", "#16a34a"),
        ("slides", "#4a3f1b", "#fde68a", "#ca8a04"),
        ("viagem", "#3b3b3b", "#d4d4d4", "#737373"),
        ("livre", "#1e2d3d", "#7dd3fc", "#0284c7"),
    ];

    for (name, bg, text, border) in defaults {
        sqlx::query(
            "INSERT INTO labels (id, user_id, name, color_bg, color_text, color_border, is_default) \
             VALUES ($1, $2, $3, $4, $5, $6, true)",
        )
        .bind(Uuid::now_v7())
        .bind(user_id)
        .bind(name)
        .bind(bg)
        .bind(text)
        .bind(border)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}
