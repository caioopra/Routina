use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::error::AppError;

/// How long a confirm token is valid (5 minutes).
const CONFIRM_TOKEN_TTL_SECS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenKind {
    Access,
    Refresh,
    Confirm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: i64,
    pub iat: i64,
    pub typ: TokenKind,
}

/// Claims for a step-up confirm token.
///
/// A confirm token is short-lived (5 minutes) and scoped to a single `action`
/// string (e.g. `"provider.update"`).  The action must match when the token is
/// decoded, preventing one confirm token from authorising a different operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmClaims {
    pub sub: Uuid,
    pub exp: i64,
    pub iat: i64,
    pub typ: TokenKind,
    /// The admin action this token authorises, e.g. `"provider.update"`.
    pub action: String,
}

/// Mint a short-lived confirm token tied to a specific admin action.
pub fn encode_confirm_token(user_id: Uuid, action: &str, secret: &str) -> Result<String, AppError> {
    let now = Utc::now().timestamp();
    let claims = ConfirmClaims {
        sub: user_id,
        iat: now,
        exp: now + CONFIRM_TOKEN_TTL_SECS,
        typ: TokenKind::Confirm,
        action: action.to_owned(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("confirm token encode failed: {e}")))
}

/// Decode and validate a confirm token.
///
/// Returns `AppError::Unauthorized` if the token is invalid or expired.
/// Returns `AppError::Forbidden` if the `action` claim does not match
/// `expected_action` — preventing one confirm token from being used for a
/// different operation.
pub fn decode_confirm_token(
    token: &str,
    secret: &str,
    expected_action: &str,
) -> Result<ConfirmClaims, AppError> {
    let mut validation = Validation::default();
    validation.leeway = 0;
    let data = decode::<ConfirmClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| AppError::Unauthorized)?;

    let claims = data.claims;
    if claims.typ != TokenKind::Confirm {
        return Err(AppError::Unauthorized);
    }
    if claims.action != expected_action {
        return Err(AppError::Forbidden);
    }
    Ok(claims)
}

pub fn encode_token(
    user_id: Uuid,
    kind: TokenKind,
    secret: &str,
    expiration_seconds: i64,
) -> Result<String, AppError> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: user_id,
        iat: now,
        exp: now + expiration_seconds,
        typ: kind,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("jwt encode failed: {e}")))
}

pub fn decode_token(token: &str, secret: &str) -> Result<Claims, AppError> {
    let mut validation = Validation::default();
    validation.leeway = 0;
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| AppError::Unauthorized)?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-please-change";

    #[test]
    fn encode_then_decode_roundtrips() {
        let id = Uuid::now_v7();
        let token = encode_token(id, TokenKind::Access, SECRET, 60).unwrap();
        let claims = decode_token(&token, SECRET).unwrap();
        assert_eq!(claims.sub, id);
        assert_eq!(claims.typ, TokenKind::Access);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn decode_rejects_wrong_secret() {
        let token = encode_token(Uuid::now_v7(), TokenKind::Access, SECRET, 60).unwrap();
        let err = decode_token(&token, "different-secret").unwrap_err();
        assert!(matches!(err, AppError::Unauthorized));
    }

    #[test]
    fn decode_rejects_expired_token() {
        let token = encode_token(Uuid::now_v7(), TokenKind::Access, SECRET, -10).unwrap();
        let err = decode_token(&token, SECRET).unwrap_err();
        assert!(matches!(err, AppError::Unauthorized));
    }

    #[test]
    fn decode_rejects_garbage() {
        let err = decode_token("not.a.jwt", SECRET).unwrap_err();
        assert!(matches!(err, AppError::Unauthorized));
    }

    #[test]
    fn token_kind_serializes_as_lowercase() {
        let token = encode_token(Uuid::now_v7(), TokenKind::Refresh, SECRET, 60).unwrap();
        let claims = decode_token(&token, SECRET).unwrap();
        assert_eq!(claims.typ, TokenKind::Refresh);
    }

    // ── confirm token tests ────────────────────────────────────────────────────

    #[test]
    fn confirm_token_roundtrip() {
        let id = Uuid::now_v7();
        let action = "provider.update";
        let token = encode_confirm_token(id, action, SECRET).unwrap();
        let claims = decode_confirm_token(&token, SECRET, action).unwrap();
        assert_eq!(claims.sub, id);
        assert_eq!(claims.action, action);
        assert_eq!(claims.typ, TokenKind::Confirm);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn confirm_token_wrong_action_returns_forbidden() {
        let id = Uuid::now_v7();
        let token = encode_confirm_token(id, "provider.update", SECRET).unwrap();
        let err = decode_confirm_token(&token, SECRET, "kill_switch.toggle").unwrap_err();
        assert!(
            matches!(err, AppError::Forbidden),
            "wrong action must return Forbidden, got {err:?}"
        );
    }

    #[test]
    fn confirm_token_expired_returns_unauthorized() {
        // Use a negative TTL so the token is already expired.
        let id = Uuid::now_v7();
        // Manually mint an expired confirm token by manipulating the claims.
        let now = chrono::Utc::now().timestamp();
        let claims = ConfirmClaims {
            sub: id,
            iat: now - 600,
            exp: now - 300, // expired 5 minutes ago
            typ: TokenKind::Confirm,
            action: "provider.update".to_owned(),
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        let err = decode_confirm_token(&token, SECRET, "provider.update").unwrap_err();
        assert!(matches!(err, AppError::Unauthorized));
    }

    #[test]
    fn confirm_token_wrong_secret_returns_unauthorized() {
        let token = encode_confirm_token(Uuid::now_v7(), "provider.update", SECRET).unwrap();
        let err = decode_confirm_token(&token, "other-secret", "provider.update").unwrap_err();
        assert!(matches!(err, AppError::Unauthorized));
    }

    #[test]
    fn access_token_rejected_by_decode_confirm() {
        let id = Uuid::now_v7();
        let token = encode_token(id, TokenKind::Access, SECRET, 60).unwrap();
        let err = decode_confirm_token(&token, SECRET, "any.action").unwrap_err();
        assert!(
            matches!(err, AppError::Unauthorized),
            "access token must not be accepted as confirm token"
        );
    }
}
