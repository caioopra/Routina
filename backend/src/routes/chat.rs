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
use std::time::Instant;

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
use tracing::{Level, Span, field};
use uuid::Uuid;

use crate::ai::pricing::estimate_cost_usd;
use crate::ai::prompts::{RoutineContext, UserContext, planner_system_prompt};
use crate::ai::provider::{
    FinishReason, Message as LlmMessage, ProviderEvent, TokenUsage, ToolCall,
};
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
    // ── Span: chat.turn — covers the entire request ───────────────────────
    // `provider` is recorded once we know it; `conversation_id` is filled in
    // after conversation resolution.  Both start as Empty so the span is still
    // opened before we have the values.
    let turn_span = tracing::span!(
        Level::INFO,
        "chat.turn",
        user_id = %user.id,
        conversation_id = field::Empty,
        routine_id = field::Empty,
        provider = field::Empty,
    );
    let _turn_guard = turn_span.enter();

    // ── Kill-switch: check chat_enabled setting ───────────────────────────
    let chat_enabled = state
        .settings_cache
        .get(&state.pool, "chat_enabled")
        .await
        .unwrap_or_else(|| "true".to_string());
    if chat_enabled == "false" {
        return Err(AppError::ServiceUnavailable("chat_disabled".into()));
    }

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

    // Record resolved IDs and provider into the turn span now that we have them.
    Span::current().record("conversation_id", conv_id.to_string().as_str());
    Span::current().record("routine_id", routine_id.to_string().as_str());

    // ── Budget check ───────────────────────────────────────────────────────
    // Query the current month's spend for this user from the daily rollup.
    let monthly_spend: f64 = sqlx::query_scalar::<_, f64>(
        "SELECT COALESCE(SUM(estimated_cost_usd::float8), 0) \
         FROM llm_usage_daily \
         WHERE user_id = $1 \
           AND day >= date_trunc('month', CURRENT_DATE)::date",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0.0);

    let budget_monthly_usd: f64 = state
        .settings_cache
        .get(&state.pool, "budget_monthly_usd")
        .await
        .and_then(|v| v.parse().ok())
        .unwrap_or(5.0);

    let budget_warn_pct: f64 = state
        .settings_cache
        .get(&state.pool, "budget_warn_pct")
        .await
        .and_then(|v| v.parse().ok())
        .unwrap_or(80.0);

    if monthly_spend >= budget_monthly_usd {
        return Err(AppError::BudgetExceeded {
            monthly_spend,
            budget: budget_monthly_usd,
        });
    }

    let budget_warning = monthly_spend >= budget_monthly_usd * budget_warn_pct / 100.0;

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
    // Record provider name into the enclosing turn span.
    Span::current().record("provider", provider_name.as_str());

    // Derive model name from config based on provider.
    let model_name = match provider_name.as_str() {
        "gemini" => state.config.llm_gemini_model.clone(),
        "claude" => state.config.llm_claude_model.clone(),
        _ => provider_name.clone(),
    };

    let tools = all_tool_schemas();
    let pool = state.pool.clone();

    let user_id = user.id;

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
        // Accumulated token usage across all rounds.
        let mut total_usage: Option<TokenUsage> = None;

        'rounds: for round in 0..MAX_TOOL_ROUNDS {
            // ── Span: chat.round — covers one provider call ───────────────
            // finish_reason is recorded at round end; use Empty initially.
            let round_span = tracing::span!(
                Level::DEBUG,
                "chat.round",
                round = round,
                finish_reason = field::Empty,
            );
            let _round_guard = round_span.enter();

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
            // Per-round usage (reset each iteration).
            let mut round_usage: Option<TokenUsage> = None;

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
                    ProviderEvent::Done { finish_reason: fr, usage } => {
                        finish_reason = fr;
                        round_usage = usage;
                        // Accumulate token usage from this round into the total.
                        if let Some(u) = round_usage {
                            let acc = total_usage.get_or_insert(TokenUsage::default());
                            acc.add(u);
                        }
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

            // Record finish_reason into the round span.
            Span::current().record("finish_reason", format!("{finish_reason:?}").as_str());

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
                 (id, conversation_id, role, content, tool_calls, provider, input_tokens, output_tokens, model) \
                 VALUES ($1, $2, 'assistant', $3, $4, $5, $6, $7, $8)",
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
            .bind(round_usage.map(|u| u.input_tokens as i32))
            .bind(round_usage.map(|u| u.output_tokens as i32))
            .bind(model_name.as_str())
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

                // ── Span: chat.tool_call — wraps execute_tool ─────────────
                let tool_start = Instant::now();
                let tool_span = tracing::span!(
                    Level::INFO,
                    "chat.tool_call",
                    tool_name = %tc.name,
                    tool_call_id = %tc.id,
                    duration_ms = field::Empty,
                );
                let _tool_guard = tool_span.enter();

                let result = execute_tool(&ctx, tc).await;

                let duration_ms = tool_start.elapsed().as_millis();
                Span::current().record("duration_ms", duration_ms);

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
                    tracing::error!(
                        tool_call_id = %tc.id,
                        error = ?e,
                        "Failed to persist tool message",
                    );
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

        // ── Usage rollup upsert ────────────────────────────────────────────
        // After all rounds complete, upsert the accumulated usage into the
        // daily rollup table so budget checks reflect real-time spend.
        if let Some(u) = total_usage {
            let cost = estimate_cost_usd(
                &provider_name,
                &model_name,
                u.input_tokens,
                u.output_tokens,
            );
            let rollup_result = sqlx::query(
                "INSERT INTO llm_usage_daily \
                 (day, user_id, provider, model, input_tokens, output_tokens, request_count, estimated_cost_usd) \
                 VALUES (CURRENT_DATE, $1, $2, $3, $4, $5, 1, $6) \
                 ON CONFLICT (day, user_id, provider, model) DO UPDATE SET \
                     input_tokens       = llm_usage_daily.input_tokens  + EXCLUDED.input_tokens, \
                     output_tokens      = llm_usage_daily.output_tokens + EXCLUDED.output_tokens, \
                     request_count      = llm_usage_daily.request_count + 1, \
                     estimated_cost_usd = llm_usage_daily.estimated_cost_usd + EXCLUDED.estimated_cost_usd",
            )
            .bind(user_id)
            .bind(provider_name.as_str())
            .bind(model_name.as_str())
            .bind(u.input_tokens as i64)
            .bind(u.output_tokens as i64)
            .bind(cost)
            .execute(&pool)
            .await;

            if let Err(e) = rollup_result {
                tracing::error!(error = ?e, "Failed to upsert llm_usage_daily");
            }
        }

        if had_error {
            // error_payload is already a JSON string (provider_error or tool_loop_limit_reached).
            yield Ok(sse_line("error", &error_payload));
        } else {
            let done_payload = match total_usage {
                Some(u) => json!({
                    "conversation_id": conv_id,
                    "message_id": last_asst_msg_id,
                    "usage": {
                        "input_tokens": u.input_tokens,
                        "output_tokens": u.output_tokens,
                    },
                    "budget_warning": budget_warning,
                }),
                None => json!({
                    "conversation_id": conv_id,
                    "message_id": last_asst_msg_id,
                    "budget_warning": budget_warning,
                }),
            };
            yield Ok(sse_line("done", &done_payload.to_string()));
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
