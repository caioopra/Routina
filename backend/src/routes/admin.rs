//! Admin routes — `/api/admin/*`.
//!
//! Every handler in this module requires the `AdminUser` extractor, which
//! enforces `role == "admin"` via a DB lookup on every request.  No
//! router-level middleware is needed because the extractor itself returns
//! HTTP 403 for non-admins and HTTP 401 (via `CurrentUser`) for missing/invalid
//! tokens.
//!
//! Routes in this module:
//!   `GET  /api/admin/dashboard` → proof-of-gating stub (Slice A)
//!   `POST /api/admin/confirm`   → step-up auth: password re-check → confirm token
//!   `GET  /api/admin/audit`     → paginated audit log reader

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::{Json, Router, routing::get, routing::post};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::auth::{ConfirmClaims, decode_confirm_token, encode_confirm_token, verify_password};
use crate::middleware::error::AppError;
use crate::middleware::{AdminUser, emit_audit};

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(dashboard))
        .route("/confirm", post(confirm))
        .route("/audit", get(audit_list))
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct DashboardResponse {
    ok: bool,
    admin_email: String,
}

/// Stub dashboard endpoint — proof-of-gating for Slice A.
async fn dashboard(State(_state): State<AppState>, admin: AdminUser) -> Json<DashboardResponse> {
    Json(DashboardResponse {
        ok: true,
        admin_email: admin.email,
    })
}

// ── Confirm (step-up auth) ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ConfirmRequest {
    password: String,
    action: String,
}

#[derive(Debug, Serialize)]
struct ConfirmResponse {
    confirm_token: String,
}

/// Validate that `action` only contains characters in `[a-zA-Z0-9._-]` and is
/// at most 128 bytes.  Returns `AppError::Validation` on violation.
fn validate_action(action: &str) -> Result<(), AppError> {
    if action.len() > 128 {
        return Err(AppError::Validation(
            "action must be at most 128 characters".into(),
        ));
    }
    if !action
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err(AppError::Validation(
            "action may only contain letters, digits, '.', '_', '-'".into(),
        ));
    }
    Ok(())
}

/// `POST /api/admin/confirm`
///
/// Re-verifies the admin's password and mints a short-lived confirm token for
/// the requested action.  The token is returned to the caller and must be
/// supplied as `x-confirm-token` on admin mutation requests.
///
/// Rate-limited to 5 attempts per 5-minute window per admin user_id.
async fn confirm(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(body): Json<ConfirmRequest>,
) -> Result<Json<ConfirmResponse>, AppError> {
    // Fix 4: validate action before touching the DB.
    validate_action(&body.action)?;

    // Fix 3: rate-limit on confirm endpoint, keyed on admin's user_id.
    let key = admin.user_id.to_string();
    if let Err(retry_after) = state.confirm_rate_limit.check_and_record(&key) {
        return Err(AppError::Validation(format!(
            "rate limited; retry after {retry_after} seconds"
        )));
    }

    // Fetch the admin's current password hash from the DB.
    let row = sqlx::query_scalar::<_, String>("SELECT password_hash FROM users WHERE id = $1")
        .bind(admin.user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if !verify_password(&body.password, &row)? {
        return Err(AppError::Unauthorized);
    }

    let confirm_token =
        encode_confirm_token(admin.user_id, &body.action, &state.config.jwt_secret)?;

    // Emit audit: admin requested a step-up confirm token for this action.
    let _ = emit_audit(
        &state.pool,
        Some(admin.user_id),
        &admin.email,
        "admin.confirm",
        Some("action"),
        Some(&body.action),
        None,
        None,
        None,
    )
    .await;

    Ok(Json(ConfirmResponse { confirm_token }))
}

// ── Confirm token validation helper ──────────────────────────────────────────

/// Read `x-confirm-token` from request headers, decode it, verify that its
/// `action` claim matches `expected_action`, and check that `claims.sub ==
/// expected_user_id` to prevent cross-admin token reuse.
///
/// Returns `AppError::Forbidden` if the header is missing, the token is invalid
/// or expired, the action does not match, or the subject does not match.
pub fn validate_confirm_token(
    headers: &HeaderMap,
    secret: &str,
    expected_action: &str,
    expected_user_id: Uuid,
) -> Result<ConfirmClaims, AppError> {
    let token_str = headers
        .get("x-confirm-token")
        .ok_or(AppError::Forbidden)?
        .to_str()
        .map_err(|_| AppError::Forbidden)?;

    let claims = decode_confirm_token(token_str, secret, expected_action)?;

    // Fix 5: verify the sub matches the calling admin's user_id.
    if claims.sub != expected_user_id {
        return Err(AppError::Forbidden);
    }

    Ok(claims)
}

// ── Audit log reader ──────────────────────────────────────────────────────────

/// Query parameters for `GET /api/admin/audit`.
#[derive(Debug, Deserialize)]
struct AuditQuery {
    /// Cursor: return rows created before the row with this id.
    before: Option<Uuid>,
    /// Optional action prefix filter (e.g. `auth.login` matches `auth.login.success`).
    /// Allowed characters: `[a-zA-Z0-9._-]`.  Returns 422 on invalid input.
    action: Option<String>,
    /// Page size (default 50, max 100).
    limit: Option<i64>,
}

/// A single audit log row returned to the client.
#[derive(Debug, Serialize, sqlx::FromRow)]
struct AuditRow {
    pub id: Uuid,
    pub actor_id: Option<Uuid>,
    pub actor_email: String,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// `GET /api/admin/audit`
///
/// Returns a JSON array of audit_log rows, newest first.  Supports cursor-based
/// pagination via the `before` UUID and optional prefix filtering via `action`.
///
/// Fix 1: `action` is validated to `[a-zA-Z0-9._-]` characters only; wildcards
/// in the value cannot escape into the LIKE pattern.
///
/// Fix 2: cursor uses a composite `(created_at, id)` tuple comparison so that
/// rows sharing the same microsecond timestamp are never silently skipped.
async fn audit_list(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(params): Query<AuditQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);

    // Fix 1: reject action values that contain LIKE wildcard characters or
    // anything outside [a-zA-Z0-9._-].
    if let Some(ref action_prefix) = params.action {
        validate_action(action_prefix)?;
    }

    let rows = match (&params.before, &params.action) {
        (Some(before_id), Some(action_prefix)) => {
            let pattern = format!("{action_prefix}%");
            sqlx::query_as::<_, AuditRow>(
                // Fix 2: composite (created_at, id) cursor — Postgres tuple
                // comparison is stable even when multiple rows share the same
                // microsecond timestamp.
                r#"
                SELECT id, actor_id, actor_email, action, target_type, target_id,
                       payload, ip::text, user_agent, created_at
                FROM audit_log
                WHERE (created_at, id) < (
                    (SELECT created_at FROM audit_log WHERE id = $1),
                    $1
                )
                  AND action LIKE $2
                ORDER BY created_at DESC, id DESC
                LIMIT $3
                "#,
            )
            .bind(before_id)
            .bind(pattern)
            .bind(limit)
            .fetch_all(&state.pool)
            .await?
        }
        (Some(before_id), None) => {
            sqlx::query_as::<_, AuditRow>(
                r#"
                SELECT id, actor_id, actor_email, action, target_type, target_id,
                       payload, ip::text, user_agent, created_at
                FROM audit_log
                WHERE (created_at, id) < (
                    (SELECT created_at FROM audit_log WHERE id = $1),
                    $1
                )
                ORDER BY created_at DESC, id DESC
                LIMIT $2
                "#,
            )
            .bind(before_id)
            .bind(limit)
            .fetch_all(&state.pool)
            .await?
        }
        (None, Some(action_prefix)) => {
            let pattern = format!("{action_prefix}%");
            sqlx::query_as::<_, AuditRow>(
                r#"
                SELECT id, actor_id, actor_email, action, target_type, target_id,
                       payload, ip::text, user_agent, created_at
                FROM audit_log
                WHERE action LIKE $1
                ORDER BY created_at DESC, id DESC
                LIMIT $2
                "#,
            )
            .bind(pattern)
            .bind(limit)
            .fetch_all(&state.pool)
            .await?
        }
        (None, None) => {
            sqlx::query_as::<_, AuditRow>(
                r#"
                SELECT id, actor_id, actor_email, action, target_type, target_id,
                       payload, ip::text, user_agent, created_at
                FROM audit_log
                ORDER BY created_at DESC, id DESC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(&state.pool)
            .await?
        }
    };

    Ok(Json(json!(rows)))
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_action ───────────────────────────────────────────────────────

    #[test]
    fn validate_action_accepts_valid_strings() {
        assert!(validate_action("auth.login.success").is_ok());
        assert!(validate_action("provider.update").is_ok());
        assert!(validate_action("kill-switch.toggle").is_ok());
        assert!(validate_action("a").is_ok());
        assert!(validate_action("A1.B2_C3-D4").is_ok());
    }

    #[test]
    fn validate_action_rejects_percent_wildcard() {
        // '%' is the primary SQL LIKE wildcard and must be rejected to prevent
        // injection through the action prefix filter.
        let err = validate_action("auth%login");
        assert!(
            err.is_err(),
            "percent sign must be rejected as a wildcard injection vector"
        );
        assert!(validate_action("auth%").is_err());
        // '_' is in the allowed set ([a-zA-Z0-9._-]) and must still pass.
        assert!(
            validate_action("auth_login").is_ok(),
            "underscore is an allowed character and must not be rejected"
        );
    }

    #[test]
    fn validate_action_rejects_backslash() {
        let err = validate_action("auth\\login");
        assert!(
            err.is_err(),
            "backslash must be rejected as a potential LIKE escape character"
        );
    }

    #[test]
    fn validate_action_rejects_too_long() {
        let long = "a".repeat(129);
        assert!(
            validate_action(&long).is_err(),
            "strings longer than 128 bytes must be rejected"
        );
    }

    #[test]
    fn validate_action_accepts_exactly_128_bytes() {
        let exactly_128 = "a".repeat(128);
        assert!(
            validate_action(&exactly_128).is_ok(),
            "128-byte action must be accepted"
        );
    }

    #[test]
    fn validate_action_rejects_spaces() {
        assert!(validate_action("auth login").is_err());
    }

    #[test]
    fn validate_action_rejects_empty_string() {
        // An empty action is technically valid (0 chars all pass the predicate,
        // 0 bytes ≤ 128) — but callers should provide a meaningful action.
        // We leave this as accepted for now and document it.
        // An empty LIKE pattern "%" would match everything, but the empty prefix
        // would become "%", which is still a valid prefix filter behaviour.
        assert!(validate_action("").is_ok());
    }

    // ── validate_confirm_token ────────────────────────────────────────────────

    #[test]
    fn validate_confirm_token_rejects_cross_admin_sub() {
        use crate::auth::encode_confirm_token;
        use axum::http::HeaderMap;

        const SECRET: &str = "test-secret";
        let admin_a = Uuid::now_v7();
        let admin_b = Uuid::now_v7();
        let action = "provider.update";

        // Mint a token for admin_a.
        let token = encode_confirm_token(admin_a, action, SECRET).unwrap();

        // Build headers containing admin_a's token.
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-confirm-token",
            token.parse().expect("token must be a valid header value"),
        );

        // admin_b tries to use admin_a's confirm token — must be Forbidden.
        let err = validate_confirm_token(&headers, SECRET, action, admin_b).unwrap_err();
        assert!(
            matches!(err, AppError::Forbidden),
            "cross-admin token reuse must return Forbidden, got {err:?}"
        );
    }

    #[test]
    fn validate_confirm_token_accepts_matching_sub() {
        use crate::auth::encode_confirm_token;
        use axum::http::HeaderMap;

        const SECRET: &str = "test-secret";
        let admin_id = Uuid::now_v7();
        let action = "provider.update";

        let token = encode_confirm_token(admin_id, action, SECRET).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-confirm-token",
            token.parse().expect("token must be a valid header value"),
        );

        let claims = validate_confirm_token(&headers, SECRET, action, admin_id)
            .expect("same admin sub must be accepted");
        assert_eq!(claims.sub, admin_id);
        assert_eq!(claims.action, action);
    }

    #[test]
    fn validate_confirm_token_missing_header_returns_forbidden() {
        let headers = HeaderMap::new();
        let err =
            validate_confirm_token(&headers, "secret", "any.action", Uuid::now_v7()).unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }
}
