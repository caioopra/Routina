mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{
    MockLlmProvider, build_app, build_app_with_mock, json_oneshot, raw_oneshot, register_test_user,
};
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
