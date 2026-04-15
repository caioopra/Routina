use axum::http::StatusCode;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, put},
};
use once_cell::sync::Lazy;
use regex::Regex;
use uuid::Uuid;

use crate::middleware::CurrentUser;
use crate::middleware::error::AppError;
use crate::models::label::{CreateLabelRequest, Label, LabelResponse, UpdateLabelRequest};

use super::{AppState, validate_length};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_labels).post(create_label))
        .route("/{id}", put(update_label).delete(delete_label))
}

const SELECT_COLUMNS: &str =
    "id, user_id, name, color_bg, color_text, color_border, icon, is_default";

/// Accepts CSS hex colors in 3, 4, 6, or 8 hex-digit form: `#rgb`, `#rgba`,
/// `#rrggbb`, `#rrggbbaa`.
static HEX_COLOR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^#[0-9a-fA-F]{3,8}$").expect("valid regex"));

fn validate_hex_color(field: &str, value: &str) -> Result<(), AppError> {
    if !HEX_COLOR_RE.is_match(value) {
        return Err(AppError::Validation(format!(
            "{field} must be a valid hex color (e.g. #rgb or #rrggbb), got '{value}'"
        )));
    }
    Ok(())
}

async fn list_labels(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<LabelResponse>>, AppError> {
    let rows = sqlx::query_as::<_, Label>(&format!(
        "SELECT {SELECT_COLUMNS} FROM labels \
         WHERE user_id = $1 ORDER BY is_default DESC, name ASC"
    ))
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows.into_iter().map(LabelResponse::from).collect()))
}

async fn create_label(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<CreateLabelRequest>,
) -> Result<(StatusCode, Json<LabelResponse>), AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    validate_length("name", &body.name, 100)?;
    if body.color_bg.trim().is_empty() {
        return Err(AppError::Validation("color_bg is required".into()));
    }
    validate_hex_color("color_bg", &body.color_bg)?;
    if body.color_text.trim().is_empty() {
        return Err(AppError::Validation("color_text is required".into()));
    }
    validate_hex_color("color_text", &body.color_text)?;
    if body.color_border.trim().is_empty() {
        return Err(AppError::Validation("color_border is required".into()));
    }
    validate_hex_color("color_border", &body.color_border)?;
    if let Some(ref icon) = body.icon {
        validate_length("icon", icon, 100)?;
    }

    let id = Uuid::now_v7();
    let label = sqlx::query_as::<_, Label>(&format!(
        "INSERT INTO labels (id, user_id, name, color_bg, color_text, color_border, icon, is_default) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, false) \
         RETURNING {SELECT_COLUMNS}"
    ))
    .bind(id)
    .bind(user.id)
    .bind(&body.name)
    .bind(&body.color_bg)
    .bind(&body.color_text)
    .bind(&body.color_border)
    .bind(body.icon.as_deref())
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            // Unique constraint violation: user_id + name
            if db_err.code().as_deref() == Some("23505") {
                return AppError::Conflict("a label with that name already exists".into());
            }
        }
        AppError::Database(e)
    })?;

    Ok((StatusCode::CREATED, Json(LabelResponse::from(label))))
}

async fn update_label(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateLabelRequest>,
) -> Result<Json<LabelResponse>, AppError> {
    // Ownership check — also surfaces 404 for non-existent labels.
    let label = fetch_owned(&state, user.id, id).await?;

    if label.is_default {
        return Err(AppError::Conflict(
            "default labels cannot be modified".into(),
        ));
    }

    if let Some(ref name) = body.name {
        if name.trim().is_empty() {
            return Err(AppError::Validation("name cannot be empty".into()));
        }
        validate_length("name", name, 100)?;
    }
    if let Some(ref color_bg) = body.color_bg {
        validate_hex_color("color_bg", color_bg)?;
    }
    if let Some(ref color_text) = body.color_text {
        validate_hex_color("color_text", color_text)?;
    }
    if let Some(ref color_border) = body.color_border {
        validate_hex_color("color_border", color_border)?;
    }
    // icon: None means "field absent — do not touch". Some(None) means "set to NULL".
    // Some(Some(v)) means "set to v".
    if let Some(Some(ref icon_value)) = body.icon {
        validate_length("icon", icon_value, 100)?;
    }

    // Drive the CASE from the outer Option: field present in JSON → update the column.
    let icon_present = body.icon.is_some();
    // Flatten to the actual DB value (Option<&str>).
    let icon_value: Option<&str> = body.icon.as_ref().and_then(|o| o.as_deref());

    let updated = sqlx::query_as::<_, Label>(&format!(
        "UPDATE labels SET \
            name        = COALESCE($1, name), \
            color_bg    = COALESCE($2, color_bg), \
            color_text  = COALESCE($3, color_text), \
            color_border = COALESCE($4, color_border), \
            icon        = CASE WHEN $5::bool THEN $6 ELSE icon END \
         WHERE id = $7 AND user_id = $8 \
         RETURNING {SELECT_COLUMNS}"
    ))
    .bind(body.name.as_deref())
    .bind(body.color_bg.as_deref())
    .bind(body.color_text.as_deref())
    .bind(body.color_border.as_deref())
    .bind(icon_present)
    .bind(icon_value)
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(LabelResponse::from(updated)))
}

async fn delete_label(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let label = fetch_owned(&state, user.id, id).await?;

    if label.is_default {
        return Err(AppError::Conflict(
            "default labels cannot be deleted".into(),
        ));
    }

    let affected = sqlx::query("DELETE FROM labels WHERE id = $1 AND user_id = $2")
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

async fn fetch_owned(state: &AppState, user_id: Uuid, id: Uuid) -> Result<Label, AppError> {
    sqlx::query_as::<_, Label>(&format!(
        "SELECT {SELECT_COLUMNS} FROM labels \
         WHERE id = $1 AND user_id = $2"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::error::AppError;

    #[test]
    fn conflict_is_409() {
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        let resp = AppError::Conflict("test".into()).into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn valid_hex_colors_accepted() {
        assert!(validate_hex_color("f", "#abc").is_ok());
        assert!(validate_hex_color("f", "#abcd").is_ok());
        assert!(validate_hex_color("f", "#aabbcc").is_ok());
        assert!(validate_hex_color("f", "#aabbccdd").is_ok());
        assert!(validate_hex_color("f", "#1e1836").is_ok());
        assert!(validate_hex_color("f", "#FFF").is_ok());
    }

    #[test]
    fn invalid_hex_colors_rejected() {
        assert!(validate_hex_color("f", "abc").is_err()); // no hash
        assert!(validate_hex_color("f", "#ab").is_err()); // too short
        assert!(validate_hex_color("f", "#aabbccddee").is_err()); // too long
        assert!(validate_hex_color("f", "#zzzzzz").is_err()); // invalid chars
        assert!(validate_hex_color("f", "").is_err()); // empty
        assert!(validate_hex_color("f", "#gg1234").is_err()); // g is not hex
    }
}
