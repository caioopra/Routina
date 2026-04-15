//! End-to-end integration tests for the LLM chat flow.
//!
//! These tests exercise the full cradle-to-grave cycle: HTTP request →
//! provider mock → tool executor → DB writes → SSE response → DB assertions.
//!
//! ## Multi-round dynamic scripting note
//! The current `ScriptedMockProvider` pre-programs event sequences at
//! construction time.  Round 1's `update_block` tool call would ideally
//! reference the block_id produced by round 0's `create_block`, but that ID
//! is only known after the DB insert.  The approach for multi-dependent-round
//! scenarios (option a) would be to extend `ScriptedMockProvider` with a
//! `Box<dyn Fn(&[Message]) -> Vec<ProviderEvent> + Send + Sync>` per round so
//! the closure can inspect the tool-result message injected by the handler and
//! parse the block_id out.  For the initial implementation we use the simpler
//! 2-round approach (option b): create only, then respond with Stop — this
//! still covers the full loop including tool execution, `routine_updated`
//! events, and audit log writes.

mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{
    ScriptedMockProvider, build_app_with_mock, json_oneshot, parse_sse_body, raw_oneshot,
    register_test_user,
};
use planner_backend::ai::provider::{FinishReason, ProviderEvent, ToolCall};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Local helpers
// ---------------------------------------------------------------------------

async fn register_and_token(app: &axum::Router, email: &str) -> String {
    let body = register_test_user(app, email, "longenoughpass").await;
    body["token"].as_str().unwrap().to_string()
}

async fn create_routine(app: &axum::Router, token: &str) -> Value {
    let (status, body) = json_oneshot(
        app,
        Method::POST,
        "/api/routines",
        Some(json!({ "name": "E2E Test Routine" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create_routine failed: {body}");
    body
}

/// Build a `ToolCall` with the given name and arguments.
fn make_tool_call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: format!("call-{}", Uuid::now_v7()),
        name: name.to_string(),
        args,
    }
}

/// Assert that the SSE frame sequence contains exactly the named events in the
/// given order, starting from `start_index`.  Returns the index of the last
/// matched frame for chaining.
fn assert_event_order(
    frames: &[(String, Value)],
    expected_names: &[&str],
    start_index: usize,
) -> usize {
    let mut cursor = start_index;
    for expected in expected_names {
        let found = frames[cursor..]
            .iter()
            .enumerate()
            .find(|(_, (name, _))| name == expected);
        let (offset, _) = found.unwrap_or_else(|| {
            let remaining: Vec<&str> = frames[cursor..].iter().map(|(n, _)| n.as_str()).collect();
            panic!(
                "Expected event '{expected}' not found after index {cursor}. \
                 Remaining events: {remaining:?}"
            )
        });
        cursor += offset + 1;
    }
    cursor - 1
}

// ---------------------------------------------------------------------------
// Test 1: e2e_full_tool_use_cycle
// ---------------------------------------------------------------------------
//
// Scenario:
//   Round 0 — provider emits Token + ToolCall(create_block) + Done(ToolCalls)
//   Round 1 — provider emits Token + Done(Stop)
//
// Verifies:
//   • SSE event order: provider → token → tool_call → tool_result(success) →
//     routine_updated → token → done
//   • DB: user msg, assistant(tool_calls) msg, tool msg, final assistant msg
//   • assistant message's tool_calls JSONB contains the create_block call id
//   • tool message's tool_call_id = the scripted call id
//   • routine_actions has exactly 1 row for this conversation
//   • blocks table has the new row with title "Planning"

#[sqlx::test(migrations = "./migrations")]
async fn e2e_full_tool_use_cycle(pool: PgPool) {
    let tc = make_tool_call(
        "create_block",
        json!({
            "day_of_week": 1,
            "start_time": "09:00",
            "end_time": "10:00",
            "title": "Planning",
            "type": "trabalho"
        }),
    );
    let tc_id = tc.id.clone();

    // Round 0: provider announces a tool call and stops for execution.
    let round0 = vec![
        ProviderEvent::Token("Vou criar um bloco.".to_string()),
        ProviderEvent::ToolCall(tc),
        ProviderEvent::Done {
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
    ];
    // Round 1: provider has seen the tool result and wraps up.
    let round1 = vec![
        ProviderEvent::Token("Bloco criado com sucesso!".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let scripted = ScriptedMockProvider::new(vec![round0, round1])
        .with_name("mock_scripted")
        .into_shared();
    let provider_arc: Arc<dyn planner_backend::ai::provider::LlmProvider> = scripted;

    let app = build_app_with_mock(pool.clone(), provider_arc);
    let token = register_and_token(&app, "e2e-full-cycle@example.com").await;
    let routine = create_routine(&app, &token).await;
    let routine_id = routine["id"].as_str().unwrap();

    // ── Fire the request ────────────────────────────────────────────────────
    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Crie um bloco de planejamento na segunda-feira.",
            "routine_id": routine_id
        })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected SSE 200");

    let frames = parse_sse_body(&bytes);
    let event_names: Vec<&str> = frames.iter().map(|(n, _)| n.as_str()).collect();

    // ── Assert SSE event order ───────────────────────────────────────────────

    // 1. First event must be `provider`.
    assert_eq!(
        event_names[0], "provider",
        "first SSE event must be 'provider'; got: {event_names:?}"
    );
    assert_eq!(
        frames[0].1["name"].as_str().unwrap(),
        "mock_scripted",
        "provider name mismatch"
    );

    // 2. Ordered sequence: token → tool_call → tool_result → routine_updated → token → done
    assert_event_order(
        &frames,
        &[
            "token",
            "tool_call",
            "tool_result",
            "routine_updated",
            "token",
            "done",
        ],
        1,
    );

    // 3. tool_call payload shape.
    let (_, tc_data) = frames.iter().find(|(n, _)| n == "tool_call").unwrap();
    assert_eq!(tc_data["id"].as_str().unwrap(), tc_id.as_str());
    assert_eq!(tc_data["name"].as_str().unwrap(), "create_block");
    assert_eq!(tc_data["args"]["title"].as_str().unwrap(), "Planning");
    assert_eq!(tc_data["args"]["type"].as_str().unwrap(), "trabalho");
    assert_eq!(tc_data["args"]["day_of_week"].as_i64().unwrap(), 1);

    // 4. tool_result payload shape.
    let (_, tr_data) = frames.iter().find(|(n, _)| n == "tool_result").unwrap();
    assert_eq!(
        tr_data["id"].as_str().unwrap(),
        tc_id.as_str(),
        "tool_result id must match tool_call id"
    );
    assert!(
        tr_data["success"].as_bool().unwrap(),
        "tool execution must succeed"
    );
    assert!(
        tr_data["data"].is_object() || tr_data["data"].is_array(),
        "tool_result data must be present"
    );
    // The created block data should contain the title.
    let block_data = &tr_data["data"];
    assert_eq!(
        block_data["title"].as_str().unwrap(),
        "Planning",
        "tool_result data must contain the created block"
    );

    // 5. routine_updated carries the correct routine_id.
    let (_, ru_data) = frames.iter().find(|(n, _)| n == "routine_updated").unwrap();
    assert_eq!(
        ru_data["routine_id"].as_str().unwrap(),
        routine_id,
        "routine_updated must reference the correct routine"
    );

    // 6. Final `done` event carries conversation_id and message_id.
    let (_, done_data) = frames.iter().find(|(n, _)| n == "done").unwrap();
    assert!(
        done_data["conversation_id"].is_string(),
        "done must contain conversation_id"
    );
    assert!(
        done_data["message_id"].is_string(),
        "done must contain message_id"
    );

    let conv_id: Uuid = done_data["conversation_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // ── Assert DB state ─────────────────────────────────────────────────────

    // Total message count: user + assistant(round0, has tool_calls) + tool + assistant(round1).
    let msg_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE conversation_id = $1")
            .bind(conv_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        msg_count, 4,
        "expected 4 messages: user, assistant(tool_calls), tool, final assistant"
    );

    // The first assistant message must have tool_calls JSONB containing our call id.
    let asst_with_tools = sqlx::query!(
        "SELECT tool_calls FROM messages \
         WHERE conversation_id = $1 AND role = 'assistant' AND tool_calls IS NOT NULL \
         ORDER BY created_at ASC \
         LIMIT 1",
        conv_id
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(
        asst_with_tools.is_some(),
        "expected an assistant message with non-null tool_calls"
    );
    let tool_calls_json = asst_with_tools.unwrap().tool_calls.unwrap();
    let tool_calls_arr = tool_calls_json.as_array().unwrap();
    assert!(
        !tool_calls_arr.is_empty(),
        "tool_calls array must not be empty"
    );
    let stored_tc_id = tool_calls_arr[0]["id"].as_str().unwrap();
    assert_eq!(
        stored_tc_id,
        tc_id.as_str(),
        "stored tool_call id must match the scripted call id"
    );

    // The tool message must reference tc_id via tool_call_id.
    let tool_msg = sqlx::query!(
        "SELECT tool_call_id FROM messages \
         WHERE conversation_id = $1 AND role = 'tool' \
         ORDER BY created_at ASC \
         LIMIT 1",
        conv_id
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(tool_msg.is_some(), "expected a tool role message in DB");
    assert_eq!(
        tool_msg.unwrap().tool_call_id.as_deref(),
        Some(tc_id.as_str()),
        "tool message tool_call_id must match the scripted call id"
    );

    // Exactly one routine_actions row for this conversation.
    let action_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM routine_actions WHERE conversation_id = $1")
            .bind(conv_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        action_count, 1,
        "expected exactly 1 routine_actions audit row"
    );

    // The action was a create_block targeting our routine.
    let action = sqlx::query!(
        "SELECT action_type FROM routine_actions WHERE conversation_id = $1",
        conv_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(action.action_type, "create_block");

    // The block is in the blocks table with the right title.
    let block_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM blocks WHERE routine_id = $1 AND title = 'Planning'",
    )
    .bind(routine_id.parse::<Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        block_count, 1,
        "expected the 'Planning' block to exist in DB"
    );
}

// ---------------------------------------------------------------------------
// Test 2: e2e_tool_error_does_not_break_stream
// ---------------------------------------------------------------------------
//
// A tool call that will fail at the executor level (update_block with a
// block_id that was never created → authz failure → "not_found").
//
// Verifies:
//   • SSE still completes normally with `done`.
//   • `tool_result` carries `success: false` and error data.
//   • No `routine_updated` event (nothing was mutated).
//   • No `routine_actions` row (no successful mutation).

#[sqlx::test(migrations = "./migrations")]
async fn e2e_tool_error_does_not_break_stream(pool: PgPool) {
    // Fabricate a block_id that doesn't exist in the DB.
    let nonexistent_block_id = Uuid::now_v7();

    let tc = make_tool_call(
        "update_block",
        json!({
            "block_id": nonexistent_block_id.to_string(),
            "title": "Should Fail"
        }),
    );
    let tc_id = tc.id.clone();

    // Round 0: tool call that will fail.
    let round0 = vec![
        ProviderEvent::ToolCall(tc),
        ProviderEvent::Done {
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
    ];
    // Round 1: provider receives the failure result and gracefully wraps up.
    let round1 = vec![
        ProviderEvent::Token("Não foi possível encontrar o bloco.".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let scripted = ScriptedMockProvider::new(vec![round0, round1]).into_shared();
    let provider_arc: Arc<dyn planner_backend::ai::provider::LlmProvider> = scripted;

    let app = build_app_with_mock(pool.clone(), provider_arc);
    let token = register_and_token(&app, "e2e-tool-error@example.com").await;
    let routine = create_routine(&app, &token).await;

    let (status, bytes) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Atualize o bloco.",
            "routine_id": routine["id"].as_str().unwrap()
        })),
        Some(&token),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "SSE endpoint must return 200 even on tool error"
    );

    let frames = parse_sse_body(&bytes);
    let event_names: Vec<&str> = frames.iter().map(|(n, _)| n.as_str()).collect();

    // tool_call and tool_result must both appear.
    assert!(
        event_names.contains(&"tool_call"),
        "expected tool_call event; got: {event_names:?}"
    );
    assert!(
        event_names.contains(&"tool_result"),
        "expected tool_result event; got: {event_names:?}"
    );

    // tool_result must report failure.
    let (_, tr_data) = frames.iter().find(|(n, _)| n == "tool_result").unwrap();
    assert_eq!(
        tr_data["id"].as_str().unwrap(),
        tc_id.as_str(),
        "tool_result id must match tool_call id"
    );
    assert!(
        !tr_data["success"].as_bool().unwrap_or(true),
        "tool_result must have success=false for a not-found update"
    );
    // The error payload must contain an "error" key.
    assert!(
        tr_data["data"]["error"].is_string(),
        "tool_result data must contain an error string; got: {}",
        tr_data["data"]
    );

    // No routine_updated event because nothing was mutated.
    assert!(
        !event_names.contains(&"routine_updated"),
        "routine_updated must NOT appear when the tool failed; got: {event_names:?}"
    );

    // Stream must end with `done`, not `error`.
    let last_event = event_names.last().unwrap();
    assert_eq!(
        *last_event, "done",
        "SSE must end with 'done' even after a failed tool call; last event: {last_event}"
    );

    // Extract conversation_id for DB assertions.
    let (_, done_data) = frames.iter().find(|(n, _)| n == "done").unwrap();
    let conv_id: Uuid = done_data["conversation_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // No routine_actions row — nothing was committed.
    let action_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM routine_actions WHERE conversation_id = $1")
            .bind(conv_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        action_count, 0,
        "no routine_actions row should exist after a failed tool call"
    );
}

// ---------------------------------------------------------------------------
// Test 3: e2e_undo_round_trip
// ---------------------------------------------------------------------------
//
// Two sequential HTTP requests on the same conversation:
//   Request 1 — create_block via LLM → block persisted + routine_actions row.
//   Request 2 — undo_last_action via LLM → block deleted + undone_at set.
//
// This exercises `ScriptedMockProvider` keeping per-round state across
// separate HTTP calls: the provider `rounds` mutex is drained across calls,
// so rounds[0] and rounds[1] go to request 1, and rounds[2] goes to request 2.

#[sqlx::test(migrations = "./migrations")]
async fn e2e_undo_round_trip(pool: PgPool) {
    // Request 1 scripts: round 0 creates block, round 1 wraps up.
    let create_tc = make_tool_call(
        "create_block",
        json!({
            "day_of_week": 3,
            "start_time": "14:00",
            "title": "ToBeUndone",
            "type": "livre"
        }),
    );
    let undo_tc = make_tool_call("undo_last_action", json!({}));

    let round0 = vec![
        ProviderEvent::Token("Criando bloco.".to_string()),
        ProviderEvent::ToolCall(create_tc),
        ProviderEvent::Done {
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
    ];
    let round1 = vec![
        ProviderEvent::Token("Bloco criado.".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];
    // Request 2 scripts: round 2 calls undo, round 3 wraps up.
    let round2 = vec![
        ProviderEvent::Token("Desfazendo.".to_string()),
        ProviderEvent::ToolCall(undo_tc),
        ProviderEvent::Done {
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
    ];
    let round3 = vec![
        ProviderEvent::Token("Feito.".to_string()),
        ProviderEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    // All 4 rounds loaded into a single shared provider so state persists.
    let scripted = ScriptedMockProvider::new(vec![round0, round1, round2, round3]).into_shared();
    let provider_arc: Arc<dyn planner_backend::ai::provider::LlmProvider> = scripted;

    let app = build_app_with_mock(pool.clone(), provider_arc);
    let token = register_and_token(&app, "e2e-undo@example.com").await;
    let routine = create_routine(&app, &token).await;
    let routine_id = routine["id"].as_str().unwrap();

    // ── Request 1: create the block ─────────────────────────────────────────
    let (status1, bytes1) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "message": "Crie um bloco para mim.",
            "routine_id": routine_id
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status1, StatusCode::OK, "request 1 must return 200");

    let frames1 = parse_sse_body(&bytes1);
    let event_names1: Vec<&str> = frames1.iter().map(|(n, _)| n.as_str()).collect();

    // Request 1 must have: tool_call → tool_result(success) → routine_updated → done.
    assert!(
        event_names1.contains(&"tool_call"),
        "request 1 must have tool_call; got {event_names1:?}"
    );
    assert!(
        event_names1.contains(&"routine_updated"),
        "request 1 must have routine_updated; got {event_names1:?}"
    );
    assert_eq!(
        event_names1.last().unwrap(),
        &"done",
        "request 1 must end with done"
    );

    // Extract conversation_id from the first done event.
    let (_, done1_data) = frames1.iter().find(|(n, _)| n == "done").unwrap();
    let conv_id: Uuid = done1_data["conversation_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Block exists in DB after request 1.
    let block_count_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM blocks WHERE routine_id = $1 AND title = 'ToBeUndone'",
    )
    .bind(routine_id.parse::<Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        block_count_before, 1,
        "block 'ToBeUndone' must exist after create"
    );

    // One routine_actions row with undone_at IS NULL.
    let undone_at_before: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT undone_at FROM routine_actions WHERE conversation_id = $1")
            .bind(conv_id)
            .fetch_optional(&pool)
            .await
            .unwrap()
            .flatten();
    assert!(
        undone_at_before.is_none(),
        "routine_actions.undone_at must be NULL before undo"
    );

    // ── Request 2: undo the block creation ──────────────────────────────────
    let (status2, bytes2) = raw_oneshot(
        &app,
        Method::POST,
        "/api/chat/message",
        Some(json!({
            "conversation_id": conv_id.to_string(),
            "message": "Desfaça a última ação."
        })),
        Some(&token),
    )
    .await;
    assert_eq!(status2, StatusCode::OK, "request 2 must return 200");

    let frames2 = parse_sse_body(&bytes2);
    let event_names2: Vec<&str> = frames2.iter().map(|(n, _)| n.as_str()).collect();

    // Request 2 must have: tool_call(undo) → tool_result(success) → routine_updated → done.
    assert!(
        event_names2.contains(&"tool_call"),
        "request 2 must have tool_call; got {event_names2:?}"
    );
    assert!(
        event_names2.contains(&"routine_updated"),
        "request 2 must have routine_updated for the undo mutation; got {event_names2:?}"
    );
    assert_eq!(
        event_names2.last().unwrap(),
        &"done",
        "request 2 must end with done"
    );

    // undo tool_call must be for undo_last_action.
    let (_, undo_tc_data) = frames2.iter().find(|(n, _)| n == "tool_call").unwrap();
    assert_eq!(
        undo_tc_data["name"].as_str().unwrap(),
        "undo_last_action",
        "tool_call in request 2 must be undo_last_action"
    );

    // undo tool_result must succeed.
    let (_, undo_tr_data) = frames2.iter().find(|(n, _)| n == "tool_result").unwrap();
    assert!(
        undo_tr_data["success"].as_bool().unwrap_or(false),
        "undo_last_action must succeed; got: {}",
        undo_tr_data
    );

    // ── Post-undo DB assertions ──────────────────────────────────────────────

    // Block is gone from blocks table.
    let block_count_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM blocks WHERE routine_id = $1 AND title = 'ToBeUndone'",
    )
    .bind(routine_id.parse::<Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        block_count_after, 0,
        "block 'ToBeUndone' must be gone after undo"
    );

    // routine_actions row for the create_block now has undone_at set.
    let undone_at_after: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT undone_at FROM routine_actions \
         WHERE conversation_id = $1 AND action_type = 'create_block'",
    )
    .bind(conv_id)
    .fetch_optional(&pool)
    .await
    .unwrap()
    .flatten();
    assert!(
        undone_at_after.is_some(),
        "routine_actions.undone_at must be set after undo"
    );
}
