use std::collections::HashMap;

use axum::http::StatusCode;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, put},
};
use chrono::NaiveTime;
use serde::Deserialize;
use uuid::Uuid;

use crate::middleware::CurrentUser;
use crate::middleware::error::AppError;
use crate::models::block::{Block, BlockResponse, CreateBlockRequest, UpdateBlockRequest};
use crate::models::label::LabelResponse;

use super::{AppState, validate_length, verify_routine_owned};

/// Allowed values for the `type` column.
const BLOCK_TYPES: &[&str] = &[
    "trabalho",
    "mestrado",
    "aula",
    "exercicio",
    "slides",
    "viagem",
    "livre",
];

/// Sub-router for `/routines/:routine_id/blocks` (GET + POST).
pub fn nested_router() -> Router<AppState> {
    Router::new().route("/", get(list_blocks).post(create_block))
}

/// Sub-router for `/blocks/:id` (PUT + DELETE).
pub fn flat_router() -> Router<AppState> {
    Router::new().route("/{id}", put(update_block).delete(delete_block))
}

#[derive(Debug, Deserialize)]
struct DayFilter {
    day: Option<i16>,
}

const BLOCK_SELECT: &str = "id, routine_id, day_of_week, start_time, end_time, title, type, note, sort_order, created_at, updated_at";

async fn list_blocks(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(routine_id): Path<Uuid>,
    Query(filter): Query<DayFilter>,
) -> Result<Json<Vec<BlockResponse>>, AppError> {
    verify_routine_owned(&state.pool, user.id, routine_id).await?;

    // Validate optional day filter.
    if let Some(day) = filter.day
        && !(0..=6).contains(&day)
    {
        return Err(AppError::Validation("day must be between 0 and 6".into()));
    }

    let blocks: Vec<Block> = if let Some(day) = filter.day {
        sqlx::query_as::<_, Block>(&format!(
            "SELECT {BLOCK_SELECT} FROM blocks \
             WHERE routine_id = $1 AND day_of_week = $2 \
             ORDER BY day_of_week ASC, sort_order ASC, start_time ASC"
        ))
        .bind(routine_id)
        .bind(day)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Block>(&format!(
            "SELECT {BLOCK_SELECT} FROM blocks \
             WHERE routine_id = $1 \
             ORDER BY day_of_week ASC, sort_order ASC, start_time ASC"
        ))
        .bind(routine_id)
        .fetch_all(&state.pool)
        .await?
    };

    // Collect labels for all blocks in a single query — O(1) lookup per block.
    let block_ids: Vec<Uuid> = blocks.iter().map(|b| b.id).collect();
    let mut labels_map = fetch_labels_for_blocks(&state, &block_ids, user.id).await?;

    let responses = blocks
        .into_iter()
        .map(|b| {
            let labels = labels_map.remove(&b.id).unwrap_or_default();
            BlockResponse::from_block(b, labels)
        })
        .collect();

    Ok(Json(responses))
}

async fn create_block(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(routine_id): Path<Uuid>,
    Json(body): Json<CreateBlockRequest>,
) -> Result<(StatusCode, Json<BlockResponse>), AppError> {
    verify_routine_owned(&state.pool, user.id, routine_id).await?;

    let (start, end) = validate_block_fields(
        body.day_of_week,
        &body.start_time,
        body.end_time.as_deref(),
        &body.title,
        body.note.as_deref(),
        &body.block_type,
    )?;

    let sort_order = body.sort_order.unwrap_or(0);
    let id = Uuid::now_v7();

    let block = sqlx::query_as::<_, Block>(&format!(
        "INSERT INTO blocks (id, routine_id, day_of_week, start_time, end_time, title, type, note, sort_order) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING {BLOCK_SELECT}"
    ))
    .bind(id)
    .bind(routine_id)
    .bind(body.day_of_week)
    .bind(start)
    .bind(end)
    .bind(&body.title)
    .bind(&body.block_type)
    .bind(body.note.as_deref())
    .bind(sort_order)
    .fetch_one(&state.pool)
    .await?;

    let response = BlockResponse::from_block(block, vec![]);
    Ok((StatusCode::CREATED, Json(response)))
}

async fn update_block(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBlockRequest>,
) -> Result<Json<BlockResponse>, AppError> {
    // Verify ownership via routine join.
    fetch_owned_block(&state, user.id, id).await?;

    // Validate only the provided fields.
    if let Some(day) = body.day_of_week
        && !(0..=6).contains(&day)
    {
        return Err(AppError::Validation(
            "day_of_week must be between 0 and 6".into(),
        ));
    }
    if let Some(ref title) = body.title {
        if title.trim().is_empty() {
            return Err(AppError::Validation("title cannot be empty".into()));
        }
        validate_length("title", title, 200)?;
    }
    if let Some(ref note) = body.note {
        validate_length("note", note, 2000)?;
    }
    if let Some(ref t) = body.block_type
        && !BLOCK_TYPES.contains(&t.as_str())
    {
        return Err(AppError::Validation(format!(
            "unknown block type '{t}'; allowed: {}",
            BLOCK_TYPES.join(", ")
        )));
    }

    // Parse times exactly once; use those parsed values in the SQL binding.
    let start_time = body.start_time.as_deref().map(parse_time).transpose()?;
    let end_time = body.end_time.as_deref().map(parse_time).transpose()?;

    // If both are provided in this update, enforce ordering.
    if let (Some(start), Some(end)) = (start_time, end_time)
        && end <= start
    {
        return Err(AppError::Validation(
            "end_time must be strictly after start_time".into(),
        ));
    }

    let updated = sqlx::query_as::<_, Block>(&format!(
        "UPDATE blocks SET \
            day_of_week = COALESCE($1, day_of_week), \
            start_time  = COALESCE($2, start_time), \
            end_time    = CASE WHEN $3::bool THEN $4 ELSE end_time END, \
            title       = COALESCE($5, title), \
            type        = COALESCE($6, type), \
            note        = CASE WHEN $7::bool THEN $8 ELSE note END, \
            sort_order  = COALESCE($9, sort_order), \
            updated_at  = now() \
         WHERE id = $10 \
         RETURNING {BLOCK_SELECT}"
    ))
    .bind(body.day_of_week)
    .bind(start_time)
    .bind(body.end_time.is_some())
    .bind(end_time)
    .bind(body.title.as_deref())
    .bind(body.block_type.as_deref())
    .bind(body.note.is_some())
    .bind(body.note.as_deref())
    .bind(body.sort_order)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    // Re-fetch labels for this block.
    let mut labels_map = fetch_labels_for_blocks(&state, &[updated.id], user.id).await?;
    let labels = labels_map.remove(&updated.id).unwrap_or_default();

    Ok(Json(BlockResponse::from_block(updated, labels)))
}

async fn delete_block(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    fetch_owned_block(&state, user.id, id).await?;

    let affected = sqlx::query("DELETE FROM blocks WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_time(s: &str) -> Result<NaiveTime, AppError> {
    NaiveTime::parse_from_str(s, "%H:%M")
        .map_err(|_| AppError::Validation(format!("invalid time format '{s}', expected HH:MM")))
}

/// Validates a full block creation payload and returns the parsed `(start, Option<end>)` times.
/// All validation for create-time fields lives here; callers bind the returned
/// `NaiveTime` values directly, eliminating any chance of a re-parse mismatch.
fn validate_block_fields(
    day_of_week: i16,
    start_time: &str,
    end_time: Option<&str>,
    title: &str,
    note: Option<&str>,
    block_type: &str,
) -> Result<(NaiveTime, Option<NaiveTime>), AppError> {
    if !(0..=6).contains(&day_of_week) {
        return Err(AppError::Validation(
            "day_of_week must be between 0 and 6".into(),
        ));
    }
    if title.trim().is_empty() {
        return Err(AppError::Validation("title is required".into()));
    }
    validate_length("title", title, 200)?;
    if let Some(n) = note {
        validate_length("note", n, 2000)?;
    }
    if !BLOCK_TYPES.contains(&block_type) {
        return Err(AppError::Validation(format!(
            "unknown block type '{block_type}'; allowed: {}",
            BLOCK_TYPES.join(", ")
        )));
    }
    let start = parse_time(start_time)?;
    let end = end_time.map(parse_time).transpose()?;

    if let Some(e) = end
        && e <= start
    {
        return Err(AppError::Validation(
            "end_time must be strictly after start_time".into(),
        ));
    }

    Ok((start, end))
}

async fn fetch_owned_block(
    state: &AppState,
    user_id: Uuid,
    block_id: Uuid,
) -> Result<Block, AppError> {
    sqlx::query_as::<_, Block>(
        "SELECT b.id, b.routine_id, b.day_of_week, b.start_time, b.end_time, \
                b.title, b.type, b.note, b.sort_order, b.created_at, b.updated_at \
         FROM blocks b \
         JOIN routines rt ON rt.id = b.routine_id \
         WHERE b.id = $1 AND rt.user_id = $2",
    )
    .bind(block_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// Intermediate row for the block-labels join query.
#[derive(sqlx::FromRow)]
struct BlockLabelRow {
    block_id: Uuid,
    id: Uuid,
    name: String,
    color_bg: String,
    color_text: String,
    color_border: String,
    icon: Option<String>,
    is_default: bool,
}

/// Returns a `HashMap<block_id, Vec<LabelResponse>>` for all requested block IDs.
/// O(1) per lookup at the call site. The `user_id` filter is defense-in-depth
/// to prevent cross-user label leakage even if a write path mis-assigns labels.
async fn fetch_labels_for_blocks(
    state: &AppState,
    block_ids: &[Uuid],
    user_id: Uuid,
) -> Result<HashMap<Uuid, Vec<LabelResponse>>, AppError> {
    if block_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows: Vec<BlockLabelRow> = sqlx::query_as(
        "SELECT bl.block_id, l.id, l.name, l.color_bg, l.color_text, l.color_border, l.icon, l.is_default \
         FROM block_labels bl \
         JOIN labels l ON l.id = bl.label_id \
         WHERE bl.block_id = ANY($1) AND l.user_id = $2",
    )
    .bind(block_ids)
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;

    let mut map: HashMap<Uuid, Vec<LabelResponse>> = HashMap::new();
    for row in rows {
        map.entry(row.block_id).or_default().push(LabelResponse {
            id: row.id,
            name: row.name,
            color_bg: row.color_bg,
            color_text: row.color_text,
            color_border: row.color_border,
            icon: row.icon,
            is_default: row.is_default,
        });
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_time_valid() {
        assert!(parse_time("09:00").is_ok());
        assert!(parse_time("23:59").is_ok());
        assert!(parse_time("00:00").is_ok());
    }

    #[test]
    fn parse_time_invalid() {
        // chrono's %H:%M also accepts single-digit hours ("9:00"), which is fine
        // — we don't need to reject them since the DB stores TIME natively.
        assert!(parse_time("25:00").is_err());
        assert!(parse_time("not-a-time").is_err());
        assert!(parse_time("9am").is_err());
    }

    #[test]
    fn validate_block_fields_bad_day() {
        let err = validate_block_fields(7, "09:00", None, "title", None, "trabalho");
        assert!(matches!(err, Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_block_fields_empty_title() {
        let err = validate_block_fields(1, "09:00", None, "  ", None, "trabalho");
        assert!(matches!(err, Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_block_fields_bad_time() {
        let err = validate_block_fields(1, "9am", None, "title", None, "trabalho");
        assert!(matches!(err, Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_block_fields_ok() {
        assert!(
            validate_block_fields(0, "09:00", Some("10:30"), "title", None, "trabalho").is_ok()
        );
        assert!(validate_block_fields(6, "23:00", None, "title", None, "livre").is_ok());
    }

    #[test]
    fn validate_block_fields_returns_parsed_times() {
        let (start, end) =
            validate_block_fields(1, "09:00", Some("10:30"), "title", None, "trabalho").unwrap();
        assert_eq!(start, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        assert_eq!(end, Some(NaiveTime::from_hms_opt(10, 30, 0).unwrap()));
    }

    #[test]
    fn validate_block_fields_end_before_start_rejected() {
        let err = validate_block_fields(1, "10:00", Some("09:00"), "title", None, "trabalho");
        assert!(matches!(err, Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_block_fields_end_equal_start_rejected() {
        let err = validate_block_fields(1, "10:00", Some("10:00"), "title", None, "trabalho");
        assert!(matches!(err, Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_block_fields_unknown_type_rejected() {
        let err = validate_block_fields(1, "09:00", None, "title", None, "invalid_type");
        assert!(matches!(err, Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_block_fields_all_known_types_accepted() {
        for t in BLOCK_TYPES {
            assert!(
                validate_block_fields(0, "09:00", None, "title", None, t).is_ok(),
                "type '{t}' should be valid"
            );
        }
    }

    #[test]
    fn validate_block_fields_title_too_long_rejected() {
        let long_title = "a".repeat(201);
        let err = validate_block_fields(1, "09:00", None, &long_title, None, "trabalho");
        assert!(matches!(err, Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_block_fields_note_too_long_rejected() {
        let long_note = "a".repeat(2001);
        let err = validate_block_fields(1, "09:00", None, "title", Some(&long_note), "trabalho");
        assert!(matches!(err, Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_block_fields_end_after_start_ok() {
        let result = validate_block_fields(1, "08:00", Some("09:00"), "title", None, "trabalho");
        assert!(result.is_ok());
    }
}
