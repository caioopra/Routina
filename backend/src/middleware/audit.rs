//! `emit_audit` helper for writing rows to the `audit_log` table.
//!
//! This is the *single* code path that inserts into `audit_log`.  It performs
//! application-layer stripping of forbidden payload keys (defense-in-depth on
//! top of the DB-level CHECK constraint) and returns the newly created row id.

use std::net::IpAddr;
use std::str::FromStr;

use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::error::AppError;

/// Forbidden top-level payload keys — mirrored from the DB CHECK constraint.
const FORBIDDEN_KEYS: &[&str] = &[
    "password",
    "password_hash",
    "token",
    "refresh_token",
    "secret",
    "api_key",
];

/// Write one row to `audit_log` and return the new row `id`.
///
/// Strips any `FORBIDDEN_KEYS` from the top level of `payload` before the
/// INSERT — this is defense-in-depth alongside the DB CHECK constraint.  The
/// `ip` parameter is parsed as an `IpAddr`; if parsing fails it is silently
/// dropped (we don't want a bad IP string to abort an audit write).
///
/// `actor_id` may be `None` for unauthenticated events (e.g. a failed login
/// attempt for an email that doesn't exist in the database).
#[allow(clippy::too_many_arguments)]
pub async fn emit_audit(
    pool: &PgPool,
    actor_id: Option<Uuid>,
    actor_email: &str,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
    payload: Option<serde_json::Value>,
    ip: Option<&str>,
    user_agent: Option<&str>,
) -> Result<Uuid, AppError> {
    // Strip forbidden keys from payload before we ever touch the DB.
    let clean_payload = payload.map(strip_forbidden_keys);

    // Parse IP — ignore invalid strings rather than failing the audit write.
    let parsed_ip: Option<IpAddr> = ip.and_then(|s| IpAddr::from_str(s).ok());
    let ip_str: Option<String> = parsed_ip.map(|a| a.to_string());

    let row_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO audit_log
            (actor_id, actor_email, action, target_type, target_id, payload, ip, user_agent)
        VALUES ($1, $2, $3, $4, $5, $6, $7::inet, $8)
        RETURNING id
        "#,
    )
    .bind(actor_id)
    .bind(actor_email)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(clean_payload)
    .bind(ip_str)
    .bind(user_agent)
    .fetch_one(pool)
    .await?;

    Ok(row_id)
}

/// Remove forbidden top-level keys from a JSON object.
///
/// Non-object values are returned unchanged.
fn strip_forbidden_keys(mut value: serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(ref mut map) = value {
        for key in FORBIDDEN_KEYS {
            map.remove(*key);
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strip_removes_password_key() {
        let payload = json!({ "password": "secret", "user_id": "123", "action": "login" });
        let clean = strip_forbidden_keys(payload);
        assert!(clean.get("password").is_none(), "password must be stripped");
        assert_eq!(clean["user_id"], "123");
        assert_eq!(clean["action"], "login");
    }

    #[test]
    fn strip_removes_all_forbidden_keys() {
        let payload = json!({
            "password": "x",
            "password_hash": "y",
            "token": "z",
            "refresh_token": "w",
            "secret": "s",
            "api_key": "k",
            "safe_field": "keep"
        });
        let clean = strip_forbidden_keys(payload);
        for key in FORBIDDEN_KEYS {
            assert!(
                clean.get(key).is_none(),
                "key '{key}' must be stripped from payload"
            );
        }
        assert_eq!(clean["safe_field"], "keep");
    }

    #[test]
    fn strip_leaves_non_object_unchanged() {
        let payload = json!([1, 2, 3]);
        let clean = strip_forbidden_keys(payload.clone());
        assert_eq!(clean, payload);
    }

    #[test]
    fn strip_handles_empty_object() {
        let payload = json!({});
        let clean = strip_forbidden_keys(payload);
        assert_eq!(clean, json!({}));
    }

    #[test]
    fn strip_leaves_safe_fields_intact() {
        let payload = json!({ "email": "user@example.com", "role": "admin" });
        let clean = strip_forbidden_keys(payload);
        assert_eq!(clean["email"], "user@example.com");
        assert_eq!(clean["role"], "admin");
    }
}
