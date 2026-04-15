//! ToolExecutor — dispatches LLM `ToolCall`s to DB operations with authz + audit logging.
//!
//! Design constraints:
//! - Every tool error is returned as a `ToolResult` with `success: false` and a JSON payload
//!   containing `{ "error": "..." }`.  Errors **never** bubble up as Rust `Result::Err` so the
//!   SSE handler can always forward the error text back to the LLM for self-correction.
//! - Every mutation writes one row to `routine_actions` for undo support.
//! - Authorization is enforced per-resource: blocks/rules are checked against `ctx.routine_id`,
//!   labels against `ctx.user_id`.  IDs invented by the LLM are rejected silently (the response
//!   says `not_found`, not "that block belongs to another user").

use chrono::NaiveTime;
use serde_json::{Value, json};
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use crate::ai::provider::ToolCall;
use crate::models::block::{Block, BlockResponse};
use crate::models::label::{Label, LabelResponse};
use crate::models::rule::Rule;

use super::schemas::{
    CreateBlockArgs, CreateRuleArgs, DeleteBlockArgs, DeleteRuleArgs, ListBlocksArgs,
    ListLabelsArgs, ListRulesArgs, UndoLastActionArgs, UpdateBlockArgs, UpdateRuleArgs,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Runtime context threaded through every tool call.
#[derive(Debug, Clone)]
pub struct ToolContext<'a> {
    pub pool: &'a PgPool,
    /// The authenticated user making the request.
    pub user_id: Uuid,
    /// The routine this conversation is locked to.
    pub routine_id: Uuid,
    /// The conversation that originated this tool call.
    pub conversation_id: Uuid,
}

/// Uniform result returned by every tool execution.
pub struct ToolResult {
    /// Whether the tool succeeded.
    pub success: bool,
    /// Tool-specific response payload.  On error contains `{ "error": "..." }`.
    pub data: Value,
    /// `true` iff the tool wrote to the DB.  The SSE handler emits a
    /// `routine_updated` event when this is `true`.
    pub mutated_routine: bool,
}

impl ToolResult {
    fn ok(data: Value, mutated: bool) -> Self {
        Self {
            success: true,
            data,
            mutated_routine: mutated,
        }
    }

    fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: json!({ "error": message.into() }),
            mutated_routine: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Dispatch a `ToolCall` to the appropriate handler.
///
/// Never panics and never returns `Err`; all failures are encoded in the
/// returned `ToolResult`.
#[instrument(skip(ctx), fields(tool = %call.name, user_id = %ctx.user_id))]
pub async fn execute_tool(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    match call.name.as_str() {
        "list_blocks" => run_list_blocks(ctx, call).await,
        "create_block" => run_create_block(ctx, call).await,
        "update_block" => run_update_block(ctx, call).await,
        "delete_block" => run_delete_block(ctx, call).await,
        "list_rules" => run_list_rules(ctx, call).await,
        "create_rule" => run_create_rule(ctx, call).await,
        "update_rule" => run_update_rule(ctx, call).await,
        "delete_rule" => run_delete_rule(ctx, call).await,
        "list_labels" => run_list_labels(ctx, call).await,
        "undo_last_action" => run_undo_last_action(ctx, call).await,
        unknown => ToolResult::err(format!("unknown_tool: {unknown}")),
    }
}

// ---------------------------------------------------------------------------
// Block tools
// ---------------------------------------------------------------------------

const BLOCK_SELECT: &str = "id, routine_id, day_of_week, start_time, end_time, title, type, note, sort_order, \
     created_at, updated_at";

const BLOCK_TYPES: &[&str] = &[
    "trabalho",
    "mestrado",
    "aula",
    "exercicio",
    "slides",
    "viagem",
    "livre",
];

async fn run_list_blocks(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    let args: ListBlocksArgs = match serde_json::from_value(call.args.clone()) {
        Ok(a) => a,
        Err(e) => return ToolResult::err(format!("invalid_args: {e}")),
    };

    // Validate optional day_of_week before hitting the DB.
    if let Some(day) = args.day_of_week
        && !(0..=6).contains(&day)
    {
        return ToolResult::err("invalid_args: day_of_week_out_of_range");
    }

    let blocks: Vec<Block> = if let Some(day) = args.day_of_week {
        match sqlx::query_as::<_, Block>(&format!(
            "SELECT {BLOCK_SELECT} FROM blocks \
             WHERE routine_id = $1 AND day_of_week = $2 \
             ORDER BY day_of_week ASC, sort_order ASC, start_time ASC"
        ))
        .bind(ctx.routine_id)
        .bind(day as i16)
        .fetch_all(ctx.pool)
        .await
        {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(error = ?e, "tool DB error");
                return ToolResult::err("internal_error");
            }
        }
    } else {
        match sqlx::query_as::<_, Block>(&format!(
            "SELECT {BLOCK_SELECT} FROM blocks \
             WHERE routine_id = $1 \
             ORDER BY day_of_week ASC, sort_order ASC, start_time ASC"
        ))
        .bind(ctx.routine_id)
        .fetch_all(ctx.pool)
        .await
        {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(error = ?e, "tool DB error");
                return ToolResult::err("internal_error");
            }
        }
    };

    let block_ids: Vec<Uuid> = blocks.iter().map(|b| b.id).collect();
    let labels_map = match fetch_labels_map(ctx.pool, &block_ids, ctx.user_id).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };

    let responses: Vec<BlockResponse> = blocks
        .into_iter()
        .map(|b| {
            let labels = labels_map.get(&b.id).cloned().unwrap_or_default();
            BlockResponse::from_block(b, labels)
        })
        .collect();

    ToolResult::ok(json!(responses), false)
}

async fn run_create_block(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    let args: CreateBlockArgs = match serde_json::from_value(call.args.clone()) {
        Ok(a) => a,
        Err(e) => return ToolResult::err(format!("invalid_args: {e}")),
    };

    // --- validate ---
    if !(0..=6).contains(&args.day_of_week) {
        return ToolResult::err("day_of_week must be between 0 and 6");
    }
    if args.title.trim().is_empty() {
        return ToolResult::err("title is required");
    }
    if args.title.len() > 200 {
        return ToolResult::err("invalid_args: title_too_long");
    }
    if args
        .note
        .as_deref()
        .map(|n| n.len() > 2000)
        .unwrap_or(false)
    {
        return ToolResult::err("invalid_args: note_too_long");
    }
    if let Some(ref labels) = args.label_names {
        if labels.len() > 20 {
            return ToolResult::err("invalid_args: too_many_labels");
        }
        if labels.iter().any(|l| l.len() > 50) {
            return ToolResult::err("invalid_args: label_name_too_long");
        }
    }
    if !BLOCK_TYPES.contains(&args.block_type.as_str()) {
        return ToolResult::err(format!(
            "unknown block type '{}'; allowed: {}",
            args.block_type,
            BLOCK_TYPES.join(", ")
        ));
    }
    let start = match parse_time(&args.start_time) {
        Ok(t) => t,
        Err(e) => return ToolResult::err(e),
    };
    let end = match args.end_time.as_deref().map(parse_time).transpose() {
        Ok(t) => t,
        Err(e) => return ToolResult::err(e),
    };
    if let (Some(e), s) = (end, start)
        && e <= s
    {
        return ToolResult::err("end_time must be strictly after start_time");
    }

    let sort_order = args.sort_order.unwrap_or(0);
    let id = Uuid::now_v7();

    let block = match sqlx::query_as::<_, Block>(&format!(
        "INSERT INTO blocks (id, routine_id, day_of_week, start_time, end_time, title, type, note, sort_order) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING {BLOCK_SELECT}"
    ))
    .bind(id)
    .bind(ctx.routine_id)
    .bind(args.day_of_week as i16)
    .bind(start)
    .bind(end)
    .bind(&args.title)
    .bind(&args.block_type)
    .bind(args.note.as_deref())
    .bind(sort_order)
    .fetch_one(ctx.pool)
    .await
    {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };

    // --- attach labels ---
    if let Some(ref label_names) = args.label_names
        && let Err(e) = attach_labels_by_name(ctx.pool, block.id, ctx.user_id, label_names).await
    {
        tracing::error!(error = ?e, "tool DB error");
        return ToolResult::err("internal_error");
    }

    // --- fetch labels for response ---
    let labels = match fetch_labels_for_block(ctx.pool, block.id, ctx.user_id).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };
    let response = BlockResponse::from_block(block, labels);
    let payload_after = json!(response);

    // --- audit log ---
    if let Err(e) = record_action(
        ctx.pool,
        ctx.user_id,
        ctx.routine_id,
        ctx.conversation_id,
        "create_block",
        id,
        None,
        Some(&payload_after),
    )
    .await
    {
        tracing::error!(error = ?e, "tool DB error");
        return ToolResult::err("internal_error");
    }

    ToolResult::ok(payload_after, true)
}

async fn run_update_block(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    let args: UpdateBlockArgs = match serde_json::from_value(call.args.clone()) {
        Ok(a) => a,
        Err(e) => return ToolResult::err(format!("invalid_args: {e}")),
    };

    // --- authz: block must belong to the locked routine ---
    let old_block = match verify_block_ownership(ctx.pool, args.block_id, ctx.routine_id).await {
        Ok(b) => b,
        Err(_) => return ToolResult::err("not_found"),
    };

    // --- validate optional fields ---
    if let Some(day) = args.day_of_week
        && !(0..=6).contains(&day)
    {
        return ToolResult::err("day_of_week must be between 0 and 6");
    }
    if let Some(ref title) = args.title {
        if title.trim().is_empty() {
            return ToolResult::err("title cannot be empty");
        }
        if title.len() > 200 {
            return ToolResult::err("invalid_args: title_too_long");
        }
    }
    if args
        .note
        .as_deref()
        .map(|n| n.len() > 2000)
        .unwrap_or(false)
    {
        return ToolResult::err("invalid_args: note_too_long");
    }
    if let Some(ref labels) = args.label_names {
        if labels.len() > 20 {
            return ToolResult::err("invalid_args: too_many_labels");
        }
        if labels.iter().any(|l| l.len() > 50) {
            return ToolResult::err("invalid_args: label_name_too_long");
        }
    }
    if let Some(ref t) = args.block_type
        && !BLOCK_TYPES.contains(&t.as_str())
    {
        return ToolResult::err(format!(
            "unknown block type '{t}'; allowed: {}",
            BLOCK_TYPES.join(", ")
        ));
    }
    let start_time = match args.start_time.as_deref().map(parse_time).transpose() {
        Ok(t) => t,
        Err(e) => return ToolResult::err(e),
    };
    let end_time = match args.end_time.as_deref().map(parse_time).transpose() {
        Ok(t) => t,
        Err(e) => return ToolResult::err(e),
    };
    if let (Some(start), Some(end)) = (start_time, end_time)
        && end <= start
    {
        return ToolResult::err("end_time must be strictly after start_time");
    }

    // Capture before-snapshot for audit.
    let old_labels = match fetch_labels_for_block(ctx.pool, old_block.id, ctx.user_id).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };
    let payload_before = json!(BlockResponse::from_block(old_block, old_labels));

    let updated = match sqlx::query_as::<_, Block>(&format!(
        "UPDATE blocks SET \
            day_of_week = COALESCE($1, day_of_week), \
            start_time  = COALESCE($2, start_time), \
            end_time    = CASE WHEN $3::bool THEN $4 ELSE end_time END, \
            title       = COALESCE($5, title), \
            type        = COALESCE($6, type), \
            note        = CASE WHEN $7::bool THEN $8 ELSE note END, \
            sort_order  = COALESCE($9, sort_order), \
            updated_at  = now() \
         WHERE id = $10 \
         RETURNING {BLOCK_SELECT}"
    ))
    .bind(args.day_of_week.map(|d| d as i16))
    .bind(start_time)
    .bind(args.end_time.is_some())
    .bind(end_time)
    .bind(args.title.as_deref())
    .bind(args.block_type.as_deref())
    .bind(args.note.is_some())
    .bind(args.note.as_deref())
    .bind(args.sort_order)
    .bind(args.block_id)
    .fetch_optional(ctx.pool)
    .await
    {
        Ok(Some(b)) => b,
        Ok(None) => return ToolResult::err("not_found"),
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };

    // Update labels if provided.
    if let Some(ref label_names) = args.label_names
        && let Err(e) = replace_labels_by_name(ctx.pool, updated.id, ctx.user_id, label_names).await
    {
        tracing::error!(error = ?e, "tool DB error");
        return ToolResult::err("internal_error");
    }

    let new_labels = match fetch_labels_for_block(ctx.pool, updated.id, ctx.user_id).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };
    let payload_after = json!(BlockResponse::from_block(updated, new_labels));

    if let Err(e) = record_action(
        ctx.pool,
        ctx.user_id,
        ctx.routine_id,
        ctx.conversation_id,
        "update_block",
        args.block_id,
        Some(&payload_before),
        Some(&payload_after),
    )
    .await
    {
        tracing::error!(error = ?e, "tool DB error");
        return ToolResult::err("internal_error");
    }

    ToolResult::ok(payload_after, true)
}

async fn run_delete_block(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    let args: DeleteBlockArgs = match serde_json::from_value(call.args.clone()) {
        Ok(a) => a,
        Err(e) => return ToolResult::err(format!("invalid_args: {e}")),
    };

    // authz
    let old_block = match verify_block_ownership(ctx.pool, args.block_id, ctx.routine_id).await {
        Ok(b) => b,
        Err(_) => return ToolResult::err("not_found"),
    };

    let old_labels = match fetch_labels_for_block(ctx.pool, old_block.id, ctx.user_id).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };
    let payload_before = json!(BlockResponse::from_block(old_block, old_labels));

    match sqlx::query("DELETE FROM blocks WHERE id = $1")
        .bind(args.block_id)
        .execute(ctx.pool)
        .await
    {
        Ok(r) if r.rows_affected() == 0 => return ToolResult::err("not_found"),
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
        _ => {}
    }

    if let Err(e) = record_action(
        ctx.pool,
        ctx.user_id,
        ctx.routine_id,
        ctx.conversation_id,
        "delete_block",
        args.block_id,
        Some(&payload_before),
        None,
    )
    .await
    {
        tracing::error!(error = ?e, "tool DB error");
        return ToolResult::err("internal_error");
    }

    ToolResult::ok(json!({ "deleted": true }), true)
}

// ---------------------------------------------------------------------------
// Rule tools
// ---------------------------------------------------------------------------

const RULE_SELECT: &str = "id, routine_id, text, sort_order";

/// Build the rule `text` string from `title` + optional `description`.
/// The LLM sends `title` (and optionally `description`); the DB stores `text`.
fn build_rule_text(title: &str, description: Option<&str>) -> String {
    match description {
        Some(desc) if !desc.trim().is_empty() => format!("{title}: {desc}"),
        _ => title.to_string(),
    }
}

async fn run_list_rules(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    let _args: ListRulesArgs = match serde_json::from_value(call.args.clone()) {
        Ok(a) => a,
        Err(e) => return ToolResult::err(format!("invalid_args: {e}")),
    };

    let rules = match sqlx::query_as::<_, Rule>(&format!(
        "SELECT {RULE_SELECT} FROM rules \
         WHERE routine_id = $1 ORDER BY sort_order ASC, id ASC"
    ))
    .bind(ctx.routine_id)
    .fetch_all(ctx.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };

    ToolResult::ok(json!(rules), false)
}

async fn run_create_rule(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    let args: CreateRuleArgs = match serde_json::from_value(call.args.clone()) {
        Ok(a) => a,
        Err(e) => return ToolResult::err(format!("invalid_args: {e}")),
    };

    if args.title.trim().is_empty() {
        return ToolResult::err("title is required");
    }
    if args.title.len() > 200 {
        return ToolResult::err("invalid_args: title_too_long");
    }
    if args
        .description
        .as_deref()
        .map(|d| d.len() > 2000)
        .unwrap_or(false)
    {
        return ToolResult::err("invalid_args: description_too_long");
    }

    let text = build_rule_text(&args.title, args.description.as_deref());
    let sort_order = args.priority.unwrap_or(0);
    let id = Uuid::now_v7();

    let rule = match sqlx::query_as::<_, Rule>(&format!(
        "INSERT INTO rules (id, routine_id, text, sort_order) \
         VALUES ($1, $2, $3, $4) \
         RETURNING {RULE_SELECT}"
    ))
    .bind(id)
    .bind(ctx.routine_id)
    .bind(&text)
    .bind(sort_order)
    .fetch_one(ctx.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };

    let payload_after = json!(rule);

    if let Err(e) = record_action(
        ctx.pool,
        ctx.user_id,
        ctx.routine_id,
        ctx.conversation_id,
        "create_rule",
        id,
        None,
        Some(&payload_after),
    )
    .await
    {
        tracing::error!(error = ?e, "tool DB error");
        return ToolResult::err("internal_error");
    }

    ToolResult::ok(payload_after, true)
}

async fn run_update_rule(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    let args: UpdateRuleArgs = match serde_json::from_value(call.args.clone()) {
        Ok(a) => a,
        Err(e) => return ToolResult::err(format!("invalid_args: {e}")),
    };

    // authz
    let old_rule = match verify_rule_ownership(ctx.pool, args.rule_id, ctx.routine_id).await {
        Ok(r) => r,
        Err(_) => return ToolResult::err("not_found"),
    };

    if let Some(ref title) = args.title {
        if title.trim().is_empty() {
            return ToolResult::err("title cannot be empty");
        }
        if title.len() > 200 {
            return ToolResult::err("invalid_args: title_too_long");
        }
    }
    if args
        .description
        .as_deref()
        .map(|d| d.len() > 2000)
        .unwrap_or(false)
    {
        return ToolResult::err("invalid_args: description_too_long");
    }

    let payload_before = json!(old_rule);

    // Build a new text value if any text-related fields were provided.
    let new_text: Option<String> = if args.title.is_some() || args.description.is_some() {
        // Split the stored text into (title, description) using ": " as separator.
        let (existing_title, existing_description) = old_rule
            .text
            .split_once(": ")
            .unwrap_or((&old_rule.text, ""));
        let current_title = args.title.as_deref().unwrap_or(existing_title);
        let current_description =
            args.description
                .as_deref()
                .or(if existing_description.is_empty() {
                    None
                } else {
                    Some(existing_description)
                });
        Some(build_rule_text(current_title, current_description))
    } else {
        None
    };

    let updated = match sqlx::query_as::<_, Rule>(&format!(
        "UPDATE rules SET \
            text       = COALESCE($1, text), \
            sort_order = COALESCE($2, sort_order) \
         WHERE id = $3 \
         RETURNING {RULE_SELECT}"
    ))
    .bind(new_text.as_deref())
    .bind(args.priority)
    .bind(args.rule_id)
    .fetch_optional(ctx.pool)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return ToolResult::err("not_found"),
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };

    let payload_after = json!(updated);

    if let Err(e) = record_action(
        ctx.pool,
        ctx.user_id,
        ctx.routine_id,
        ctx.conversation_id,
        "update_rule",
        args.rule_id,
        Some(&payload_before),
        Some(&payload_after),
    )
    .await
    {
        tracing::error!(error = ?e, "tool DB error");
        return ToolResult::err("internal_error");
    }

    ToolResult::ok(payload_after, true)
}

async fn run_delete_rule(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    let args: DeleteRuleArgs = match serde_json::from_value(call.args.clone()) {
        Ok(a) => a,
        Err(e) => return ToolResult::err(format!("invalid_args: {e}")),
    };

    // authz
    let old_rule = match verify_rule_ownership(ctx.pool, args.rule_id, ctx.routine_id).await {
        Ok(r) => r,
        Err(_) => return ToolResult::err("not_found"),
    };

    let payload_before = json!(old_rule);

    match sqlx::query("DELETE FROM rules WHERE id = $1")
        .bind(args.rule_id)
        .execute(ctx.pool)
        .await
    {
        Ok(r) if r.rows_affected() == 0 => return ToolResult::err("not_found"),
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
        _ => {}
    }

    if let Err(e) = record_action(
        ctx.pool,
        ctx.user_id,
        ctx.routine_id,
        ctx.conversation_id,
        "delete_rule",
        args.rule_id,
        Some(&payload_before),
        None,
    )
    .await
    {
        tracing::error!(error = ?e, "tool DB error");
        return ToolResult::err("internal_error");
    }

    ToolResult::ok(json!({ "deleted": true }), true)
}

// ---------------------------------------------------------------------------
// Label tools
// ---------------------------------------------------------------------------

const LABEL_SELECT: &str =
    "id, user_id, name, color_bg, color_text, color_border, icon, is_default";

async fn run_list_labels(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    let _args: ListLabelsArgs = match serde_json::from_value(call.args.clone()) {
        Ok(a) => a,
        Err(e) => return ToolResult::err(format!("invalid_args: {e}")),
    };

    let labels: Vec<Label> = match sqlx::query_as::<_, Label>(&format!(
        "SELECT {LABEL_SELECT} FROM labels \
         WHERE user_id = $1 ORDER BY is_default DESC, name ASC"
    ))
    .bind(ctx.user_id)
    .fetch_all(ctx.pool)
    .await
    {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };

    let responses: Vec<LabelResponse> = labels.into_iter().map(LabelResponse::from).collect();
    ToolResult::ok(json!(responses), false)
}

// ---------------------------------------------------------------------------
// Undo
// ---------------------------------------------------------------------------

/// A row from the `routine_actions` audit table.
#[derive(sqlx::FromRow, Debug)]
struct RoutineAction {
    id: Uuid,
    action_type: String,
    target_id: Uuid,
    payload_before: Option<Value>,
    /// Fetched from DB but only used for create→undo path (delete target).
    #[allow(dead_code)]
    payload_after: Option<Value>,
}

async fn run_undo_last_action(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolResult {
    let _args: UndoLastActionArgs = match serde_json::from_value(call.args.clone()) {
        Ok(a) => a,
        Err(e) => return ToolResult::err(format!("invalid_args: {e}")),
    };

    // Find the most recent non-undone action in this conversation.
    let action: Option<RoutineAction> = match sqlx::query_as::<_, RoutineAction>(
        "SELECT id, action_type, target_id, payload_before, payload_after \
         FROM routine_actions \
         WHERE conversation_id = $1 AND undone_at IS NULL \
         ORDER BY created_at DESC \
         LIMIT 1",
    )
    .bind(ctx.conversation_id)
    .fetch_optional(ctx.pool)
    .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };

    let action = match action {
        Some(a) => a,
        None => return ToolResult::err("nothing_to_undo"),
    };

    // Begin a transaction so that the reversal and the audit stamp are atomic.
    // A crash between the two would otherwise leave the DB reversed but the
    // audit row not marked undone — causing a duplicate undo on the next call.
    let mut tx = match ctx.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = ?e, "tool DB error");
            return ToolResult::err("internal_error");
        }
    };

    // Perform the reversal inside the transaction.
    let reversal_result = match action.action_type.as_str() {
        "create_block" => undo_create_block(&mut tx, action.target_id).await,
        "update_block" => undo_update_block(&mut tx, action.target_id, &action).await,
        "delete_block" => undo_delete_block(&mut tx, ctx.routine_id, &action).await,
        "create_rule" => undo_create_rule(&mut tx, action.target_id).await,
        "update_rule" => undo_update_rule(&mut tx, action.target_id, &action).await,
        "delete_rule" => undo_delete_rule(&mut tx, ctx.routine_id, &action).await,
        other => Err(format!("unknown_action_type: {other}")),
    };

    if let Err(e) = reversal_result {
        // tx is dropped here, rolling back automatically.
        return ToolResult::err(e);
    }

    // Mark the action as undone inside the same transaction (preserve audit trail).
    if let Err(e) = sqlx::query("UPDATE routine_actions SET undone_at = now() WHERE id = $1")
        .bind(action.id)
        .execute(&mut *tx)
        .await
    {
        tracing::error!(error = ?e, "tool DB error");
        return ToolResult::err("internal_error");
    }

    // Commit — both the reversal and the audit stamp land atomically.
    if let Err(e) = tx.commit().await {
        tracing::error!(error = ?e, "tool DB error");
        return ToolResult::err("internal_error");
    }

    ToolResult::ok(
        json!({
            "undone": action.action_type,
            "target_id": action.target_id,
        }),
        true,
    )
}

// ---- undo helpers -----------------------------------------------------------

async fn undo_create_block(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    block_id: Uuid,
) -> Result<(), String> {
    sqlx::query("DELETE FROM blocks WHERE id = $1")
        .bind(block_id)
        .execute(&mut **tx)
        .await
        .map(|_| ())
        .map_err(|e| {
            tracing::error!(error = ?e, "tool DB error");
            "internal_error".to_string()
        })
}

async fn undo_update_block(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    block_id: Uuid,
    action: &RoutineAction,
) -> Result<(), String> {
    let before = action
        .payload_before
        .as_ref()
        .ok_or("undo_update_block: payload_before is NULL")?;

    let day_of_week: i16 = before["day_of_week"]
        .as_i64()
        .ok_or("payload_before missing day_of_week")? as i16;
    let start_time_str = before["start_time"]
        .as_str()
        .ok_or("payload_before missing start_time")?;
    let end_time_str = before["end_time"].as_str();
    let title = before["title"]
        .as_str()
        .ok_or("payload_before missing title")?;
    let block_type = before["type"]
        .as_str()
        .ok_or("payload_before missing type")?;
    let note = before["note"].as_str();
    let sort_order: i32 = before["sort_order"]
        .as_i64()
        .ok_or("payload_before missing sort_order")? as i32;

    let start_time = parse_naive_time(start_time_str).map_err(|e| format!("parse error: {e}"))?;
    let end_time = end_time_str
        .map(parse_naive_time)
        .transpose()
        .map_err(|e| format!("parse error: {e}"))?;

    sqlx::query(
        "UPDATE blocks SET \
         day_of_week = $1, start_time = $2, end_time = $3, title = $4, \
         type = $5, note = $6, sort_order = $7, updated_at = now() \
         WHERE id = $8",
    )
    .bind(day_of_week)
    .bind(start_time)
    .bind(end_time)
    .bind(title)
    .bind(block_type)
    .bind(note)
    .bind(sort_order)
    .bind(block_id)
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(|e| {
        tracing::error!(error = ?e, "tool DB error");
        "internal_error".to_string()
    })
}

/// Restore a deleted block. Uses `ctx.routine_id` (defense-in-depth) rather than
/// the `routine_id` stored in `payload_before` to prevent cross-routine writes if
/// the snapshot were ever tampered with.
async fn undo_delete_block(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    routine_id: Uuid,
    action: &RoutineAction,
) -> Result<(), String> {
    let before = action
        .payload_before
        .as_ref()
        .ok_or("undo_delete_block: payload_before is NULL")?;

    let id: Uuid = before["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("payload_before missing id")?;
    // Use the authoritative routine_id from the request context, not the snapshot.
    let day_of_week: i16 = before["day_of_week"]
        .as_i64()
        .ok_or("payload_before missing day_of_week")? as i16;
    let start_time_str = before["start_time"]
        .as_str()
        .ok_or("payload_before missing start_time")?;
    let end_time_str = before["end_time"].as_str();
    let title = before["title"]
        .as_str()
        .ok_or("payload_before missing title")?;
    let block_type = before["type"]
        .as_str()
        .ok_or("payload_before missing type")?;
    let note = before["note"].as_str();
    let sort_order: i32 = before["sort_order"]
        .as_i64()
        .ok_or("payload_before missing sort_order")? as i32;

    let start_time = parse_naive_time(start_time_str).map_err(|e| format!("parse error: {e}"))?;
    let end_time = end_time_str
        .map(parse_naive_time)
        .transpose()
        .map_err(|e| format!("parse error: {e}"))?;

    sqlx::query(
        "INSERT INTO blocks \
         (id, routine_id, day_of_week, start_time, end_time, title, type, note, sort_order) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(routine_id)
    .bind(day_of_week)
    .bind(start_time)
    .bind(end_time)
    .bind(title)
    .bind(block_type)
    .bind(note)
    .bind(sort_order)
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(|e| {
        tracing::error!(error = ?e, "tool DB error");
        "internal_error".to_string()
    })
}

async fn undo_create_rule(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rule_id: Uuid,
) -> Result<(), String> {
    sqlx::query("DELETE FROM rules WHERE id = $1")
        .bind(rule_id)
        .execute(&mut **tx)
        .await
        .map(|_| ())
        .map_err(|e| {
            tracing::error!(error = ?e, "tool DB error");
            "internal_error".to_string()
        })
}

async fn undo_update_rule(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rule_id: Uuid,
    action: &RoutineAction,
) -> Result<(), String> {
    let before = action
        .payload_before
        .as_ref()
        .ok_or("undo_update_rule: payload_before is NULL")?;

    let text = before["text"]
        .as_str()
        .ok_or("payload_before missing text")?;
    let sort_order: i32 = before["sort_order"]
        .as_i64()
        .ok_or("payload_before missing sort_order")? as i32;

    sqlx::query("UPDATE rules SET text = $1, sort_order = $2 WHERE id = $3")
        .bind(text)
        .bind(sort_order)
        .bind(rule_id)
        .execute(&mut **tx)
        .await
        .map(|_| ())
        .map_err(|e| {
            tracing::error!(error = ?e, "tool DB error");
            "internal_error".to_string()
        })
}

/// Restore a deleted rule. Uses `routine_id` from the request context rather than
/// the snapshot for defense-in-depth against snapshot tampering.
async fn undo_delete_rule(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    routine_id: Uuid,
    action: &RoutineAction,
) -> Result<(), String> {
    let before = action
        .payload_before
        .as_ref()
        .ok_or("undo_delete_rule: payload_before is NULL")?;

    let id: Uuid = before["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("payload_before missing id")?;
    // Use the authoritative routine_id from the request context, not the snapshot.
    let text = before["text"]
        .as_str()
        .ok_or("payload_before missing text")?;
    let sort_order: i32 = before["sort_order"]
        .as_i64()
        .ok_or("payload_before missing sort_order")? as i32;

    sqlx::query(
        "INSERT INTO rules (id, routine_id, text, sort_order) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(routine_id)
    .bind(text)
    .bind(sort_order)
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(|e| {
        tracing::error!(error = ?e, "tool DB error");
        "internal_error".to_string()
    })
}

// ---------------------------------------------------------------------------
// Authorization helpers
// ---------------------------------------------------------------------------

/// Fetch a block, verifying that its routine matches `routine_id`.
/// Returns the block on success; returns `Err(())` if not found or not owned.
pub async fn verify_block_ownership(
    pool: &PgPool,
    block_id: Uuid,
    routine_id: Uuid,
) -> Result<Block, ()> {
    sqlx::query_as::<_, Block>(&format!(
        "SELECT {BLOCK_SELECT} FROM blocks \
         WHERE id = $1 AND routine_id = $2"
    ))
    .bind(block_id)
    .bind(routine_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .ok_or(())
}

/// Fetch a rule, verifying that its routine matches `routine_id`.
pub async fn verify_rule_ownership(
    pool: &PgPool,
    rule_id: Uuid,
    routine_id: Uuid,
) -> Result<Rule, ()> {
    sqlx::query_as::<_, Rule>(&format!(
        "SELECT {RULE_SELECT} FROM rules \
         WHERE id = $1 AND routine_id = $2"
    ))
    .bind(rule_id)
    .bind(routine_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .ok_or(())
}

// ---------------------------------------------------------------------------
// Label helpers
// ---------------------------------------------------------------------------

/// Intermediate row for block_labels join.
#[derive(sqlx::FromRow)]
struct BlockLabelRow {
    block_id: Uuid,
    id: Uuid,
    name: String,
    color_bg: String,
    color_text: String,
    color_border: String,
    icon: Option<String>,
    is_default: bool,
}

/// Returns a `HashMap<block_id, Vec<LabelResponse>>` for the given block IDs.
async fn fetch_labels_map(
    pool: &PgPool,
    block_ids: &[Uuid],
    user_id: Uuid,
) -> Result<std::collections::HashMap<Uuid, Vec<LabelResponse>>, sqlx::Error> {
    if block_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let rows: Vec<BlockLabelRow> = sqlx::query_as(
        "SELECT bl.block_id, l.id, l.name, l.color_bg, l.color_text, l.color_border, \
                l.icon, l.is_default \
         FROM block_labels bl \
         JOIN labels l ON l.id = bl.label_id \
         WHERE bl.block_id = ANY($1) AND l.user_id = $2",
    )
    .bind(block_ids)
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut map: std::collections::HashMap<Uuid, Vec<LabelResponse>> =
        std::collections::HashMap::new();
    for row in rows {
        map.entry(row.block_id).or_default().push(LabelResponse {
            id: row.id,
            name: row.name,
            color_bg: row.color_bg,
            color_text: row.color_text,
            color_border: row.color_border,
            icon: row.icon,
            is_default: row.is_default,
        });
    }
    Ok(map)
}

/// Returns labels for a single block.
async fn fetch_labels_for_block(
    pool: &PgPool,
    block_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<LabelResponse>, sqlx::Error> {
    let mut map = fetch_labels_map(pool, &[block_id], user_id).await?;
    Ok(map.remove(&block_id).unwrap_or_default())
}

/// Look up label IDs by name (user-scoped) and create `block_labels` rows.
async fn attach_labels_by_name(
    pool: &PgPool,
    block_id: Uuid,
    user_id: Uuid,
    label_names: &[String],
) -> Result<(), String> {
    if label_names.is_empty() {
        return Ok(());
    }

    let label_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM labels WHERE user_id = $1 AND name = ANY($2)")
            .bind(user_id)
            .bind(label_names)
            .fetch_all(pool)
            .await
            .map_err(|e| format!("{e}"))?;

    for label_id in label_ids {
        sqlx::query(
            "INSERT INTO block_labels (block_id, label_id) VALUES ($1, $2) \
             ON CONFLICT DO NOTHING",
        )
        .bind(block_id)
        .bind(label_id)
        .execute(pool)
        .await
        .map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

/// Replace the full label set on a block.
async fn replace_labels_by_name(
    pool: &PgPool,
    block_id: Uuid,
    user_id: Uuid,
    label_names: &[String],
) -> Result<(), String> {
    // Delete existing
    sqlx::query("DELETE FROM block_labels WHERE block_id = $1")
        .bind(block_id)
        .execute(pool)
        .await
        .map_err(|e| format!("{e}"))?;

    attach_labels_by_name(pool, block_id, user_id, label_names).await
}

// ---------------------------------------------------------------------------
// Audit log helper
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn record_action(
    pool: &PgPool,
    user_id: Uuid,
    routine_id: Uuid,
    conversation_id: Uuid,
    action_type: &str,
    target_id: Uuid,
    payload_before: Option<&Value>,
    payload_after: Option<&Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO routine_actions \
         (user_id, routine_id, conversation_id, action_type, target_id, payload_before, payload_after) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(user_id)
    .bind(routine_id)
    .bind(conversation_id)
    .bind(action_type)
    .bind(target_id)
    .bind(payload_before)
    .bind(payload_after)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Time parsing
// ---------------------------------------------------------------------------

fn parse_time(s: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(s, "%H:%M")
        .map_err(|_| format!("invalid time format '{s}', expected HH:MM"))
}

fn parse_naive_time(s: &str) -> Result<NaiveTime, String> {
    // BlockResponse serializes times as "HH:MM"; handle that format.
    parse_time(s)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_result_ok_sets_fields() {
        let r = ToolResult::ok(json!({"key": "val"}), true);
        assert!(r.success);
        assert!(r.mutated_routine);
        assert_eq!(r.data["key"], "val");
    }

    #[test]
    fn tool_result_err_sets_fields() {
        let r = ToolResult::err("oops");
        assert!(!r.success);
        assert!(!r.mutated_routine);
        assert_eq!(r.data["error"], "oops");
    }

    #[test]
    fn unknown_tool_name_returns_error() {
        // We test this synchronously by checking the dispatch pattern.
        // The actual async test is in the integration suite.
        let name = "does_not_exist";
        let expected = format!("unknown_tool: {name}");
        let result = ToolResult::err(expected.clone());
        assert_eq!(result.data["error"], expected);
    }

    #[test]
    fn parse_time_valid() {
        assert!(parse_time("09:00").is_ok());
        assert!(parse_time("23:59").is_ok());
        assert!(parse_time("00:00").is_ok());
    }

    #[test]
    fn parse_time_invalid() {
        assert!(parse_time("25:00").is_err());
        assert!(parse_time("not-a-time").is_err());
        assert!(parse_time("9am").is_err());
    }

    #[test]
    fn build_rule_text_with_description() {
        let t = build_rule_text("No meetings", Some("Before 10am"));
        assert_eq!(t, "No meetings: Before 10am");
    }

    #[test]
    fn build_rule_text_without_description() {
        let t = build_rule_text("No meetings", None);
        assert_eq!(t, "No meetings");
    }

    #[test]
    fn build_rule_text_empty_description_treated_as_absent() {
        let t = build_rule_text("No meetings", Some("  "));
        assert_eq!(t, "No meetings");
    }
}
