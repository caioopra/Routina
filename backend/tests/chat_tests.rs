mod common;

use std::collections::HashMap;
use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{
    MockLlmProvider, ScriptedMockProvider, build_app, build_app_with_mock,
    build_app_with_providers, build_app_with_rate_limit, json_oneshot, raw_oneshot,
    register_test_user,
};
use planner_backend::ai::provider::{FinishReason, ProviderEvent, ToolCall};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn register_and_token(app: &axum::Router, email: &str) -> String {
    let body = register_test_user(app, email, "longenoughpass").await;
    body["token"].as_str().unwrap().to_string()
}

async fn create_routine(app: &axum::Router, token: &str) -> serde_json::Value {
    let (status, body) = json_oneshot(
        app,
        Method::POST,
        "/api/routines",
        Some(json!({ "name": "Test Routine" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create_routine failed: {body}");
    body
}

/// Parse raw SSE bytes into a list of (event_name, data_string) pairs.
fn parse_sse(bytes: &[u8]) -> Vec<(String, String)> {
    let text = std::str::from_utf8(bytes).expect("SSE must be UTF-8");
    let mut events = Vec::new();
    let mut current_event = String::new();
    let mut current_data = String::new();

    for line in text.lines() {
        if let Some(ev) = line.strip_prefix("event: ") {
            current_event = ev.to_string();
        } else if let Some(data) = line.strip_prefix("data: ") {
            current_data = data.to_string();
        } else if line.is_empty() && !current_event.is_empty() {
            events.push((current_event.clone(), current_data.clone()));
            current_event.clear();
            current_data.clear();
        }
    }
    events
}

fn make_mock(tokens: Vec<&str>) -> Arc<dyn planner_backend::ai::provider::LlmProvider> {
    MockLlmProvider::new(tokens).into_shared()
}

fn make_tool_call(name: &str, args: serde_json::Value) -> ToolCall {
    ToolCall {
        id: format!("call-{}", Uuid::now_v7()),
        name: name.to_string(),
        args,
    }
}

// ── Auth enforcement ──────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_requires_auth(pool: PgPool) {
    let app = build_app_with_mock(pool, make_mock(vec!["hello"]));

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({ "message": "hi", "routine_id": Uuid::now_v7() })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ── 503 when no provider configured ──────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_returns_503_without_provider(pool: PgPool) {
    // build_app has no LLM provider.
    let app = build_app(pool);
    let token = register_and_token(&app, "chat-noprovider@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Hello",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

// ── Happy path: SSE stream ────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_message_streams_sse_events(pool: PgPool) {
    let mock = make_mock(vec!["Olá! ", "Como posso ", "ajudar?"]);
    let app = build_app_with_mock(pool, mock);
    let token = register_and_token(&app, "chat-stream@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Crie um bloco.",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected 200 for SSE");

    let events = parse_sse(&bytes);

    // Must start with provider event.
    let (first_ev, first_data) = &events[0];
    assert_eq!(first_ev, "provider");
    let provider_json: serde_json::Value = serde_json::from_str(first_data).unwrap();
    assert_eq!(provider_json["name"].as_str().unwrap(), "mock");

    // Must contain three token events.
    let token_events: Vec<_> = events.iter().filter(|(e, _)| e == "token").collect();
    assert_eq!(token_events.len(), 3, "expected 3 token events");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&token_events[0].1).unwrap()["text"],
        "Olá! "
    );

    // Must end with done event containing conversation_id and message_id.
    let (last_ev, last_data) = events.last().unwrap();
    assert_eq!(last_ev, "done");
    let done_json: serde_json::Value = serde_json::from_str(last_data).unwrap();
    assert!(done_json["conversation_id"].is_string());
    assert!(done_json["message_id"].is_string());
}

// ── Messages persisted in DB ──────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_message_persists_user_and_assistant_messages(pool: PgPool) {
    let mock = make_mock(vec!["Aqui está minha resposta."]);
    let app = build_app_with_mock(pool.clone(), mock);
    let token = register_and_token(&app, "chat-persist@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Mensagem de teste",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Extract conversation_id from done event.
    let events = parse_sse(&bytes);
    let (_, done_data) = events.iter().find(|(e, _)| e == "done").unwrap();
    let done_json: serde_json::Value = serde_json::from_str(done_data).unwrap();
    let conv_id_str = done_json["conversation_id"].as_str().unwrap();
    let conv_id: Uuid = conv_id_str.parse().unwrap();

    // Query DB directly via the pool.
    let messages = sqlx::query!(
        "SELECT role, content FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC",
        conv_id
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(messages.len(), 2, "expected user + assistant message in DB");
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content.as_deref(), Some("Mensagem de teste"));
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(
        messages[1].content.as_deref(),
        Some("Aqui está minha resposta.")
    );
}

// ── Conversation auto-created when not provided ───────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_creates_new_conversation_when_id_absent(pool: PgPool) {
    let mock = make_mock(vec!["ok"]);
    let app = build_app_with_mock(pool.clone(), mock);
    let token = register_and_token(&app, "chat-new-conv@example.com").await;
    let routine = create_routine(&app, &token).await;
    let routine_id = routine["id"].as_str().unwrap();

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Nova conversa",
            "routine_id": routine_id
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse(&bytes);
    let (_, done_data) = events.iter().find(|(e, _)| e == "done").unwrap();
    let done_json: serde_json::Value = serde_json::from_str(done_data).unwrap();
    let conv_id_str = done_json["conversation_id"].as_str().unwrap();
    let conv_id: Uuid = conv_id_str.parse().unwrap();

    // Verify conversation row exists with correct routine_id.
    let conv = sqlx::query!(
        "SELECT routine_id FROM conversations WHERE id = $1",
        conv_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        conv.routine_id.unwrap().to_string(),
        routine_id,
        "conversation not bound to the correct routine"
    );
}

// ── Existing conversation_id reused ──────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_reuses_existing_conversation(pool: PgPool) {
    let mock = make_mock(vec!["resposta1"]);
    let app = build_app_with_mock(pool.clone(), mock);
    let token = register_and_token(&app, "chat-reuse@example.com").await;
    let routine = create_routine(&app, &token).await;
    let routine_id = routine["id"].as_str().unwrap();

    // Create a conversation explicitly.
    let (_, conv) = json_oneshot(
        &app,
        Method::POST,
        "/api/conversations",
        Some(json!({ "routine_id": routine_id })),
        Some(&token),
    )
    .await;
    let conv_id = conv["id"].as_str().unwrap();

    // Send message referencing it.
    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "conversation_id": conv_id,
            "message": "Olá"
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse(&bytes);
    let (_, done_data) = events.iter().find(|(e, _)| e == "done").unwrap();
    let done_json: serde_json::Value = serde_json::from_str(done_data).unwrap();
    assert_eq!(
        done_json["conversation_id"].as_str().unwrap(),
        conv_id,
        "conversation_id in done event must match the one provided"
    );

    // Confirm messages exist in that conversation.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE conversation_id = $1")
        .bind(conv_id.parse::<Uuid>().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

// ── Missing routine_id when starting fresh ────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_bad_request_when_no_conv_and_no_routine(pool: PgPool) {
    let mock = make_mock(vec!["ok"]);
    let app = build_app_with_mock(pool, mock);
    let token = register_and_token(&app, "chat-noroute@example.com").await;

    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({ "message": "hi" })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ── Ownership: other user's conversation returns 404 ─────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_cannot_use_other_users_conversation(pool: PgPool) {
    let mock = make_mock(vec!["ok"]);
    let app = build_app_with_mock(pool.clone(), mock);
    let token_a = register_and_token(&app, "chat-own-a@example.com").await;
    let token_b = register_and_token(&app, "chat-own-b@example.com").await;

    let routine_a = create_routine(&app, &token_a).await;
    let (_, conv) = json_oneshot(
        &app,
        Method::POST,
        "/api/conversations",
        Some(json!({ "routine_id": routine_a["id"].as_str().unwrap() })),
        Some(&token_a),
    )
    .await;
    let conv_id = conv["id"].as_str().unwrap();

    // User B tries to send a message to user A's conversation.
    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "conversation_id": conv_id,
            "message": "sneaky"
        })),
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── planner_context injected into system prompt ───────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_injects_planner_context_into_system_prompt(pool: PgPool) {
    // Keep an Arc<MockLlmProvider> so we can inspect it after the request.
    let mock_provider = MockLlmProvider::new(vec!["ok"]).into_shared();
    let mock_arc: Arc<dyn planner_backend::ai::provider::LlmProvider> = mock_provider.clone();

    let app = build_app_with_mock(pool.clone(), mock_arc);
    let token = register_and_token(&app, "chat-ctx-inject@example.com").await;
    let routine = create_routine(&app, &token).await;

    // Set planner_context for this user.
    let ctx = "Trabalho como engenheiro de ML, foco em treinos de corrida.";
    json_oneshot(
        &app,
        Method::PUT,
        "/api/me/planner-context",
        Some(json!({ "planner_context": ctx })),
        Some(&token),
    )
    .await;

    // Send chat message.
    let (status, _) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Olá",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Inspect what the mock received via our kept Arc.
    let calls = mock_provider.captured_messages.lock().unwrap();
    assert!(!calls.is_empty(), "mock was never called");
    let first_call = &calls[0];
    // First message must be the system prompt.
    assert_eq!(
        first_call[0].role,
        planner_backend::ai::provider::Role::System
    );
    let system_content = &first_call[0].content;
    assert!(
        system_content.contains(ctx),
        "planner_context not found in system prompt.\nSystem prompt:\n{system_content}"
    );
}

// ── Tool-use loop: single tool call then text response ───────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_tool_use_loop_two_rounds(pool: PgPool) {
    // Round 1: LLM emits a ToolCall + Done(ToolCalls)
    // Round 2: LLM emits tokens + Done(Stop)
    let tc = make_tool_call("list_blocks", json!({}));
    let tc_clone = tc.clone();

    let round1 = vec![
        ProviderEvent::ToolCall(tc_clone),
        ProviderEvent::Done {
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
    ];
    let round2 = vec![
        ProviderEvent::Token("Aqui estão".to_string()),
        ProviderEvent::Token(" os blocos.".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let scripted = ScriptedMockProvider::new(vec![round1, round2]).into_shared();
    let scripted_arc: Arc<dyn planner_backend::ai::provider::LlmProvider> = scripted.clone();

    let app = build_app_with_mock(pool.clone(), scripted_arc);
    let token = register_and_token(&app, "chat-tool-loop@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Liste meus blocos.",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let events = parse_sse(&bytes);

    // Verify event sequence:
    // provider → tool_call → tool_result → token × 2 → done
    let ev_names: Vec<&str> = events.iter().map(|(e, _)| e.as_str()).collect();

    assert_eq!(ev_names[0], "provider", "first event must be provider");

    let tool_call_pos = ev_names.iter().position(|&e| e == "tool_call");
    assert!(tool_call_pos.is_some(), "expected tool_call event");
    let tool_result_pos = ev_names.iter().position(|&e| e == "tool_result");
    assert!(tool_result_pos.is_some(), "expected tool_result event");
    // tool_call comes before tool_result
    assert!(
        tool_call_pos.unwrap() < tool_result_pos.unwrap(),
        "tool_call must come before tool_result"
    );

    let token_events: Vec<_> = events.iter().filter(|(e, _)| e == "token").collect();
    assert_eq!(token_events.len(), 2, "expected 2 token events in round 2");

    let last = events.last().unwrap();
    assert_eq!(last.0, "done", "must end with done");

    // Verify tool_call event data.
    let (_, tc_data) = events.iter().find(|(e, _)| e == "tool_call").unwrap();
    let tc_json: serde_json::Value = serde_json::from_str(tc_data).unwrap();
    assert_eq!(tc_json["name"], "list_blocks");
    assert!(tc_json["id"].is_string());

    // Verify tool_result event data.
    let (_, tr_data) = events.iter().find(|(e, _)| e == "tool_result").unwrap();
    let tr_json: serde_json::Value = serde_json::from_str(tr_data).unwrap();
    assert!(tr_json["id"].is_string());
    assert!(tr_json["success"].is_boolean());
}

// ── Tool-use loop: DB contains correct message rows ──────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_tool_use_persists_correct_messages(pool: PgPool) {
    let tc = make_tool_call("list_blocks", json!({}));
    let tc_id = tc.id.clone();

    let round1 = vec![
        ProviderEvent::Token("".to_string()), // empty token OK
        ProviderEvent::ToolCall(tc),
        ProviderEvent::Done {
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
    ];
    let round2 = vec![
        ProviderEvent::Token("Resposta final.".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let scripted = ScriptedMockProvider::new(vec![round1, round2]).into_shared();
    let scripted_arc: Arc<dyn planner_backend::ai::provider::LlmProvider> = scripted;

    let app = build_app_with_mock(pool.clone(), scripted_arc);
    let token = register_and_token(&app, "chat-tool-persist@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Mensagem de teste",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse(&bytes);
    let (_, done_data) = events.iter().find(|(e, _)| e == "done").unwrap();
    let conv_id: Uuid =
        serde_json::from_str::<serde_json::Value>(done_data).unwrap()["conversation_id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();

    // Check DB message rows.
    let msgs = sqlx::query!(
        "SELECT role, content, tool_call_id FROM messages \
         WHERE conversation_id = $1 ORDER BY created_at ASC",
        conv_id
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    // Expected: user, assistant(with tool_calls), tool, assistant(final)
    assert!(
        msgs.len() >= 4,
        "expected at least 4 messages, got {}",
        msgs.len()
    );

    assert_eq!(msgs[0].role, "user");

    // First assistant message may have empty content but has tool_calls.
    assert_eq!(msgs[1].role, "assistant");

    // Tool message references the tool call id.
    let tool_msg = msgs.iter().find(|m| m.role == "tool");
    assert!(tool_msg.is_some(), "expected a 'tool' role message");
    let tool_msg = tool_msg.unwrap();
    assert_eq!(
        tool_msg.tool_call_id.as_deref(),
        Some(tc_id.as_str()),
        "tool message must reference the tool call id"
    );

    // Final assistant message.
    let final_asst = msgs.iter().rev().find(|m| m.role == "assistant");
    assert!(final_asst.is_some());
    assert_eq!(
        final_asst.unwrap().content.as_deref(),
        Some("Resposta final.")
    );
}

// ── Tool-use loop: routine_updated event emitted for mutations ───────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_tool_use_emits_routine_updated_on_mutation(pool: PgPool) {
    // create_block is a mutation that should trigger routine_updated.
    // We provide minimal valid args so the executor actually succeeds.
    let tc = make_tool_call(
        "create_block",
        json!({
            "day_of_week": 1,
            "start_time": "09:00",
            "title": "Test Block",
            "type": "trabalho"
        }),
    );

    let round1 = vec![
        ProviderEvent::ToolCall(tc),
        ProviderEvent::Done {
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
    ];
    let round2 = vec![
        ProviderEvent::Token("Bloco criado!".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let scripted = ScriptedMockProvider::new(vec![round1, round2]).into_shared();
    let scripted_arc: Arc<dyn planner_backend::ai::provider::LlmProvider> = scripted;

    let app = build_app_with_mock(pool.clone(), scripted_arc);
    let token = register_and_token(&app, "chat-routine-updated@example.com").await;
    let routine = create_routine(&app, &token).await;
    let routine_id = routine["id"].as_str().unwrap();

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Crie um bloco de trabalho.",
            "routine_id": routine_id
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse(&bytes);
    let ev_names: Vec<&str> = events.iter().map(|(e, _)| e.as_str()).collect();

    // Must have a routine_updated event.
    assert!(
        ev_names.contains(&"routine_updated"),
        "expected routine_updated event; got: {ev_names:?}"
    );

    // routine_updated data contains the correct routine_id.
    let (_, ru_data) = events.iter().find(|(e, _)| e == "routine_updated").unwrap();
    let ru_json: serde_json::Value = serde_json::from_str(ru_data).unwrap();
    assert_eq!(ru_json["routine_id"].as_str().unwrap(), routine_id);

    // Verify routine_actions row was inserted.
    let action_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM routine_actions WHERE routine_id = $1")
            .bind(routine_id.parse::<Uuid>().unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(action_count > 0, "expected a routine_actions audit row");
}

// ── Tool-use loop: MAX_TOOL_ROUNDS terminates with error event ───────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_tool_use_limit_reached_emits_error(pool: PgPool) {
    // Script 9 rounds all returning ToolCall + Done(ToolCalls) to exceed the limit.
    let rounds: Vec<Vec<ProviderEvent>> = (0..9)
        .map(|_| {
            let tc = make_tool_call("list_blocks", json!({}));
            vec![
                ProviderEvent::ToolCall(tc),
                ProviderEvent::Done {
                    finish_reason: FinishReason::ToolCalls,
                    usage: None,
                },
            ]
        })
        .collect();

    let scripted = ScriptedMockProvider::new(rounds).into_shared();
    let scripted_arc: Arc<dyn planner_backend::ai::provider::LlmProvider> = scripted;

    let app = build_app_with_mock(pool, scripted_arc);
    let token = register_and_token(&app, "chat-limit@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "loop forever",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "SSE endpoint always returns 200");

    let events = parse_sse(&bytes);
    let last = events.last().unwrap();
    assert_eq!(last.0, "error", "must end with error event after limit");

    let err_json: serde_json::Value = serde_json::from_str(&last.1).unwrap();
    assert_eq!(
        err_json["message"].as_str().unwrap(),
        "tool_loop_limit_reached"
    );
}

// ── Tool-use loop: scripted provider captures multi-round message history ────

#[sqlx::test(migrations = "./migrations")]
async fn chat_tool_use_message_history_grows_across_rounds(pool: PgPool) {
    let tc = make_tool_call("list_blocks", json!({}));

    let round1 = vec![
        ProviderEvent::ToolCall(tc),
        ProviderEvent::Done {
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
    ];
    let round2 = vec![
        ProviderEvent::Token("ok".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let scripted = ScriptedMockProvider::new(vec![round1, round2]).into_shared();
    let scripted_arc_for_app: Arc<dyn planner_backend::ai::provider::LlmProvider> =
        scripted.clone();

    let app = build_app_with_mock(pool.clone(), scripted_arc_for_app);
    let token = register_and_token(&app, "chat-history-grow@example.com").await;
    let routine = create_routine(&app, &token).await;

    let _ = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Lista blocos",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;

    // Round 2 should have received more messages than round 1.
    let captured = scripted.captured_messages.lock().unwrap();
    assert!(
        captured.len() >= 2,
        "expected at least 2 stream_completion calls; got {}",
        captured.len()
    );
    let round1_len = captured[0].len();
    let round2_len = captured[1].len();
    assert!(
        round2_len > round1_len,
        "round 2 message count ({round2_len}) must be greater than round 1 ({round1_len})"
    );
}

// ── Provider fallback from user preference ───────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_uses_user_preferred_provider(pool: PgPool) {
    // Set up two mocks: "alpha" returns "alpha_response", "beta" returns "beta_response".
    let alpha_mock = ScriptedMockProvider::new(vec![vec![
        ProviderEvent::Token("alpha_response".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ]])
    .with_name("alpha")
    .into_shared();
    let beta_mock = ScriptedMockProvider::new(vec![vec![
        ProviderEvent::Token("beta_response".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ]])
    .with_name("beta")
    .into_shared();

    let mut providers: HashMap<String, Arc<dyn planner_backend::ai::provider::LlmProvider>> =
        HashMap::new();
    providers.insert("alpha".to_string(), alpha_mock);
    providers.insert("beta".to_string(), beta_mock);

    let app = build_app_with_providers(pool.clone(), providers);
    let token = register_and_token(&app, "chat-pref-provider@example.com").await;
    let routine = create_routine(&app, &token).await;

    // Set user preference to "beta".
    let (status, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/settings/llm-provider",
        Some(json!({ "provider": "beta" })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "set provider failed");

    // Send a chat message — should use beta.
    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Qual provedor?",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse(&bytes);
    let (_, provider_data) = events.iter().find(|(e, _)| e == "provider").unwrap();
    let provider_json: serde_json::Value = serde_json::from_str(provider_data).unwrap();
    assert_eq!(
        provider_json["name"].as_str().unwrap(),
        "beta",
        "expected beta provider to be used"
    );
}

// ── Rate limiting ─────────────────────────────────────────────────────────────

/// 21st request from the same user is rejected with 429.
#[sqlx::test(migrations = "./migrations")]
async fn chat_rate_limit_429_on_excess(pool: PgPool) {
    // Use a very small limit so the test runs in milliseconds.
    let limit = 3usize;
    let mock = make_mock(vec!["ok"]);
    let app = build_app_with_rate_limit(pool.clone(), mock, limit);
    let token = register_and_token(&app, "rl-excess@example.com").await;
    let routine = create_routine(&app, &token).await;
    let routine_id = routine["id"].as_str().unwrap();

    // The first `limit` requests must succeed.
    for _ in 0..limit {
        let (status, _) = json_oneshot(
            &app,
            Method::POST,
            "/api/chat/message",
            Some(json!({ "message": "hi", "routine_id": routine_id })),
            Some(&token),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "expected OK for request within limit"
        );
    }

    // The (limit + 1)th request must be rate-limited.
    let (status, body) = json_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({ "message": "hi", "routine_id": routine_id })),
        Some(&token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "expected 429; got {status}: {body}"
    );
    assert_eq!(
        body["error"].as_str().unwrap(),
        "rate_limited",
        "expected error=rate_limited in body: {body}"
    );
    let retry = body["retry_after_seconds"].as_u64().unwrap();
    assert!(retry >= 1, "retry_after_seconds must be >= 1");
}

/// A second user sharing the same app instance is NOT rate-limited by the first
/// user's requests.
#[sqlx::test(migrations = "./migrations")]
async fn chat_rate_limit_separate_users_independent(pool: PgPool) {
    let limit = 2usize;
    let mock = make_mock(vec!["ok"]);
    let app = build_app_with_rate_limit(pool.clone(), mock, limit);

    let token_a = register_and_token(&app, "rl-user-a@example.com").await;
    let token_b = register_and_token(&app, "rl-user-b@example.com").await;

    let routine_a = create_routine(&app, &token_a).await;
    let routine_b = create_routine(&app, &token_b).await;

    // Exhaust user A's quota.
    for _ in 0..limit {
        let (status, _) = json_oneshot(
            &app,
            Method::POST,
            "/api/chat/message",
            Some(json!({
                "message": "hi",
                "routine_id": routine_a["id"].as_str().unwrap()
            })),
            Some(&token_a),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // User A is now rate-limited.
    let (status_a, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "hi",
            "routine_id": routine_a["id"].as_str().unwrap()
        })),
        Some(&token_a),
    )
    .await;
    assert_eq!(
        status_a,
        StatusCode::TOO_MANY_REQUESTS,
        "user A should be rate-limited"
    );

    // User B can still make requests.
    let (status_b, _) = json_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "hi",
            "routine_id": routine_b["id"].as_str().unwrap()
        })),
        Some(&token_b),
    )
    .await;
    assert_eq!(
        status_b,
        StatusCode::OK,
        "user B should not be affected by user A's limit"
    );
}

/// After the 60-second window expires, requests succeed again.
/// Uses `tokio::time::pause` + `advance` so the test runs instantly.
#[tokio::test]
async fn chat_rate_limit_window_resets_after_60s() {
    // This test does not need a DB — it tests the RateLimitState directly.
    use planner_backend::middleware::rate_limit::RateLimitState;
    use uuid::Uuid;

    tokio::time::pause();

    let state = RateLimitState::new(1);
    let uid = Uuid::now_v7();

    // First request: allowed.
    assert!(state.check_and_record(uid).is_ok());
    // Second request: rejected.
    assert!(state.check_and_record(uid).is_err());

    // Advance time past the 60-second window.
    tokio::time::advance(tokio::time::Duration::from_secs(61)).await;

    // After the window, the slot should be free again.
    assert!(
        state.check_and_record(uid).is_ok(),
        "request should succeed after window reset"
    );
}

// ── Token usage in done event ─────────────────────────────────────────────────

/// When the mock provider emits `Some(TokenUsage)` in its `Done` event, the
/// `done` SSE payload must include a `"usage"` object with the correct totals.
#[sqlx::test(migrations = "./migrations")]
async fn chat_done_event_includes_usage_when_provider_reports_it(pool: PgPool) {
    use planner_backend::ai::provider::TokenUsage;

    let round1 = vec![
        ProviderEvent::Token("Olá!".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: Some(TokenUsage {
                input_tokens: 300,
                output_tokens: 45,
            }),
        },
    ];

    let scripted = ScriptedMockProvider::new(vec![round1]).into_shared();
    let scripted_arc: Arc<dyn planner_backend::ai::provider::LlmProvider> = scripted;

    let app = build_app_with_mock(pool.clone(), scripted_arc);
    let token = register_and_token(&app, "chat-usage-present@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Oi",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse(&bytes);
    let (_, done_data) = events.iter().find(|(e, _)| e == "done").unwrap();
    let done_json: serde_json::Value = serde_json::from_str(done_data).unwrap();

    assert!(
        done_json.get("usage").is_some(),
        "done event must have a 'usage' key when provider reported usage"
    );
    assert_eq!(
        done_json["usage"]["input_tokens"].as_u64().unwrap(),
        300,
        "input_tokens mismatch"
    );
    assert_eq!(
        done_json["usage"]["output_tokens"].as_u64().unwrap(),
        45,
        "output_tokens mismatch"
    );
}

/// When the mock provider emits `None` for usage, the `done` event must omit
/// the `"usage"` key entirely.
#[sqlx::test(migrations = "./migrations")]
async fn chat_done_event_omits_usage_when_provider_reports_none(pool: PgPool) {
    let mock = make_mock(vec!["Tudo bem!"]);
    let app = build_app_with_mock(pool.clone(), mock);
    let token = register_and_token(&app, "chat-usage-absent@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Oi",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse(&bytes);
    let (_, done_data) = events.iter().find(|(e, _)| e == "done").unwrap();
    let done_json: serde_json::Value = serde_json::from_str(done_data).unwrap();

    assert!(
        done_json.get("usage").is_none(),
        "done event must NOT have 'usage' when provider reported None; got: {done_json}"
    );
}

/// Token usage from multiple tool-use rounds is summed in the `done` event.
#[sqlx::test(migrations = "./migrations")]
async fn chat_done_event_accumulates_usage_across_rounds(pool: PgPool) {
    use planner_backend::ai::provider::TokenUsage;

    let tc = make_tool_call("list_blocks", json!({}));

    // Round 1: tool call with usage.
    let round1 = vec![
        ProviderEvent::ToolCall(tc),
        ProviderEvent::Done {
            finish_reason: FinishReason::ToolCalls,
            usage: Some(TokenUsage {
                input_tokens: 200,
                output_tokens: 10,
            }),
        },
    ];
    // Round 2: text response with usage.
    let round2 = vec![
        ProviderEvent::Token("Aqui estão os blocos.".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: Some(TokenUsage {
                input_tokens: 250,
                output_tokens: 30,
            }),
        },
    ];

    let scripted = ScriptedMockProvider::new(vec![round1, round2]).into_shared();
    let scripted_arc: Arc<dyn planner_backend::ai::provider::LlmProvider> = scripted;

    let app = build_app_with_mock(pool.clone(), scripted_arc);
    let token = register_and_token(&app, "chat-usage-multi-round@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Liste meus blocos.",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse(&bytes);
    let (_, done_data) = events.iter().find(|(e, _)| e == "done").unwrap();
    let done_json: serde_json::Value = serde_json::from_str(done_data).unwrap();

    assert!(
        done_json.get("usage").is_some(),
        "done event must have usage when all rounds report it"
    );
    // Totals: input 200 + 250 = 450; output 10 + 30 = 40.
    assert_eq!(
        done_json["usage"]["input_tokens"].as_u64().unwrap(),
        450,
        "input_tokens should be summed across rounds"
    );
    assert_eq!(
        done_json["usage"]["output_tokens"].as_u64().unwrap(),
        40,
        "output_tokens should be summed across rounds"
    );
}
