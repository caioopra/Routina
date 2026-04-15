use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::middleware::CurrentUser;
use crate::middleware::error::AppError;
use crate::models::conversation::{
    Conversation, ConversationResponse, CreateConversationRequest, Message,
};

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_conversations).post(create_conversation))
        .route("/{id}/messages", get(get_messages))
}

#[derive(Debug, Deserialize)]
pub struct ConversationQuery {
    pub routine_id: Option<Uuid>,
}

async fn list_conversations(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<ConversationQuery>,
) -> Result<Json<Vec<ConversationResponse>>, AppError> {
    let rows = if let Some(routine_id) = params.routine_id {
        sqlx::query_as::<_, Conversation>(
            "SELECT id, user_id, routine_id, title, created_at, updated_at \
             FROM conversations \
             WHERE user_id = $1 AND routine_id = $2 \
             ORDER BY created_at DESC",
        )
        .bind(user.id)
        .bind(routine_id)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Conversation>(
            "SELECT id, user_id, routine_id, title, created_at, updated_at \
             FROM conversations \
             WHERE user_id = $1 \
             ORDER BY created_at DESC",
        )
        .bind(user.id)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(Json(
        rows.into_iter().map(ConversationResponse::from).collect(),
    ))
}

async fn create_conversation(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<CreateConversationRequest>,
) -> Result<(StatusCode, Json<ConversationResponse>), AppError> {
    // Verify that the routine exists and belongs to the caller.
    super::verify_routine_owned(&state.pool, user.id, body.routine_id).await?;

    let id = Uuid::now_v7();
    let conv = sqlx::query_as::<_, Conversation>(
        "INSERT INTO conversations (id, user_id, routine_id, title) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, user_id, routine_id, title, created_at, updated_at",
    )
    .bind(id)
    .bind(user.id)
    .bind(body.routine_id)
    .bind(body.title.as_deref())
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(ConversationResponse::from(conv))))
}

async fn get_messages(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Message>>, AppError> {
    // Verify ownership of the conversation.
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM conversations WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user.id)
            .fetch_optional(&state.pool)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let messages = sqlx::query_as::<_, Message>(
        "SELECT id, conversation_id, role, content, tool_calls, tool_call_id, provider, created_at \
         FROM messages \
         WHERE conversation_id = $1 \
         ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn conversation_query_defaults_to_none() {
        // Verify that ConversationQuery allows routine_id to be absent.
        let q = ConversationQuery { routine_id: None };
        assert!(q.routine_id.is_none());
    }

    #[test]
    fn conversation_query_stores_routine_id() {
        let id = Uuid::now_v7();
        let q = ConversationQuery {
            routine_id: Some(id),
        };
        assert_eq!(q.routine_id, Some(id));
    }
}
