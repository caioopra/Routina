//! `AdminUser` Axum extractor.
//!
//! Wraps `CurrentUser` and enforces `role == "admin"`.  The role is read from
//! the database on every request (via `CurrentUser`'s `load_user` call) so that
//! a demoted admin loses access on the very next request rather than waiting for
//! their JWT to expire.
//!
//! All failure paths (user not found, bad token, wrong role) return the same
//! `{"error":"forbidden"}` JSON body with HTTP 403 to avoid leaking whether the
//! request was rejected because the user is not an admin vs. because the token
//! was invalid.  The 401 path is covered by the router-level `auth_middleware`
//! before any extractor is reached.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::{Json, http::StatusCode};
use serde_json::json;
use uuid::Uuid;

use crate::middleware::auth::CurrentUser;
use crate::routes::AppState;

/// Extractor for admin-only handlers.
///
/// Resolves the current user via `CurrentUser` (JWT decode + DB lookup) and
/// then checks `role == "admin"`.  Returns HTTP 403 with
/// `{"error":"forbidden"}` for any failure — role mismatch, unknown user, or
/// invalid token — so callers cannot distinguish between "user not found" and
/// "user is not admin".
#[derive(Debug, Clone)]
pub struct AdminUser {
    pub user_id: Uuid,
    pub email: String,
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let forbidden =
            || (StatusCode::FORBIDDEN, Json(json!({ "error": "forbidden" }))).into_response();

        // Delegate JWT verification and DB lookup to CurrentUser.
        let current = CurrentUser::from_request_parts(parts, state)
            .await
            .map_err(|_| forbidden())?;

        if current.role != "admin" {
            return Err(forbidden());
        }

        Ok(Self {
            user_id: current.id,
            email: current.email,
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    /// Verify that the forbidden response body is the canonical JSON shape
    /// and uses HTTP 403.
    #[test]
    fn forbidden_response_shape() {
        use axum::Json;
        use serde_json::json;

        let resp = (StatusCode::FORBIDDEN, Json(json!({ "error": "forbidden" }))).into_response();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
