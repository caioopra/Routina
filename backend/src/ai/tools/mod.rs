//! Tool definitions for the AI planning assistant.
//!
//! - `schemas` — provider-agnostic `ToolSchema` values + typed `*Args` structs.
//! - `executor` — runtime dispatcher that executes tool calls against the DB.
//!
//! The executor imports the `*Args` structs from `schemas` and deserialises
//! `ToolCall::args` with `serde_json::from_value`.

pub mod executor;
pub mod schemas;

// Re-export the most commonly used items at the `tools` module level.
pub use schemas::{
    CreateBlockArgs, CreateRuleArgs, DeleteBlockArgs, DeleteRuleArgs, ListBlocksArgs,
    ListLabelsArgs, ListRulesArgs, UndoLastActionArgs, UpdateBlockArgs, UpdateRuleArgs,
    all_tool_schemas,
};
