use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenKind {
    Access,
    Refresh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: i64,
    pub iat: i64,
    pub typ: TokenKind,
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
}
