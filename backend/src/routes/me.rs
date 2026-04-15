use axum::{Json, Router, extract::State, routing::put};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::CurrentUser;
use crate::middleware::error::AppError;

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/planner-context", put(update_planner_context))
}

#[derive(Debug, Deserialize)]
pub struct UpdatePlannerContextRequest {
    pub planner_context: String,
}

/// Public user shape returned by both `GET /auth/me` and this endpoint.
/// Mirrors the `MeResponse` in `auth.rs` but is defined here so `me.rs` is
/// self-contained and the two modules can evolve independently.
#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub planner_context: Option<String>,
    pub preferences: serde_json::Value,
}

async fn update_planner_context(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<UpdatePlannerContextRequest>,
) -> Result<Json<MeResponse>, AppError> {
    // Treat empty/whitespace-only string as NULL.
    let trimmed = body.planner_context.trim();
    let new_ctx: Option<&str> = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    };

    let row = sqlx::query!(
        "UPDATE users \
         SET planner_context = $1, updated_at = now() \
         WHERE id = $2 \
         RETURNING id, email, name, planner_context, preferences",
        new_ctx,
        user.id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    Ok(Json(MeResponse {
        id: row.id,
        email: row.email,
        name: row.name,
        planner_context: row.planner_context,
        preferences: row.preferences,
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_string_becomes_none() {
        // Mirrors the trim+check logic in the handler.
        let inputs = ["", "   ", "\t\n"];
        for input in inputs {
            let trimmed = input.trim();
            let result: Option<&str> = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            };
            assert!(result.is_none(), "expected None for {:?}", input);
        }
    }

    #[test]
    fn non_empty_string_preserved() {
        let input = "  Sou engenheiro  ";
        let trimmed = input.trim();
        let result: Option<&str> = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
        assert_eq!(result, Some("Sou engenheiro"));
    }
}
