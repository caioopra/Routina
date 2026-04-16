use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found")]
    NotFound,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(String),

    /// Monthly LLM budget exceeded.
    ///
    /// Intentionally a unit variant: we do not return spend/budget values in
    /// the HTTP response to avoid leaking financial data.  The user receives
    /// budget warnings via the `budget_warning` field in SSE `done` events.
    #[error("Monthly budget exceeded")]
    BudgetExceeded,

    /// Service unavailable (e.g. kill-switch engaged).
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            AppError::BudgetExceeded => {
                // Return only the error code — no spend or budget values to
                // avoid leaking financial data in HTTP responses.
                let body = json!({ "error": "budget_exceeded" });
                return (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response();
            }
            AppError::ServiceUnavailable(reason) => {
                let body = json!({ "error": reason });
                return (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response();
            }
            _ => {}
        }

        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden".to_string()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::Validation(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            AppError::Database(err) => {
                tracing::error!("Database error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            // Already handled above; unreachable but needed for exhaustiveness.
            AppError::BudgetExceeded | AppError::ServiceUnavailable(_) => unreachable!(),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_status() {
        let response = AppError::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_unauthorized_status() {
        let response = AppError::Unauthorized.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_bad_request_status() {
        let response = AppError::BadRequest("invalid".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_conflict_status() {
        let response = AppError::Conflict("email exists".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_validation_status() {
        let response = AppError::Validation("missing field".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_internal_hides_details() {
        let response = AppError::Internal("secret details".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_budget_exceeded_returns_429() {
        let response = AppError::BudgetExceeded.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_budget_exceeded_body_omits_financial_data() {
        use http_body_util::BodyExt;

        let response = AppError::BudgetExceeded.into_response();
        // Collect body synchronously using a one-shot runtime.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let bytes = rt
            .block_on(response.into_body().collect())
            .unwrap()
            .to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Must have exactly one key: "error".
        assert_eq!(
            body["error"].as_str().unwrap(),
            "budget_exceeded",
            "error field must be budget_exceeded"
        );
        assert!(
            body.get("monthly_spend").is_none(),
            "monthly_spend must not appear in response body"
        );
        assert!(
            body.get("budget").is_none(),
            "budget must not appear in response body"
        );
    }

    #[test]
    fn test_service_unavailable_returns_503() {
        let response = AppError::ServiceUnavailable("chat_disabled".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
