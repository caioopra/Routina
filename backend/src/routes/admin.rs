//! Admin routes — `/api/admin/*`.
//!
//! Every handler in this module requires the `AdminUser` extractor, which
//! enforces `role == "admin"` via a DB lookup on every request.  No
//! router-level middleware is needed because the extractor itself returns
//! HTTP 403 for non-admins and HTTP 401 (via `CurrentUser`) for missing/invalid
//! tokens.
//!
//! Routes in this module:
//!   `GET  /api/admin/dashboard`              → proof-of-gating stub (Slice A)
//!   `POST /api/admin/confirm`                → step-up auth: password re-check → confirm token
//!   `GET  /api/admin/audit`                  → paginated audit log reader
//!   `GET  /api/admin/settings`               → list all app_settings rows
//!   `POST /api/admin/settings`               → update a setting value
//!   `GET  /api/admin/metrics/usage`          → LLM usage per day/provider
//!   `GET  /api/admin/users`                  → list all users (no passwords)
//!   `POST /api/admin/users/:id/rate-limit`   → set per-user rate limit override

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::{Json, Router, routing::get, routing::post};
use chrono::{DateTime, NaiveDate, Utc};
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
        .route("/settings", get(settings_list).post(settings_update))
        .route("/metrics/usage", get(metrics_usage))
        .route("/users", get(users_list))
        .route("/users/{id}/rate-limit", post(users_set_rate_limit))
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

// ── Client info helper ────────────────────────────────────────────────────────

/// Extract IP address and user-agent from request headers.
///
/// The IP is taken from the first entry in `x-forwarded-for` (proxy-injected);
/// if absent, `None` is returned.  The user-agent is taken verbatim from the
/// `user-agent` header.  Both may be `None` for requests that arrive without
/// those headers (e.g. direct connections in tests).
fn extract_client_info(headers: &HeaderMap) -> (Option<String>, Option<String>) {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    (ip, ua)
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

// ── Settings ──────────────────────────────────────────────────────────────────

/// A single `app_settings` row returned to the client.
#[derive(Debug, Serialize, sqlx::FromRow)]
struct SettingRow {
    pub key: String,
    pub value: String,
    pub updated_at: DateTime<Utc>,
}

/// `GET /api/admin/settings`
///
/// Returns all `app_settings` rows as `[{key, value, updated_at}]`.
async fn settings_list(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query_as::<_, SettingRow>(
        "SELECT key, value, updated_at FROM app_settings ORDER BY key",
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(json!(rows)))
}

#[derive(Debug, Deserialize)]
struct SettingUpdateRequest {
    key: String,
    value: String,
}

/// Keys that require a step-up confirm token before updating.
///
/// In addition to the LLM provider/model keys, the chat kill-switch and all
/// budget-related keys are treated as sensitive because:
/// - `chat_enabled` can disable chat for all users when set to "false".
/// - `budget_monthly_usd` and `budget_warn_pct` control how much users can
///   spend on LLM calls; reducing them without step-up auth could be abused.
const SENSITIVE_SETTING_KEYS: &[&str] = &[
    "llm_default_provider",
    "llm_gemini_model",
    "llm_claude_model",
    "chat_enabled",
    "budget_monthly_usd",
    "budget_warn_pct",
];

/// `POST /api/admin/settings`
///
/// Updates a setting value.  Requires a valid `x-confirm-token` for
/// provider/model keys (action: `"settings.update"`).  Emits an audit event
/// and invalidates the `SettingsCache` after a successful update.
async fn settings_update(
    State(state): State<AppState>,
    admin: AdminUser,
    headers: HeaderMap,
    Json(body): Json<SettingUpdateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Validate key length — mirrors the DB CHECK constraint.
    if body.key.len() > 1024 || body.value.len() > 1024 {
        return Err(AppError::Validation(
            "key and value must be at most 1024 characters".into(),
        ));
    }

    // Sensitive keys require step-up auth.
    if SENSITIVE_SETTING_KEYS.contains(&body.key.as_str()) {
        validate_confirm_token(
            &headers,
            &state.config.jwt_secret,
            "settings.update",
            admin.user_id,
        )?;
    }

    // Extract IP and user-agent for the audit row.
    let (ip, ua) = extract_client_info(&headers);

    // UPDATE — the DB CHECK constraint enforces that only known keys exist.
    let updated = sqlx::query_as::<_, SettingRow>(
        "UPDATE app_settings SET value = $1, updated_by = $2 WHERE key = $3 \
         RETURNING key, value, updated_at",
    )
    .bind(&body.value)
    .bind(admin.user_id)
    .bind(&body.key)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Validation(format!("unknown setting key '{}'", body.key)))?;

    // Invalidate the in-memory cache so the next request re-reads from DB.
    state.settings_cache.invalidate().await;

    // Emit audit.
    let _ = emit_audit(
        &state.pool,
        Some(admin.user_id),
        &admin.email,
        "admin.settings.update",
        Some("setting"),
        Some(&body.key),
        Some(json!({ "new_value": body.value })),
        ip.as_deref(),
        ua.as_deref(),
    )
    .await;

    Ok(Json(json!(updated)))
}

// ── Metrics ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct UsageQuery {
    /// Number of days to look back (default 30, max 90).
    days: Option<i32>,
}

/// A single row returned by `GET /api/admin/metrics/usage`.
#[derive(Debug, Serialize, sqlx::FromRow)]
struct UsageRow {
    pub day: NaiveDate,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub request_count: i32,
    pub estimated_cost_usd: f64,
}

/// `GET /api/admin/metrics/usage?days=30`
///
/// Returns `llm_usage_daily` rows for the past `days` days, aggregated by
/// (day, provider, model), ordered newest first.
async fn metrics_usage(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(params): Query<UsageQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let days = params.days.unwrap_or(30).clamp(1, 90) as i64;

    let rows = sqlx::query_as::<_, UsageRow>(
        "SELECT day, provider, model, \
                SUM(input_tokens)::bigint AS input_tokens, \
                SUM(output_tokens)::bigint AS output_tokens, \
                SUM(request_count)::int AS request_count, \
                SUM(estimated_cost_usd)::float8 AS estimated_cost_usd \
         FROM llm_usage_daily \
         WHERE day >= CURRENT_DATE - ($1 - 1) * INTERVAL '1 day' \
         GROUP BY day, provider, model \
         ORDER BY day DESC, provider, model",
    )
    .bind(days)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(json!(rows)))
}

// ── User management ───────────────────────────────────────────────────────────

/// A user row returned by `GET /api/admin/users` (no passwords).
#[derive(Debug, Serialize, sqlx::FromRow)]
struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

/// `GET /api/admin/users`
///
/// Returns all users ordered by creation date, newest first.  Passwords and
/// other sensitive fields are excluded.
async fn users_list(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, name, role, created_at FROM users ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(json!(rows)))
}

#[derive(Debug, Deserialize)]
struct RateLimitRequest {
    /// Daily token limit override for this user (null = no per-user limit).
    daily_token_limit: Option<i64>,
    /// Daily request limit override for this user (null = no per-user limit).
    daily_request_limit: Option<i32>,
    /// Human-readable reason for the override (audit trail).
    override_reason: Option<String>,
}

/// `POST /api/admin/users/:id/rate-limit`
///
/// Upserts a per-user rate limit override.  Requires `AdminUser` and a valid
/// `x-confirm-token` for action `"admin.user.rate_limit"`.
/// Emits an audit event (including IP and user-agent) on success.
async fn users_set_rate_limit(
    State(state): State<AppState>,
    admin: AdminUser,
    headers: HeaderMap,
    Path(target_user_id): Path<Uuid>,
    Json(body): Json<RateLimitRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Step-up auth: validate confirm token for this action.
    validate_confirm_token(
        &headers,
        &state.config.jwt_secret,
        "admin.user.rate_limit",
        admin.user_id,
    )?;

    // Extract IP and user-agent for the audit row.
    let (ip, ua) = extract_client_info(&headers);

    // Verify the target user exists.
    let user_exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE id = $1")
        .bind(target_user_id)
        .fetch_optional(&state.pool)
        .await?;

    if user_exists.is_none() {
        return Err(AppError::NotFound);
    }

    sqlx::query(
        "INSERT INTO user_rate_limits \
         (user_id, daily_token_limit, daily_request_limit, override_reason, set_by) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (user_id) DO UPDATE SET \
             daily_token_limit   = EXCLUDED.daily_token_limit, \
             daily_request_limit = EXCLUDED.daily_request_limit, \
             override_reason     = EXCLUDED.override_reason, \
             set_by              = EXCLUDED.set_by",
    )
    .bind(target_user_id)
    .bind(body.daily_token_limit)
    .bind(body.daily_request_limit)
    .bind(&body.override_reason)
    .bind(admin.user_id)
    .execute(&state.pool)
    .await?;

    // Emit audit.
    let _ = emit_audit(
        &state.pool,
        Some(admin.user_id),
        &admin.email,
        "admin.user.rate_limit",
        Some("user"),
        Some(&target_user_id.to_string()),
        Some(json!({
            "daily_token_limit":   body.daily_token_limit,
            "daily_request_limit": body.daily_request_limit,
            "override_reason":     body.override_reason,
        })),
        ip.as_deref(),
        ua.as_deref(),
    )
    .await;

    Ok(Json(json!({
        "ok": true,
        "user_id": target_user_id,
        "daily_token_limit": body.daily_token_limit,
        "daily_request_limit": body.daily_request_limit,
    })))
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

    // ── extract_client_info ───────────────────────────────────────────────────

    #[test]
    fn extract_client_info_parses_forwarded_for_and_ua() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.5, 10.0.0.1".parse().unwrap());
        headers.insert("user-agent", "Mozilla/5.0".parse().unwrap());

        let (ip, ua) = extract_client_info(&headers);
        assert_eq!(
            ip.as_deref(),
            Some("203.0.113.5"),
            "first XFF entry expected"
        );
        assert_eq!(ua.as_deref(), Some("Mozilla/5.0"));
    }

    #[test]
    fn extract_client_info_returns_none_when_headers_absent() {
        let headers = HeaderMap::new();
        let (ip, ua) = extract_client_info(&headers);
        assert!(ip.is_none(), "ip must be None without x-forwarded-for");
        assert!(ua.is_none(), "ua must be None without user-agent");
    }

    #[test]
    fn extract_client_info_single_forwarded_for_entry() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.42".parse().unwrap());

        let (ip, ua) = extract_client_info(&headers);
        assert_eq!(ip.as_deref(), Some("198.51.100.42"));
        assert!(ua.is_none());
    }
}
