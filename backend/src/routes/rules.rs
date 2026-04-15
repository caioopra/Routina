use axum::http::StatusCode;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, put},
};
use uuid::Uuid;

use crate::middleware::CurrentUser;
use crate::middleware::error::AppError;
use crate::models::rule::{CreateRuleRequest, Rule, UpdateRuleRequest};

use super::{AppState, validate_length, verify_routine_owned};

/// Sub-router for `/routines/:routine_id/rules` (GET + POST).
pub fn nested_router() -> Router<AppState> {
    Router::new().route("/", get(list_rules).post(create_rule))
}

/// Sub-router for `/rules/:id` (PUT + DELETE).
pub fn flat_router() -> Router<AppState> {
    Router::new().route("/{id}", put(update_rule).delete(delete_rule))
}

const SELECT_COLUMNS: &str = "id, routine_id, text, sort_order";

async fn list_rules(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Vec<Rule>>, AppError> {
    // Verify ownership of the routine (returns 404 if not found or not owned).
    verify_routine_owned(&state.pool, user.id, routine_id).await?;

    let rows = sqlx::query_as::<_, Rule>(&format!(
        "SELECT {SELECT_COLUMNS} FROM rules \
         WHERE routine_id = $1 ORDER BY sort_order ASC, id ASC"
    ))
    .bind(routine_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

async fn create_rule(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(routine_id): Path<Uuid>,
    Json(body): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<Rule>), AppError> {
    verify_routine_owned(&state.pool, user.id, routine_id).await?;

    if body.text.trim().is_empty() {
        return Err(AppError::Validation("text is required".into()));
    }
    validate_length("text", &body.text, 2000)?;

    let id = Uuid::now_v7();
    let sort_order = body.sort_order.unwrap_or(0);

    let rule = sqlx::query_as::<_, Rule>(&format!(
        "INSERT INTO rules (id, routine_id, text, sort_order) \
         VALUES ($1, $2, $3, $4) \
         RETURNING {SELECT_COLUMNS}"
    ))
    .bind(id)
    .bind(routine_id)
    .bind(&body.text)
    .bind(sort_order)
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(rule)))
}

async fn update_rule(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRuleRequest>,
) -> Result<Json<Rule>, AppError> {
    // Ownership is verified by joining rules -> routines -> user.
    fetch_owned_rule(&state, user.id, id).await?;

    if let Some(ref text) = body.text {
        if text.trim().is_empty() {
            return Err(AppError::Validation("text cannot be empty".into()));
        }
        validate_length("text", text, 2000)?;
    }

    let updated = sqlx::query_as::<_, Rule>(&format!(
        "UPDATE rules SET \
            text       = COALESCE($1, text), \
            sort_order = COALESCE($2, sort_order) \
         WHERE id = $3 \
         RETURNING {SELECT_COLUMNS}"
    ))
    .bind(body.text.as_deref())
    .bind(body.sort_order)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(updated))
}

async fn delete_rule(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    fetch_owned_rule(&state, user.id, id).await?;

    let affected = sqlx::query("DELETE FROM rules WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Fetches a rule and verifies that its parent routine belongs to the user.
async fn fetch_owned_rule(
    state: &AppState,
    user_id: Uuid,
    rule_id: Uuid,
) -> Result<Rule, AppError> {
    sqlx::query_as::<_, Rule>(
        "SELECT r.id, r.routine_id, r.text, r.sort_order \
         FROM rules r \
         JOIN routines rt ON rt.id = r.routine_id \
         WHERE r.id = $1 AND rt.user_id = $2",
    )
    .bind(rule_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_select_columns_constant() {
        // Sanity-check the columns we SELECT for rules.
        let cols: Vec<&str> = SELECT_COLUMNS.split(", ").collect();
        assert!(cols.contains(&"id"));
        assert!(cols.contains(&"routine_id"));
        assert!(cols.contains(&"text"));
        assert!(cols.contains(&"sort_order"));
    }

    #[test]
    fn validate_length_ok() {
        assert!(validate_length("text", "hello", 2000).is_ok());
        assert!(validate_length("text", &"a".repeat(2000), 2000).is_ok());
    }

    #[test]
    fn validate_length_exceeded() {
        let err = validate_length("text", &"a".repeat(2001), 2000);
        assert!(matches!(err, Err(AppError::Validation(_))));
    }
}
