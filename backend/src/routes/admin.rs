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

/// `POST /api/admin/confirm`
///
/// Re-verifies the admin's password and mints a short-lived confirm token for
/// the requested action.  The token is returned to the caller and must be
/// supplied as `x-confirm-token` on admin mutation requests.
async fn confirm(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(body): Json<ConfirmRequest>,
) -> Result<Json<ConfirmResponse>, AppError> {
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

/// Read `x-confirm-token` from request headers, decode it, and verify that its
/// `action` claim matches `expected_action`.
///
/// Returns `AppError::Forbidden` if the header is missing, the token is invalid
/// or expired, or the action does not match.
pub fn validate_confirm_token(
    headers: &HeaderMap,
    secret: &str,
    expected_action: &str,
) -> Result<ConfirmClaims, AppError> {
    let token_str = headers
        .get("x-confirm-token")
        .ok_or(AppError::Forbidden)?
        .to_str()
        .map_err(|_| AppError::Forbidden)?;

    decode_confirm_token(token_str, secret, expected_action)
}

// ── Audit log reader ──────────────────────────────────────────────────────────

/// Query parameters for `GET /api/admin/audit`.
#[derive(Debug, Deserialize)]
struct AuditQuery {
    /// Cursor: return rows created before the row with this id.
    before: Option<Uuid>,
    /// Optional action prefix filter (e.g. `auth.login` matches `auth.login.success`).
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
async fn audit_list(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(params): Query<AuditQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);

    // If a `before` cursor is supplied, resolve it to a timestamp.
    let before_ts: Option<DateTime<Utc>> = if let Some(before_id) = params.before {
        let ts = sqlx::query_scalar::<_, DateTime<Utc>>(
            "SELECT created_at FROM audit_log WHERE id = $1",
        )
        .bind(before_id)
        .fetch_optional(&state.pool)
        .await?;
        // If the cursor row doesn't exist return empty — better than an error.
        match ts {
            Some(t) => Some(t),
            None => return Ok(Json(json!([]))),
        }
    } else {
        None
    };

    let rows = match (&before_ts, &params.action) {
        (Some(ts), Some(action_prefix)) => {
            let pattern = format!("{action_prefix}%");
            sqlx::query_as::<_, AuditRow>(
                r#"
                SELECT id, actor_id, actor_email, action, target_type, target_id,
                       payload, ip::text, user_agent, created_at
                FROM audit_log
                WHERE created_at < $1
                  AND action LIKE $2
                ORDER BY created_at DESC
                LIMIT $3
                "#,
            )
            .bind(ts)
            .bind(pattern)
            .bind(limit)
            .fetch_all(&state.pool)
            .await?
        }
        (Some(ts), None) => {
            sqlx::query_as::<_, AuditRow>(
                r#"
                SELECT id, actor_id, actor_email, action, target_type, target_id,
                       payload, ip::text, user_agent, created_at
                FROM audit_log
                WHERE created_at < $1
                ORDER BY created_at DESC
                LIMIT $2
                "#,
            )
            .bind(ts)
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
                ORDER BY created_at DESC
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
                ORDER BY created_at DESC
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
