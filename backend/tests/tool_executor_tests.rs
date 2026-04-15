mod common;

use planner_backend::ai::provider::ToolCall;
use planner_backend::ai::tools::executor::{ToolContext, execute_tool};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// DB seed helpers
// ---------------------------------------------------------------------------

async fn create_user(pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, name, password_hash) \
         VALUES ($1, 'Test User', 'hash') \
         RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_routine(pool: &PgPool, user_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO routines (user_id, name) VALUES ($1, 'Test Routine') RETURNING id",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_conversation(pool: &PgPool, user_id: Uuid, routine_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (user_id, routine_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(user_id)
    .bind(routine_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_label(pool: &PgPool, user_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO labels (user_id, name, color_bg, color_text, color_border) \
         VALUES ($1, $2, '#111', '#fff', '#000') RETURNING id",
    )
    .bind(user_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn make_call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: Uuid::now_v7().to_string(),
        name: name.to_string(),
        args,
    }
}

fn default_block_args() -> Value {
    json!({
        "day_of_week": 1,
        "start_time": "09:00",
        "end_time": "10:00",
        "title": "Morning Work",
        "type": "trabalho",
        "sort_order": 0
    })
}

// ---------------------------------------------------------------------------
// list_blocks
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_blocks_empty_routine(pool: PgPool) {
    let user_id = create_user(&pool, "lb-empty@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let call = make_call("list_blocks", json!({}));
    let result = execute_tool(&ctx, &call).await;

    assert!(result.success);
    assert!(!result.mutated_routine);
    assert_eq!(result.data.as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_blocks_returns_all(pool: PgPool) {
    let user_id = create_user(&pool, "lb-all@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };

    // Create two blocks on different days
    let create1 = make_call(
        "create_block",
        json!({
            "day_of_week": 1, "start_time": "09:00", "title": "Mon Work", "type": "trabalho"
        }),
    );
    let create2 = make_call(
        "create_block",
        json!({
            "day_of_week": 2, "start_time": "10:00", "title": "Tue Work", "type": "trabalho"
        }),
    );
    execute_tool(&ctx, &create1).await;
    execute_tool(&ctx, &create2).await;

    let list = make_call("list_blocks", json!({}));
    let result = execute_tool(&ctx, &list).await;
    assert!(result.success);
    assert_eq!(result.data.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_blocks_with_day_filter(pool: PgPool) {
    let user_id = create_user(&pool, "lb-filter@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };

    for day in [1, 2, 3] {
        execute_tool(
            &ctx,
            &make_call(
                "create_block",
                json!({ "day_of_week": day, "start_time": "08:00", "title": "Block", "type": "livre" }),
            ),
        )
        .await;
    }

    let result = execute_tool(&ctx, &make_call("list_blocks", json!({ "day_of_week": 2 }))).await;
    assert!(result.success);
    assert_eq!(result.data.as_array().unwrap().len(), 1);
    assert_eq!(result.data[0]["day_of_week"], 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_blocks_without_day_filter_returns_all_days(pool: PgPool) {
    let user_id = create_user(&pool, "lb-nofilter@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };

    for day in [0, 3, 6] {
        execute_tool(
            &ctx,
            &make_call(
                "create_block",
                json!({ "day_of_week": day, "start_time": "08:00", "title": "Block", "type": "livre" }),
            ),
        )
        .await;
    }

    let result = execute_tool(&ctx, &make_call("list_blocks", json!({}))).await;
    assert!(result.success);
    assert_eq!(result.data.as_array().unwrap().len(), 3);
}

// ---------------------------------------------------------------------------
// create_block
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn create_block_happy_path(pool: PgPool) {
    let user_id = create_user(&pool, "cb-happy@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let call = make_call("create_block", default_block_args());
    let result = execute_tool(&ctx, &call).await;

    assert!(result.success, "data: {}", result.data);
    assert!(result.mutated_routine);
    assert_eq!(result.data["title"], "Morning Work");
    assert_eq!(result.data["type"], "trabalho");
    assert_eq!(result.data["day_of_week"], 1);
    assert!(result.data["id"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_writes_to_db(pool: PgPool) {
    let user_id = create_user(&pool, "cb-db@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    execute_tool(&ctx, &make_call("create_block", default_block_args())).await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE routine_id = $1")
        .bind(routine_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_with_label_names_links_labels(pool: PgPool) {
    let user_id = create_user(&pool, "cb-labels@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;
    let _label_id = create_label(&pool, user_id, "important").await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let mut args = default_block_args();
    args["label_names"] = json!(["important"]);

    let result = execute_tool(&ctx, &make_call("create_block", args)).await;
    assert!(result.success, "data: {}", result.data);

    let labels = result.data["labels"].as_array().unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0]["name"], "important");

    // Verify in DB
    let block_id: Uuid = serde_json::from_value(result.data["id"].clone()).unwrap();
    let link_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM block_labels WHERE block_id = $1")
            .bind(block_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(link_count, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_unknown_label_name_is_silently_ignored(pool: PgPool) {
    // Labels that don't exist for the user are simply not linked — no error.
    let user_id = create_user(&pool, "cb-unknownlabel@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let mut args = default_block_args();
    args["label_names"] = json!(["does_not_exist"]);

    let result = execute_tool(&ctx, &make_call("create_block", args)).await;
    assert!(result.success, "data: {}", result.data);
    assert_eq!(result.data["labels"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_records_audit_row(pool: PgPool) {
    let user_id = create_user(&pool, "cb-audit@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    execute_tool(&ctx, &make_call("create_block", default_block_args())).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routine_actions WHERE action_type = 'create_block' AND routine_id = $1",
    )
    .bind(routine_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_invalid_day_returns_error(pool: PgPool) {
    let user_id = create_user(&pool, "cb-badday@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let call = make_call(
        "create_block",
        json!({ "day_of_week": 7, "start_time": "09:00", "title": "Block", "type": "trabalho" }),
    );
    let result = execute_tool(&ctx, &call).await;
    assert!(!result.success);
    assert!(!result.mutated_routine);
}

// ---------------------------------------------------------------------------
// update_block
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn update_block_happy_path(pool: PgPool) {
    let user_id = create_user(&pool, "ub-happy@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(&ctx, &make_call("create_block", default_block_args())).await;
    assert!(create_result.success);
    let block_id = create_result.data["id"].as_str().unwrap();

    let update_result = execute_tool(
        &ctx,
        &make_call(
            "update_block",
            json!({ "block_id": block_id, "title": "Updated Title" }),
        ),
    )
    .await;

    assert!(update_result.success, "data: {}", update_result.data);
    assert!(update_result.mutated_routine);
    assert_eq!(update_result.data["title"], "Updated Title");
}

#[sqlx::test(migrations = "./migrations")]
async fn update_block_updates_labels(pool: PgPool) {
    let user_id = create_user(&pool, "ub-labels@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;
    create_label(&pool, user_id, "work").await;
    create_label(&pool, user_id, "urgent").await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(
        &ctx,
        &make_call(
            "create_block",
            json!({ "day_of_week": 1, "start_time": "09:00", "title": "Block", "type": "trabalho", "label_names": ["work"] }),
        ),
    )
    .await;
    assert!(create_result.success);
    let block_id = create_result.data["id"].as_str().unwrap();

    // Replace with a different label
    let update_result = execute_tool(
        &ctx,
        &make_call(
            "update_block",
            json!({ "block_id": block_id, "label_names": ["urgent"] }),
        ),
    )
    .await;
    assert!(update_result.success, "data: {}", update_result.data);
    let labels = update_result.data["labels"].as_array().unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0]["name"], "urgent");
}

#[sqlx::test(migrations = "./migrations")]
async fn update_block_authz_failure_different_routine(pool: PgPool) {
    // Block belongs to routine_1 but ctx.routine_id is routine_2.
    let user_id = create_user(&pool, "ub-authz@test.com").await;
    let routine1 = create_routine(&pool, user_id).await;
    let routine2 = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine2).await;

    let ctx1 = ToolContext {
        pool: &pool,
        user_id,
        routine_id: routine1,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(&ctx1, &make_call("create_block", default_block_args())).await;
    assert!(create_result.success);
    let block_id = create_result.data["id"].as_str().unwrap();

    // Now try to update it via a context locked to routine_2
    let ctx2 = ToolContext {
        pool: &pool,
        user_id,
        routine_id: routine2,
        conversation_id: conv_id,
    };
    let update_result = execute_tool(
        &ctx2,
        &make_call(
            "update_block",
            json!({ "block_id": block_id, "title": "Hijacked" }),
        ),
    )
    .await;

    assert!(
        !update_result.success,
        "expected error but got: {}",
        update_result.data
    );
    assert_eq!(update_result.data["error"], "not_found");
    assert!(!update_result.mutated_routine);

    // DB must be unchanged
    let db_title: String = sqlx::query_scalar("SELECT title FROM blocks WHERE id = $1")
        .bind(Uuid::parse_str(block_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(db_title, "Morning Work");
}

#[sqlx::test(migrations = "./migrations")]
async fn update_block_authz_failure_different_user(pool: PgPool) {
    let user1 = create_user(&pool, "ub-user1@test.com").await;
    let user2 = create_user(&pool, "ub-user2@test.com").await;
    let routine1 = create_routine(&pool, user1).await;
    let routine2 = create_routine(&pool, user2).await;
    let conv1 = create_conversation(&pool, user1, routine1).await;
    let conv2 = create_conversation(&pool, user2, routine2).await;

    let ctx1 = ToolContext {
        pool: &pool,
        user_id: user1,
        routine_id: routine1,
        conversation_id: conv1,
    };
    let create_result = execute_tool(&ctx1, &make_call("create_block", default_block_args())).await;
    assert!(create_result.success);
    let block_id = create_result.data["id"].as_str().unwrap();

    // user2 tries to update user1's block (using a different routine_id, so authz check fails)
    let ctx2 = ToolContext {
        pool: &pool,
        user_id: user2,
        routine_id: routine2,
        conversation_id: conv2,
    };
    let update_result = execute_tool(
        &ctx2,
        &make_call(
            "update_block",
            json!({ "block_id": block_id, "title": "Stolen" }),
        ),
    )
    .await;

    assert!(!update_result.success);
    assert_eq!(update_result.data["error"], "not_found");
}

// ---------------------------------------------------------------------------
// delete_block
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn delete_block_happy_path(pool: PgPool) {
    let user_id = create_user(&pool, "db-happy@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(&ctx, &make_call("create_block", default_block_args())).await;
    assert!(create_result.success);
    let block_id = create_result.data["id"].as_str().unwrap();

    let delete_result = execute_tool(
        &ctx,
        &make_call("delete_block", json!({ "block_id": block_id })),
    )
    .await;

    assert!(delete_result.success, "data: {}", delete_result.data);
    assert!(delete_result.mutated_routine);
    assert_eq!(delete_result.data["deleted"], true);

    // Block must be gone from DB
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE id = $1")
        .bind(Uuid::parse_str(block_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_block_authz_failure(pool: PgPool) {
    let user_id = create_user(&pool, "db-authz@test.com").await;
    let routine1 = create_routine(&pool, user_id).await;
    let routine2 = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine2).await;

    let ctx1 = ToolContext {
        pool: &pool,
        user_id,
        routine_id: routine1,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(&ctx1, &make_call("create_block", default_block_args())).await;
    let block_id = create_result.data["id"].as_str().unwrap();

    let ctx2 = ToolContext {
        pool: &pool,
        user_id,
        routine_id: routine2,
        conversation_id: conv_id,
    };
    let delete_result = execute_tool(
        &ctx2,
        &make_call("delete_block", json!({ "block_id": block_id })),
    )
    .await;

    assert!(!delete_result.success);
    assert_eq!(delete_result.data["error"], "not_found");

    // Block still in DB
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE id = $1")
        .bind(Uuid::parse_str(block_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// list_rules
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_rules_empty_routine(pool: PgPool) {
    let user_id = create_user(&pool, "lr-empty@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let result = execute_tool(&ctx, &make_call("list_rules", json!({}))).await;
    assert!(result.success);
    assert!(!result.mutated_routine);
    assert_eq!(result.data.as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// create_rule
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn create_rule_happy_path(pool: PgPool) {
    let user_id = create_user(&pool, "cr-happy@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let result = execute_tool(
        &ctx,
        &make_call("create_rule", json!({ "title": "No meetings before 10am" })),
    )
    .await;

    assert!(result.success, "data: {}", result.data);
    assert!(result.mutated_routine);
    assert_eq!(result.data["text"], "No meetings before 10am");
    assert!(result.data["id"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rule_with_description(pool: PgPool) {
    let user_id = create_user(&pool, "cr-desc@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let result = execute_tool(
        &ctx,
        &make_call(
            "create_rule",
            json!({ "title": "Focus blocks", "description": "At least 2h per day" }),
        ),
    )
    .await;

    assert!(result.success, "data: {}", result.data);
    assert_eq!(result.data["text"], "Focus blocks: At least 2h per day");
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rule_records_audit(pool: PgPool) {
    let user_id = create_user(&pool, "cr-audit@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    execute_tool(
        &ctx,
        &make_call("create_rule", json!({ "title": "Rule A" })),
    )
    .await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routine_actions WHERE action_type = 'create_rule' AND routine_id = $1",
    )
    .bind(routine_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// update_rule
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn update_rule_happy_path(pool: PgPool) {
    let user_id = create_user(&pool, "ur-happy@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(
        &ctx,
        &make_call("create_rule", json!({ "title": "Old title" })),
    )
    .await;
    assert!(create_result.success);
    let rule_id = create_result.data["id"].as_str().unwrap();

    let update_result = execute_tool(
        &ctx,
        &make_call(
            "update_rule",
            json!({ "rule_id": rule_id, "title": "New title" }),
        ),
    )
    .await;

    assert!(update_result.success, "data: {}", update_result.data);
    assert!(update_result.mutated_routine);
    assert_eq!(update_result.data["text"], "New title");
}

#[sqlx::test(migrations = "./migrations")]
async fn update_rule_authz_failure(pool: PgPool) {
    let user_id = create_user(&pool, "ur-authz@test.com").await;
    let routine1 = create_routine(&pool, user_id).await;
    let routine2 = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine2).await;

    let ctx1 = ToolContext {
        pool: &pool,
        user_id,
        routine_id: routine1,
        conversation_id: conv_id,
    };
    let create_result =
        execute_tool(&ctx1, &make_call("create_rule", json!({ "title": "Rule" }))).await;
    assert!(create_result.success);
    let rule_id = create_result.data["id"].as_str().unwrap();

    let ctx2 = ToolContext {
        pool: &pool,
        user_id,
        routine_id: routine2,
        conversation_id: conv_id,
    };
    let update_result = execute_tool(
        &ctx2,
        &make_call(
            "update_rule",
            json!({ "rule_id": rule_id, "title": "Hijacked" }),
        ),
    )
    .await;

    assert!(!update_result.success);
    assert_eq!(update_result.data["error"], "not_found");
    assert!(!update_result.mutated_routine);
}

// ---------------------------------------------------------------------------
// delete_rule
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn delete_rule_happy_path(pool: PgPool) {
    let user_id = create_user(&pool, "dr-happy@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(
        &ctx,
        &make_call("create_rule", json!({ "title": "Rule to delete" })),
    )
    .await;
    assert!(create_result.success);
    let rule_id = create_result.data["id"].as_str().unwrap();

    let delete_result = execute_tool(
        &ctx,
        &make_call("delete_rule", json!({ "rule_id": rule_id })),
    )
    .await;

    assert!(delete_result.success, "data: {}", delete_result.data);
    assert!(delete_result.mutated_routine);
    assert_eq!(delete_result.data["deleted"], true);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rules WHERE id = $1")
        .bind(Uuid::parse_str(rule_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

// ---------------------------------------------------------------------------
// list_labels
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_labels_returns_user_labels(pool: PgPool) {
    let user_id = create_user(&pool, "ll-user@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;
    create_label(&pool, user_id, "alpha").await;
    create_label(&pool, user_id, "beta").await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let result = execute_tool(&ctx, &make_call("list_labels", json!({}))).await;

    assert!(result.success);
    assert!(!result.mutated_routine);
    let labels = result.data.as_array().unwrap();
    assert_eq!(labels.len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_labels_does_not_return_other_user_labels(pool: PgPool) {
    let user1 = create_user(&pool, "ll-u1@test.com").await;
    let user2 = create_user(&pool, "ll-u2@test.com").await;
    let routine_id = create_routine(&pool, user1).await;
    let conv_id = create_conversation(&pool, user1, routine_id).await;
    create_label(&pool, user1, "mine").await;
    create_label(&pool, user2, "theirs").await;

    let ctx = ToolContext {
        pool: &pool,
        user_id: user1,
        routine_id,
        conversation_id: conv_id,
    };
    let result = execute_tool(&ctx, &make_call("list_labels", json!({}))).await;

    assert!(result.success);
    let labels = result.data.as_array().unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0]["name"], "mine");
}

// ---------------------------------------------------------------------------
// undo_last_action — create_block → undo
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn undo_create_block_removes_block(pool: PgPool) {
    let user_id = create_user(&pool, "undo-cb@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(&ctx, &make_call("create_block", default_block_args())).await;
    assert!(create_result.success);
    let block_id_str = create_result.data["id"].as_str().unwrap();
    let block_id = Uuid::parse_str(block_id_str).unwrap();

    // Block exists
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE id = $1")
        .bind(block_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    let undo_result = execute_tool(&ctx, &make_call("undo_last_action", json!({}))).await;
    assert!(undo_result.success, "data: {}", undo_result.data);
    assert!(undo_result.mutated_routine);
    assert_eq!(undo_result.data["undone"], "create_block");

    // Block must be gone
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE id = $1")
        .bind(block_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

// ---------------------------------------------------------------------------
// undo_last_action — update_block → undo (row back to original)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn undo_update_block_restores_original(pool: PgPool) {
    let user_id = create_user(&pool, "undo-ub@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(&ctx, &make_call("create_block", default_block_args())).await;
    assert!(create_result.success);
    let block_id = create_result.data["id"].as_str().unwrap();

    execute_tool(
        &ctx,
        &make_call(
            "update_block",
            json!({ "block_id": block_id, "title": "Changed" }),
        ),
    )
    .await;

    let undo_result = execute_tool(&ctx, &make_call("undo_last_action", json!({}))).await;
    assert!(undo_result.success, "data: {}", undo_result.data);
    assert_eq!(undo_result.data["undone"], "update_block");

    // Title must be restored to original
    let db_title: String = sqlx::query_scalar("SELECT title FROM blocks WHERE id = $1")
        .bind(Uuid::parse_str(block_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(db_title, "Morning Work");
}

// ---------------------------------------------------------------------------
// undo_last_action — delete_block → undo (row restored)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn undo_delete_block_restores_block(pool: PgPool) {
    let user_id = create_user(&pool, "undo-db@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(&ctx, &make_call("create_block", default_block_args())).await;
    assert!(create_result.success);
    let block_id = create_result.data["id"].as_str().unwrap();

    execute_tool(
        &ctx,
        &make_call("delete_block", json!({ "block_id": block_id })),
    )
    .await;

    // Block gone
    let count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE id = $1")
        .bind(Uuid::parse_str(block_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count_before, 0);

    let undo_result = execute_tool(&ctx, &make_call("undo_last_action", json!({}))).await;
    assert!(undo_result.success, "data: {}", undo_result.data);
    assert_eq!(undo_result.data["undone"], "delete_block");

    // Block restored
    let count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE id = $1")
        .bind(Uuid::parse_str(block_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count_after, 1);
}

// ---------------------------------------------------------------------------
// undo_last_action — create_rule → undo
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn undo_create_rule_removes_rule(pool: PgPool) {
    let user_id = create_user(&pool, "undo-cr@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(
        &ctx,
        &make_call("create_rule", json!({ "title": "No late meetings" })),
    )
    .await;
    assert!(create_result.success);
    let rule_id = Uuid::parse_str(create_result.data["id"].as_str().unwrap()).unwrap();

    let undo_result = execute_tool(&ctx, &make_call("undo_last_action", json!({}))).await;
    assert!(undo_result.success, "data: {}", undo_result.data);
    assert_eq!(undo_result.data["undone"], "create_rule");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rules WHERE id = $1")
        .bind(rule_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

// ---------------------------------------------------------------------------
// undo_last_action — update_rule → undo
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn undo_update_rule_restores_original(pool: PgPool) {
    let user_id = create_user(&pool, "undo-ur@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(
        &ctx,
        &make_call("create_rule", json!({ "title": "Original" })),
    )
    .await;
    assert!(create_result.success);
    let rule_id = create_result.data["id"].as_str().unwrap();

    execute_tool(
        &ctx,
        &make_call(
            "update_rule",
            json!({ "rule_id": rule_id, "title": "Changed" }),
        ),
    )
    .await;

    let undo_result = execute_tool(&ctx, &make_call("undo_last_action", json!({}))).await;
    assert!(undo_result.success, "data: {}", undo_result.data);
    assert_eq!(undo_result.data["undone"], "update_rule");

    let db_text: String = sqlx::query_scalar("SELECT text FROM rules WHERE id = $1")
        .bind(Uuid::parse_str(rule_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(db_text, "Original");
}

// ---------------------------------------------------------------------------
// undo_last_action — delete_rule → undo
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn undo_delete_rule_restores_rule(pool: PgPool) {
    let user_id = create_user(&pool, "undo-dr@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let create_result = execute_tool(
        &ctx,
        &make_call("create_rule", json!({ "title": "Keep me" })),
    )
    .await;
    assert!(create_result.success);
    let rule_id = create_result.data["id"].as_str().unwrap();

    execute_tool(
        &ctx,
        &make_call("delete_rule", json!({ "rule_id": rule_id })),
    )
    .await;

    let undo_result = execute_tool(&ctx, &make_call("undo_last_action", json!({}))).await;
    assert!(undo_result.success, "data: {}", undo_result.data);
    assert_eq!(undo_result.data["undone"], "delete_rule");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rules WHERE id = $1")
        .bind(Uuid::parse_str(rule_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// undo_last_action — nothing to undo
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn undo_nothing_to_undo_returns_error(pool: PgPool) {
    let user_id = create_user(&pool, "undo-empty@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let result = execute_tool(&ctx, &make_call("undo_last_action", json!({}))).await;

    assert!(!result.success);
    assert_eq!(result.data["error"], "nothing_to_undo");
    assert!(!result.mutated_routine);
}

// ---------------------------------------------------------------------------
// undo scoped to conversation_id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn undo_only_affects_same_conversation(pool: PgPool) {
    // Two conversations for the same routine.  An action in conv1 must not be
    // undone via undo in conv2.
    let user_id = create_user(&pool, "undo-conv@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv1 = create_conversation(&pool, user_id, routine_id).await;
    let conv2 = create_conversation(&pool, user_id, routine_id).await;

    let ctx1 = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv1,
    };
    let create_result = execute_tool(&ctx1, &make_call("create_block", default_block_args())).await;
    assert!(create_result.success);

    // undo from conv2 — nothing to undo for that conversation
    let ctx2 = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv2,
    };
    let undo_result = execute_tool(&ctx2, &make_call("undo_last_action", json!({}))).await;

    assert!(!undo_result.success);
    assert_eq!(undo_result.data["error"], "nothing_to_undo");

    // Block from conv1 must still exist
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE routine_id = $1")
        .bind(routine_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// undo stamps undone_at (does not lose audit trail)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn undo_stamps_undone_at_not_deletes_audit_row(pool: PgPool) {
    let user_id = create_user(&pool, "undo-stamp@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    execute_tool(&ctx, &make_call("create_block", default_block_args())).await;
    execute_tool(&ctx, &make_call("undo_last_action", json!({}))).await;

    // The audit row must still exist but have undone_at set.
    let undone_at: Option<String> = sqlx::query_scalar(
        "SELECT undone_at::text FROM routine_actions \
         WHERE conversation_id = $1",
    )
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(undone_at.is_some(), "undone_at must be stamped, got NULL");
}

// ---------------------------------------------------------------------------
// Double-undo: after undo, the action is marked done and second undo on same
// conversation finds nothing.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn double_undo_returns_nothing_to_undo(pool: PgPool) {
    let user_id = create_user(&pool, "undo-double@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    execute_tool(&ctx, &make_call("create_block", default_block_args())).await;

    let r1 = execute_tool(&ctx, &make_call("undo_last_action", json!({}))).await;
    assert!(r1.success, "first undo should succeed");

    let r2 = execute_tool(&ctx, &make_call("undo_last_action", json!({}))).await;
    assert!(!r2.success, "second undo should fail");
    assert_eq!(r2.data["error"], "nothing_to_undo");
}

// ---------------------------------------------------------------------------
// Unknown tool name — no panic
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn unknown_tool_returns_error_no_panic(pool: PgPool) {
    let user_id = create_user(&pool, "unknown-tool@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    let call = make_call("totally_fake_tool", json!({ "foo": "bar" }));
    let result = execute_tool(&ctx, &call).await;

    assert!(!result.success);
    assert!(!result.mutated_routine);
    assert!(
        result.data["error"]
            .as_str()
            .unwrap()
            .contains("unknown_tool")
    );
}

// ---------------------------------------------------------------------------
// Invalid args JSON — no panic
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn invalid_args_returns_error_no_panic(pool: PgPool) {
    let user_id = create_user(&pool, "invalid-args@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    // create_block requires day_of_week, start_time, title, type — pass nothing
    let call = make_call("create_block", json!({ "garbage": true }));
    let result = execute_tool(&ctx, &call).await;

    assert!(!result.success);
    assert!(!result.mutated_routine);
    assert!(
        result.data["error"]
            .as_str()
            .unwrap()
            .contains("invalid_args"),
        "expected invalid_args error, got: {}",
        result.data["error"]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_uuid_in_args_returns_error(pool: PgPool) {
    let user_id = create_user(&pool, "invalid-uuid@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };
    // Pass a non-UUID for block_id
    let call = make_call(
        "update_block",
        json!({ "block_id": "not-a-uuid", "title": "New" }),
    );
    let result = execute_tool(&ctx, &call).await;

    assert!(!result.success);
    assert!(!result.mutated_routine);
}

// ---------------------------------------------------------------------------
// Length validation — create_block title_too_long
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn create_block_title_too_long_returns_error(pool: PgPool) {
    let user_id = create_user(&pool, "cb-longtitle@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };

    // 1000-character title — well above the 200-char cap.
    let long_title = "a".repeat(1000);
    let call = make_call(
        "create_block",
        json!({
            "day_of_week": 1,
            "start_time": "09:00",
            "title": long_title,
            "type": "trabalho"
        }),
    );
    let result = execute_tool(&ctx, &call).await;

    assert!(!result.success, "expected failure, got: {}", result.data);
    assert!(!result.mutated_routine);
    assert_eq!(
        result.data["error"].as_str().unwrap(),
        "invalid_args: title_too_long"
    );

    // Nothing should have been written to the DB.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE routine_id = $1")
        .bind(routine_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "no block should be created when title is too long"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_note_too_long_returns_error(pool: PgPool) {
    let user_id = create_user(&pool, "cb-longnote@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };

    let long_note = "n".repeat(2001);
    let call = make_call(
        "create_block",
        json!({
            "day_of_week": 1,
            "start_time": "09:00",
            "title": "Valid Title",
            "type": "trabalho",
            "note": long_note
        }),
    );
    let result = execute_tool(&ctx, &call).await;

    assert!(!result.success, "expected failure, got: {}", result.data);
    assert_eq!(
        result.data["error"].as_str().unwrap(),
        "invalid_args: note_too_long"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn create_block_too_many_labels_returns_error(pool: PgPool) {
    let user_id = create_user(&pool, "cb-manylabels@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };

    // 21 labels — above the 20-label cap.
    let labels: Vec<String> = (0..21).map(|i| format!("label{i}")).collect();
    let call = make_call(
        "create_block",
        json!({
            "day_of_week": 1,
            "start_time": "09:00",
            "title": "Valid Title",
            "type": "trabalho",
            "label_names": labels
        }),
    );
    let result = execute_tool(&ctx, &call).await;

    assert!(!result.success, "expected failure, got: {}", result.data);
    assert_eq!(
        result.data["error"].as_str().unwrap(),
        "invalid_args: too_many_labels"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rule_title_too_long_returns_error(pool: PgPool) {
    let user_id = create_user(&pool, "cr-longtitle@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };

    let long_title = "r".repeat(201);
    let call = make_call("create_rule", json!({ "title": long_title }));
    let result = execute_tool(&ctx, &call).await;

    assert!(!result.success, "expected failure, got: {}", result.data);
    assert_eq!(
        result.data["error"].as_str().unwrap(),
        "invalid_args: title_too_long"
    );
}

// ---------------------------------------------------------------------------
// day_of_week range validation in list_blocks
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_blocks_day_out_of_range_returns_error(pool: PgPool) {
    let user_id = create_user(&pool, "lb-dayrange@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };

    for bad_day in [7, 8, 100, -1] {
        let result = execute_tool(
            &ctx,
            &make_call("list_blocks", json!({ "day_of_week": bad_day })),
        )
        .await;
        assert!(
            !result.success,
            "day={bad_day} should fail, got: {}",
            result.data
        );
        assert_eq!(
            result.data["error"].as_str().unwrap(),
            "invalid_args: day_of_week_out_of_range",
            "day={bad_day}"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn list_blocks_boundary_days_are_valid(pool: PgPool) {
    let user_id = create_user(&pool, "lb-boundary@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };

    // 0 and 6 are the boundary valid values.
    for day in [0, 6] {
        let result = execute_tool(
            &ctx,
            &make_call("list_blocks", json!({ "day_of_week": day })),
        )
        .await;
        assert!(
            result.success,
            "day={day} should succeed, got: {}",
            result.data
        );
    }
}

// ---------------------------------------------------------------------------
// db_error is not leaked — internal_error is returned instead
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn update_block_not_found_returns_not_found_not_db_error(pool: PgPool) {
    let user_id = create_user(&pool, "ub-notfound@test.com").await;
    let routine_id = create_routine(&pool, user_id).await;
    let conv_id = create_conversation(&pool, user_id, routine_id).await;

    let ctx = ToolContext {
        pool: &pool,
        user_id,
        routine_id,
        conversation_id: conv_id,
    };

    let nonexistent_id = Uuid::now_v7();
    let result = execute_tool(
        &ctx,
        &make_call(
            "update_block",
            json!({ "block_id": nonexistent_id, "title": "New" }),
        ),
    )
    .await;

    assert!(!result.success);
    // Must return not_found (not a raw DB error string).
    assert_eq!(result.data["error"].as_str().unwrap(), "not_found");
    assert!(
        !result.data["error"].as_str().unwrap().contains("db_error"),
        "db_error must not be leaked to client"
    );
}
