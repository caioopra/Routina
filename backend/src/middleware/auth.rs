use axum::extract::{FromRequestParts, Request, State};
use axum::http::header;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{TokenKind, decode_token};
use crate::middleware::error::AppError;
use crate::models::user::User;
use crate::routes::AppState;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(&parts.headers)?;
        let claims = decode_token(&token, &state.config.jwt_secret)?;
        if claims.typ != TokenKind::Access {
            return Err(AppError::Unauthorized);
        }
        let user = load_user(&state.pool, claims.sub).await?;
        Ok(Self {
            id: user.id,
            email: user.email,
            name: user.name,
            role: user.role,
        })
    }
}

/// Router-level middleware that enforces JWT authentication.
///
/// Applied to the protected sub-router in `routes/mod.rs`.  Any request that
/// reaches this middleware without a valid Access token receives 401 immediately,
/// before any handler is invoked.  This provides defence-in-depth: even a handler
/// that accidentally omits the `CurrentUser` extractor cannot be reached without
/// a valid token.
pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let token = match bearer_token(request.headers()) {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };
    let claims = match decode_token(&token, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(_) => return AppError::Unauthorized.into_response(),
    };
    if claims.typ != TokenKind::Access {
        return AppError::Unauthorized.into_response();
    }
    next.run(request).await
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Result<String, AppError> {
    let raw = headers
        .get(header::AUTHORIZATION)
        .ok_or(AppError::Unauthorized)?
        .to_str()
        .map_err(|_| AppError::Unauthorized)?;
    raw.strip_prefix("Bearer ")
        .map(str::to_owned)
        .ok_or(AppError::Unauthorized)
}

pub(crate) async fn load_user(pool: &PgPool, id: Uuid) -> Result<User, AppError> {
    sqlx::query_as::<_, User>(
        "SELECT id, email, name, password_hash, preferences, created_at, updated_at, role \
         FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn bearer_token_extracts_value() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer abc.def.ghi".parse().unwrap());
        assert_eq!(bearer_token(&headers).unwrap(), "abc.def.ghi");
    }

    #[test]
    fn bearer_token_missing_header_unauthorized() {
        let headers = HeaderMap::new();
        assert!(matches!(
            bearer_token(&headers).unwrap_err(),
            AppError::Unauthorized
        ));
    }

    #[test]
    fn bearer_token_wrong_scheme_unauthorized() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Basic abc".parse().unwrap());
        assert!(matches!(
            bearer_token(&headers).unwrap_err(),
            AppError::Unauthorized
        ));
    }
}
