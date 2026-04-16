// Integration tests for token usage persistence, budget enforcement, and
// kill-switch in the chat handler.
//
// Coverage:
//  - Messages row has non-null input_tokens, output_tokens, model after a chat turn
//  - Budget exceeded → 429
//  - Kill-switch (chat_enabled=false) → 503
//  - budget_warning in done event when near limit

mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{
    ScriptedMockProvider, build_app_with_mock, json_oneshot, raw_oneshot, register_test_user,
};
use planner_backend::ai::provider::{FinishReason, ProviderEvent, TokenUsage};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn login_token(app: &axum::Router, email: &str) -> String {
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

fn parse_sse_events(bytes: &[u8]) -> Vec<(String, serde_json::Value)> {
    common::parse_sse_body(bytes)
}

fn make_mock_with_usage(
    input_tokens: u32,
    output_tokens: u32,
) -> Arc<dyn planner_backend::ai::provider::LlmProvider> {
    let round = vec![
        ProviderEvent::Token("response".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: Some(TokenUsage {
                input_tokens,
                output_tokens,
            }),
        },
    ];
    Arc::new(ScriptedMockProvider::new(vec![round]).with_name("mock"))
}

// ── Token persistence ─────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_message_persists_token_usage_and_model(pool: PgPool) {
    let mock = make_mock_with_usage(300, 45);
    let app = build_app_with_mock(pool.clone(), mock);
    let token = login_token(&app, "chat-tok-persist@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Teste",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse_events(&bytes);
    let (_, done_val) = events.iter().find(|(e, _)| e == "done").unwrap();
    let conv_id: Uuid = done_val["conversation_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Query the assistant message row.
    let row = sqlx::query!(
        "SELECT input_tokens, output_tokens, model FROM messages \
         WHERE conversation_id = $1 AND role = 'assistant' \
         ORDER BY created_at DESC LIMIT 1",
        conv_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        row.input_tokens,
        Some(300),
        "input_tokens must be persisted"
    );
    assert_eq!(
        row.output_tokens,
        Some(45),
        "output_tokens must be persisted"
    );
    assert!(row.model.is_some(), "model must be persisted; got None");
}

#[sqlx::test(migrations = "./migrations")]
async fn chat_message_model_column_is_not_null_when_provider_known(pool: PgPool) {
    // Even with no usage (None), the model column should be set from config.
    let round = vec![
        ProviderEvent::Token("ok".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];
    let mock = Arc::new(ScriptedMockProvider::new(vec![round]).with_name("mock"))
        as Arc<dyn planner_backend::ai::provider::LlmProvider>;

    let app = build_app_with_mock(pool.clone(), mock);
    let token = login_token(&app, "chat-model-col@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Teste",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse_events(&bytes);
    let (_, done_val) = events.iter().find(|(e, _)| e == "done").unwrap();
    let conv_id: Uuid = done_val["conversation_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let model: Option<String> = sqlx::query_scalar(
        "SELECT model FROM messages WHERE conversation_id = $1 AND role = 'assistant'",
    )
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(
        model.is_some(),
        "model column must be set even when provider reports no usage"
    );
}

// ── Kill-switch ──────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_returns_503_when_kill_switch_enabled(pool: PgPool) {
    // Disable chat via the settings table.
    sqlx::query("UPDATE app_settings SET value = 'false' WHERE key = 'chat_enabled'")
        .execute(&pool)
        .await
        .unwrap();

    let mock = make_mock_with_usage(100, 10);
    let app = build_app_with_mock(pool.clone(), mock);
    let token = login_token(&app, "chat-kill-switch@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, body) = json_oneshot(
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

    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "kill-switch must return 503; got {status}: {body}"
    );
    assert_eq!(
        body["error"], "chat_disabled",
        "error field must be chat_disabled; got {body}"
    );
}

// ── Budget exceeded ───────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_returns_429_when_budget_exceeded(pool: PgPool) {
    // Register user first (need their UUID for the upsert).
    let mock = make_mock_with_usage(100, 10);
    let app = build_app_with_mock(pool.clone(), mock);
    let token = login_token(&app, "chat-budget-exceeded@example.com").await;
    let routine = create_routine(&app, &token).await;

    // Get the user ID.
    let user_id: Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'chat-budget-exceeded@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();

    // Set budget very low.
    sqlx::query("UPDATE app_settings SET value = '0.01' WHERE key = 'budget_monthly_usd'")
        .execute(&pool)
        .await
        .unwrap();

    // Manually insert usage that exceeds the budget.
    sqlx::query(
        "INSERT INTO llm_usage_daily \
         (day, user_id, provider, model, input_tokens, output_tokens, request_count, estimated_cost_usd) \
         VALUES (CURRENT_DATE, $1, 'gemini', 'gemini-flash', 1000000, 500000, 1, 0.60)"
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, body) = json_oneshot(
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

    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "exceeded budget must return 429; got {status}: {body}"
    );
    assert_eq!(
        body["error"], "budget_exceeded",
        "error field must be budget_exceeded; got {body}"
    );
    // Financial data (monthly_spend, budget) must NOT appear in the response
    // to avoid leaking spend information in HTTP error bodies.
    assert!(
        body.get("monthly_spend").is_none(),
        "monthly_spend must not appear in 429 response; got {body}"
    );
    assert!(
        body.get("budget").is_none(),
        "budget must not appear in 429 response; got {body}"
    );
}

// ── budget_warning in done event ─────────────────────────────────────────────

/// Seed `llm_usage_daily` so the user's current-month spend is just above the
/// warning threshold (`budget_monthly_usd * budget_warn_pct / 100`).
///
/// Default seed values: budget=$5.00, warn_pct=80 → threshold=$4.00.
/// We insert $4.01 to guarantee `monthly_spend >= threshold` without exceeding
/// the hard budget cap of $5.00.
async fn seed_spend_above_warning_threshold(pool: &PgPool, user_id: uuid::Uuid) {
    sqlx::query(
        "INSERT INTO llm_usage_daily \
         (day, user_id, provider, model, input_tokens, output_tokens, request_count, estimated_cost_usd) \
         VALUES (CURRENT_DATE, $1, 'gemini', 'gemini-flash', 0, 0, 0, 4.01)",
    )
    .bind(user_id)
    .execute(pool)
    .await
    .expect("failed to seed llm_usage_daily above warning threshold");
}

#[sqlx::test(migrations = "./migrations")]
async fn chat_done_event_includes_budget_warning_true_when_near_limit(pool: PgPool) {
    let mock = make_mock_with_usage(10, 5);
    let app = build_app_with_mock(pool.clone(), mock);
    let token = login_token(&app, "chat-budget-warn-true@example.com").await;
    let routine = create_routine(&app, &token).await;

    // Fetch the user UUID so we can seed usage directly.
    let user_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM users WHERE email = 'chat-budget-warn-true@example.com'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // Seed a spend just above the 80% warning threshold (default: $4.01 of $5.00).
    seed_spend_above_warning_threshold(&pool, user_id).await;

    let (status, bytes) = raw_oneshot(
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
    assert_eq!(
        status,
        StatusCode::OK,
        "chat must succeed when below hard budget"
    );

    let events = parse_sse_events(&bytes);
    let (_, done_val) = events
        .iter()
        .find(|(e, _)| e == "done")
        .expect("done event must be present");

    assert!(
        done_val.get("budget_warning").is_some(),
        "done event must include budget_warning field; got {done_val}"
    );
    assert!(
        done_val["budget_warning"]
            .as_bool()
            .expect("budget_warning must be a bool"),
        "budget_warning must be true when monthly spend exceeds warn threshold; got {done_val}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn chat_done_event_includes_budget_warning_false_by_default(pool: PgPool) {
    let mock = make_mock_with_usage(10, 5);
    let app = build_app_with_mock(pool.clone(), mock);
    let token = login_token(&app, "chat-budget-warn-false@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
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
    assert_eq!(status, StatusCode::OK);

    let events = parse_sse_events(&bytes);
    let (_, done_val) = events.iter().find(|(e, _)| e == "done").unwrap();

    // budget_warning key must be present.
    assert!(
        done_val.get("budget_warning").is_some(),
        "done event must include budget_warning field"
    );
    // With no prior spend, warning must be false.
    assert!(
        !done_val["budget_warning"].as_bool().unwrap(),
        "budget_warning should be false with negligible spend"
    );
}
