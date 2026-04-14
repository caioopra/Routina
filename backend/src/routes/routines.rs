use axum::http::StatusCode;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde_json::json;
use uuid::Uuid;

use crate::middleware::CurrentUser;
use crate::middleware::error::AppError;
use crate::models::routine::{
    CreateRoutineRequest, Routine, RoutineResponse, RoutineSummary, UpdateRoutineRequest,
};

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_routines).post(create_routine))
        .route(
            "/{id}",
            get(get_routine).put(update_routine).delete(delete_routine),
        )
        .route("/{id}/activate", post(activate_routine))
}

const SELECT_COLUMNS: &str = "id, user_id, name, period, is_active, meta, created_at, updated_at";

async fn list_routines(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<RoutineSummary>>, AppError> {
    let rows = sqlx::query_as::<_, Routine>(&format!(
        "SELECT {SELECT_COLUMNS} FROM routines \
         WHERE user_id = $1 ORDER BY created_at DESC"
    ))
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows.into_iter().map(RoutineSummary::from).collect()))
}

async fn create_routine(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<CreateRoutineRequest>,
) -> Result<(StatusCode, Json<RoutineSummary>), AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }

    let mut tx = state.pool.begin().await?;

    // If caller has zero routines, auto-activate the new one.
    let existing_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM routines WHERE user_id = $1")
            .bind(user.id)
            .fetch_one(&mut *tx)
            .await?;
    let is_active = existing_count == 0;

    let meta = body.meta.unwrap_or_else(|| json!({}));
    let id = Uuid::now_v7();

    let routine = sqlx::query_as::<_, Routine>(&format!(
        "INSERT INTO routines (id, user_id, name, period, is_active, meta) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING {SELECT_COLUMNS}"
    ))
    .bind(id)
    .bind(user.id)
    .bind(&body.name)
    .bind(body.period.as_deref())
    .bind(is_active)
    .bind(&meta)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(RoutineSummary::from(routine))))
}

async fn get_routine(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<RoutineResponse>, AppError> {
    let routine = fetch_owned(&state, user.id, id).await?;
    Ok(Json(RoutineResponse::from(routine)))
}

async fn update_routine(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRoutineRequest>,
) -> Result<Json<RoutineSummary>, AppError> {
    // Ensure ownership (and existence) up-front; 404 if not found.
    let _ = fetch_owned(&state, user.id, id).await?;

    if let Some(ref name) = body.name
        && name.trim().is_empty()
    {
        return Err(AppError::Validation("name cannot be empty".into()));
    }

    let updated = sqlx::query_as::<_, Routine>(&format!(
        "UPDATE routines SET \
            name = COALESCE($1, name), \
            period = CASE WHEN $2::bool THEN $3 ELSE period END, \
            meta = COALESCE($4, meta), \
            updated_at = now() \
         WHERE id = $5 AND user_id = $6 \
         RETURNING {SELECT_COLUMNS}"
    ))
    .bind(body.name.as_deref())
    .bind(body.period.is_some())
    .bind(body.period.as_deref())
    .bind(body.meta.as_ref())
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(RoutineSummary::from(updated)))
}

async fn activate_routine(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<RoutineSummary>, AppError> {
    let mut tx = state.pool.begin().await?;

    // Confirm target routine exists for this user inside the transaction so
    // the caller cannot observe a window where zero routines are active.
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM routines WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user.id)
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    // Deactivate all others.
    sqlx::query(
        "UPDATE routines SET is_active = false, updated_at = now() \
         WHERE user_id = $1 AND id <> $2",
    )
    .bind(user.id)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // Activate target.
    let routine = sqlx::query_as::<_, Routine>(&format!(
        "UPDATE routines SET is_active = true, updated_at = now() \
         WHERE id = $1 AND user_id = $2 \
         RETURNING {SELECT_COLUMNS}"
    ))
    .bind(id)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(RoutineSummary::from(routine)))
}

async fn delete_routine(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let affected = sqlx::query("DELETE FROM routines WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn fetch_owned(state: &AppState, user_id: Uuid, id: Uuid) -> Result<Routine, AppError> {
    sqlx::query_as::<_, Routine>(&format!(
        "SELECT {SELECT_COLUMNS} FROM routines \
         WHERE id = $1 AND user_id = $2"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)
}
