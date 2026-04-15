//! SSE chat endpoint — `POST /api/chat/message`.
//!
//! ## Error persistence policy
//! If the LLM stream errors mid-flight, whatever text was already accumulated
//! before the error is persisted as the assistant message.  This means a partial
//! reply may end up in the DB — the frontend can detect an incomplete message by
//! watching for the `error` SSE event before `done`.  Persisting partial content
//! is preferable to losing context entirely.

use std::convert::Infallible;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::ai::prompts::{RoutineContext, UserContext, planner_system_prompt};
use crate::ai::provider::{Message as LlmMessage, ProviderEvent};
use crate::middleware::CurrentUser;
use crate::middleware::error::AppError;
use crate::models::conversation::Conversation;

use super::AppState;

/// Maximum number of past messages included in the LLM context window.
const MAX_HISTORY: i64 = 40;

pub fn router() -> Router<AppState> {
    Router::new().route("/message", post(send_message))
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub conversation_id: Option<Uuid>,
    pub message: String,
    pub routine_id: Option<Uuid>,
}

fn sse_line(event: &str, data: &str) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

async fn send_message(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<SendMessageRequest>,
) -> Result<impl IntoResponse, AppError> {
    // ── Validate and resolve the LLM provider ──────────────────────────────
    let provider = state.llm_provider.clone().ok_or_else(|| {
        AppError::Internal("LLM provider not configured (GEMINI_API_KEY missing)".into())
    })?;

    // ── Resolve or create the conversation ────────────────────────────────
    let conversation = match body.conversation_id {
        Some(conv_id) => {
            // Verify ownership.
            sqlx::query_as::<_, Conversation>(
                "SELECT id, user_id, routine_id, title, created_at, updated_at \
                 FROM conversations WHERE id = $1 AND user_id = $2",
            )
            .bind(conv_id)
            .bind(user.id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(AppError::NotFound)?
        }
        None => {
            // routine_id is required when starting a new conversation.
            let routine_id = body.routine_id.ok_or_else(|| {
                AppError::BadRequest(
                    "routine_id is required when starting a new conversation".into(),
                )
            })?;

            // Verify the routine belongs to the caller.
            super::verify_routine_owned(&state.pool, user.id, routine_id).await?;

            let id = Uuid::now_v7();
            sqlx::query_as::<_, Conversation>(
                "INSERT INTO conversations (id, user_id, routine_id) \
                 VALUES ($1, $2, $3) \
                 RETURNING id, user_id, routine_id, title, created_at, updated_at",
            )
            .bind(id)
            .bind(user.id)
            .bind(routine_id)
            .fetch_one(&state.pool)
            .await?
        }
    };

    let conv_id = conversation.id;

    // Routine id is guaranteed non-null by creation logic, but guard anyway.
    let routine_id = conversation
        .routine_id
        .ok_or_else(|| AppError::Internal(format!("conversation {conv_id} has no routine_id")))?;

    // ── Persist the user message ───────────────────────────────────────────
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content) VALUES ($1, $2, 'user', $3)",
    )
    .bind(Uuid::now_v7())
    .bind(conv_id)
    .bind(&body.message)
    .execute(&state.pool)
    .await?;

    // Bump conversation updated_at.
    sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
        .bind(conv_id)
        .execute(&state.pool)
        .await?;

    // ── Build LLM message history ──────────────────────────────────────────

    // Load user's name and planner_context from DB.
    let user_row = sqlx::query!(
        "SELECT name, planner_context FROM users WHERE id = $1",
        user.id,
    )
    .fetch_one(&state.pool)
    .await?;

    // Load routine metadata for the system prompt.
    let routine_row = sqlx::query!(
        "SELECT name, period FROM routines WHERE id = $1",
        routine_id,
    )
    .fetch_one(&state.pool)
    .await?;

    let user_ctx = UserContext {
        name: user_row.name,
        planner_context: user_row.planner_context,
    };
    let routine_ctx = RoutineContext {
        id: routine_id,
        name: routine_row.name,
        period: routine_row.period,
    };
    let system_prompt = planner_system_prompt(&user_ctx, &routine_ctx);

    // Load last MAX_HISTORY messages in chronological order (the newly inserted
    // user message is included via the subquery trick).
    let history_rows = sqlx::query!(
        "SELECT role, content, tool_calls, tool_call_id \
         FROM ( \
             SELECT role, content, tool_calls, tool_call_id, created_at \
             FROM messages \
             WHERE conversation_id = $1 \
             ORDER BY created_at DESC \
             LIMIT $2 \
         ) sub \
         ORDER BY sub.created_at ASC",
        conv_id,
        MAX_HISTORY,
    )
    .fetch_all(&state.pool)
    .await?;

    let mut llm_messages: Vec<LlmMessage> = Vec::with_capacity(history_rows.len() + 1);
    llm_messages.push(LlmMessage::system(system_prompt));

    for row in history_rows {
        let content = row.content.unwrap_or_default();
        let msg = match row.role.as_str() {
            "user" => LlmMessage::user(content),
            "assistant" => LlmMessage::assistant(content),
            "tool" => {
                let id = row.tool_call_id.unwrap_or_default();
                LlmMessage::tool_result(id, content)
            }
            _ => LlmMessage::user(content),
        };
        llm_messages.push(msg);
    }

    // ── Start streaming ────────────────────────────────────────────────────
    let provider_name = provider.name().to_owned();
    let stream_result = provider.stream_completion(&llm_messages, &[]).await;

    let pool = state.pool.clone();

    let stream_body = async_stream::stream! {
        // Announce provider.
        yield Ok::<String, Infallible>(sse_line(
            "provider",
            &json!({ "name": provider_name }).to_string(),
        ));

        let mut llm_stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                yield Ok(sse_line(
                    "error",
                    &json!({ "message": e.to_string() }).to_string(),
                ));
                return;
            }
        };

        let mut accumulated = String::new();
        let mut had_error = false;
        let mut error_msg = String::new();

        while let Some(event) = llm_stream.next().await {
            match event {
                ProviderEvent::Token(text) => {
                    let line = sse_line("token", &json!({ "text": text }).to_string());
                    accumulated.push_str(&text);
                    yield Ok(line);
                }
                ProviderEvent::Done { .. } => {
                    // Handled after the loop.
                    break;
                }
                ProviderEvent::Error(msg) => {
                    had_error = true;
                    error_msg = msg;
                    break;
                }
                // Tool calls deferred to Slice C — ignored here.
                ProviderEvent::ToolCall(_) => {}
            }
        }

        // Persist the assistant message (partial content on error, full on success).
        let asst_msg_id = Uuid::now_v7();
        let persist_result = sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, provider) \
             VALUES ($1, $2, 'assistant', $3, 'gemini')",
        )
        .bind(asst_msg_id)
        .bind(conv_id)
        .bind(if accumulated.is_empty() { None } else { Some(accumulated.clone()) })
        .execute(&pool)
        .await;

        if let Err(e) = persist_result {
            tracing::error!("Failed to persist assistant message: {e}");
        }

        // Bump conversation updated_at.
        let _ = sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
            .bind(conv_id)
            .execute(&pool)
            .await;

        if had_error {
            yield Ok(sse_line(
                "error",
                &json!({ "message": error_msg }).to_string(),
            ));
        } else {
            yield Ok(sse_line(
                "done",
                &json!({
                    "conversation_id": conv_id,
                    "message_id": asst_msg_id
                })
                .to_string(),
            ));
        }
    };

    // Build the SSE response with required headers.
    let body = axum::body::Body::from_stream(stream_body);
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "text/event-stream".parse().unwrap());
    headers.insert("Cache-Control", "no-cache".parse().unwrap());
    headers.insert("X-Accel-Buffering", "no".parse().unwrap());

    Ok((StatusCode::OK, headers, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_line_format() {
        let line = sse_line("token", r#"{"text":"hi"}"#);
        assert_eq!(line, "event: token\ndata: {\"text\":\"hi\"}\n\n");
    }

    #[test]
    fn sse_line_done() {
        let line = sse_line("done", "{}");
        assert!(line.starts_with("event: done\n"));
        assert!(line.contains("data: {}"));
        assert!(line.ends_with("\n\n"));
    }

    #[test]
    fn sse_line_provider() {
        let line = sse_line("provider", r#"{"name":"gemini"}"#);
        assert!(line.contains("event: provider"));
        assert!(line.contains(r#"{"name":"gemini"}"#));
    }
}
