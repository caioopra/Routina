//! SSE chat endpoint — `POST /api/chat/message`.
//!
//! ## Tool-use loop
//! The handler runs up to `MAX_TOOL_ROUNDS` iterations.  Each iteration calls
//! `provider.stream_completion`, accumulates the result, and if the finish
//! reason is `ToolCalls` it executes all requested tools and feeds the results
//! back as additional messages before the next iteration.
//!
//! ## Error persistence policy
//! If the LLM stream errors mid-flight, whatever text was already accumulated
//! before the error is persisted as the assistant message.  A partial reply may
//! end up in the DB — the frontend can detect an incomplete message by watching
//! for the `error` SSE event before `done`.  Persisting partial content is
//! preferable to losing context entirely.

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
use crate::ai::provider::{FinishReason, Message as LlmMessage, ProviderEvent, ToolCall};
use crate::ai::tools::executor::{ToolContext, execute_tool};
use crate::ai::tools::schemas::all_tool_schemas;
use crate::middleware::CurrentUser;
use crate::middleware::error::AppError;
use crate::models::conversation::Conversation;

use super::AppState;

/// Maximum number of past messages included in the LLM context window.
const MAX_HISTORY: i64 = 40;

/// Maximum number of tool-call/execute rounds before we stop and report an error.
const MAX_TOOL_ROUNDS: usize = 8;

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
    // ── Resolve the LLM provider for this user ────────────────────────────
    // Read from users.preferences.llm_provider; fall back to first available.
    let pref_row = sqlx::query!("SELECT preferences FROM users WHERE id = $1", user.id,)
        .fetch_optional(&state.pool)
        .await?;

    let preferred_provider = pref_row
        .as_ref()
        .and_then(|r| r.preferences.get("llm_provider"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    let provider = state
        .resolve_provider(preferred_provider.as_deref())
        .ok_or_else(|| {
            AppError::Internal(
                "No LLM provider configured (set LLM_GEMINI_API_KEY or LLM_CLAUDE_API_KEY)".into(),
            )
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
            "assistant" => {
                let mut msg = LlmMessage::assistant(content);
                // Re-attach tool_calls so the provider can see them in history.
                if let Some(tc_json) = row.tool_calls
                    && let Ok(tcs) = serde_json::from_value::<Vec<ToolCall>>(tc_json)
                {
                    msg.tool_calls = Some(tcs);
                }
                msg
            }
            "tool" => {
                let id = row.tool_call_id.unwrap_or_default();
                LlmMessage::tool_result(id, content)
            }
            _ => LlmMessage::user(content),
        };
        llm_messages.push(msg);
    }

    // ── Start the tool-use loop ────────────────────────────────────────────
    let provider_name = provider.name().to_owned();
    let tools = all_tool_schemas();
    let pool = state.pool.clone();

    let stream_body = async_stream::stream! {
        // Announce provider.
        yield Ok::<String, Infallible>(sse_line(
            "provider",
            &json!({ "name": provider_name }).to_string(),
        ));

        let mut last_asst_msg_id = Uuid::now_v7();
        let mut had_error = false;
        // The client-visible error message — must never contain raw provider or DB details.
        let mut error_payload = String::new();

        'rounds: for round in 0..MAX_TOOL_ROUNDS {
            // Start a new stream for this round.
            let stream_result = provider.stream_completion(&llm_messages, &tools).await;
            let mut llm_stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(provider = %provider_name, error = ?e, "LLM provider stream_completion error");
                    had_error = true;
                    error_payload = json!({ "message": "provider_error", "provider": provider_name }).to_string();
                    break 'rounds;
                }
            };

            let mut accumulated_text = String::new();
            let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();
            let mut finish_reason = FinishReason::Stop;

            // Drain the stream for this round.
            while let Some(event) = llm_stream.next().await {
                match event {
                    ProviderEvent::Token(text) => {
                        let line = sse_line("token", &json!({ "text": text }).to_string());
                        accumulated_text.push_str(&text);
                        yield Ok(line);
                    }
                    ProviderEvent::ToolCall(tc) => {
                        // Accumulate; emit after Done so we know the full set.
                        accumulated_tool_calls.push(tc);
                    }
                    ProviderEvent::Done { finish_reason: fr } => {
                        finish_reason = fr;
                        break;
                    }
                    ProviderEvent::Error(msg) => {
                        tracing::error!(provider = %provider_name, error = %msg, "LLM provider stream error event");
                        had_error = true;
                        error_payload = json!({ "message": "provider_error", "provider": provider_name }).to_string();
                        break 'rounds;
                    }
                }
            }

            // Persist the assistant message for this round.
            let asst_msg_id = Uuid::now_v7();
            last_asst_msg_id = asst_msg_id;

            let tool_calls_json: Option<serde_json::Value> =
                if accumulated_tool_calls.is_empty() {
                    None
                } else {
                    serde_json::to_value(&accumulated_tool_calls).ok()
                };

            let persist_result = sqlx::query(
                "INSERT INTO messages \
                 (id, conversation_id, role, content, tool_calls, provider) \
                 VALUES ($1, $2, 'assistant', $3, $4, $5)",
            )
            .bind(asst_msg_id)
            .bind(conv_id)
            .bind(if accumulated_text.is_empty() {
                None
            } else {
                Some(accumulated_text.clone())
            })
            .bind(&tool_calls_json)
            .bind(provider_name.as_str())
            .execute(&pool)
            .await;

            if let Err(e) = persist_result {
                tracing::error!("Failed to persist assistant message (round {round}): {e}");
            }

            // Add the assistant turn to the running message history.
            let mut asst_msg = LlmMessage::assistant(accumulated_text.clone());
            if !accumulated_tool_calls.is_empty() {
                asst_msg.tool_calls = Some(accumulated_tool_calls.clone());
            }
            llm_messages.push(asst_msg);

            // If the model didn't ask for tools, we're done.
            if finish_reason != FinishReason::ToolCalls || accumulated_tool_calls.is_empty() {
                break 'rounds;
            }

            // ── Execute each tool call and feed results back ──────────────
            let ctx = ToolContext {
                pool: &pool,
                user_id: user.id,
                routine_id,
                conversation_id: conv_id,
            };

            for tc in &accumulated_tool_calls {
                // Emit tool_call event so the frontend can show progress.
                yield Ok(sse_line(
                    "tool_call",
                    &json!({
                        "id":   tc.id,
                        "name": tc.name,
                        "args": tc.args
                    })
                    .to_string(),
                ));

                // Execute.
                let result = execute_tool(&ctx, tc).await;

                // Persist tool-result message.
                let tool_msg_id = Uuid::now_v7();
                let content_str = result.data.to_string();
                let persist_tool = sqlx::query(
                    "INSERT INTO messages \
                     (id, conversation_id, role, content, tool_call_id, provider) \
                     VALUES ($1, $2, 'tool', $3, $4, $5)",
                )
                .bind(tool_msg_id)
                .bind(conv_id)
                .bind(&content_str)
                .bind(&tc.id)
                .bind(provider_name.as_str())
                .execute(&pool)
                .await;

                if let Err(e) = persist_tool {
                    tracing::error!("Failed to persist tool message for call {}: {e}", tc.id);
                }

                // Emit tool_result event.
                yield Ok(sse_line(
                    "tool_result",
                    &json!({
                        "id":      tc.id,
                        "success": result.success,
                        "data":    result.data
                    })
                    .to_string(),
                ));

                // If the tool mutated the routine, tell the frontend to refresh.
                if result.mutated_routine {
                    yield Ok(sse_line(
                        "routine_updated",
                        &json!({ "routine_id": routine_id }).to_string(),
                    ));
                }

                // Append the tool result to the message history for the next round.
                llm_messages.push(LlmMessage::tool_result(tc.id.clone(), content_str));
            }

            // If we exhausted all rounds, emit a limit error.
            if round + 1 == MAX_TOOL_ROUNDS {
                had_error = true;
                error_payload = json!({ "message": "tool_loop_limit_reached" }).to_string();
                break 'rounds;
            }
        }

        // Bump conversation updated_at.
        let _ = sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
            .bind(conv_id)
            .execute(&pool)
            .await;

        if had_error {
            // error_payload is already a JSON string (provider_error or tool_loop_limit_reached).
            yield Ok(sse_line("error", &error_payload));
        } else {
            yield Ok(sse_line(
                "done",
                &json!({
                    "conversation_id": conv_id,
                    "message_id": last_asst_msg_id
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

    #[test]
    fn sse_line_tool_call() {
        let line = sse_line("tool_call", r#"{"id":"c1","name":"list_blocks","args":{}}"#);
        assert!(line.starts_with("event: tool_call\n"));
        assert!(line.contains("\"name\":\"list_blocks\""));
    }

    #[test]
    fn sse_line_tool_result() {
        let line = sse_line("tool_result", r#"{"id":"c1","success":true,"data":[]}"#);
        assert!(line.starts_with("event: tool_result\n"));
        assert!(line.contains("\"success\":true"));
    }

    #[test]
    fn sse_line_routine_updated() {
        let id = Uuid::nil();
        let line = sse_line("routine_updated", &json!({ "routine_id": id }).to_string());
        assert!(line.starts_with("event: routine_updated\n"));
        assert!(line.contains("routine_id"));
    }
}
