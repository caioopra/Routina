//! Provider-agnostic tool schema definitions and typed argument structs.
//!
//! `all_tool_schemas()` returns the full list of tools advertised to the LLM.
//! The companion `*Args` structs (all `serde::Deserialize`) let the backend
//! `ToolExecutor` deserialise `ToolCall::args` into a typed value per tool.

use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::ai::provider::ToolSchema;

// ---------------------------------------------------------------------------
// Typed argument structs
// ---------------------------------------------------------------------------

/// `list_blocks` — query blocks, optionally filtered by day.
#[derive(Debug, Clone, Deserialize)]
pub struct ListBlocksArgs {
    /// 0 = Sunday … 6 = Saturday. Omit to list all days.
    pub day_of_week: Option<i32>,
}

/// `create_block` — insert a new block into the locked routine.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateBlockArgs {
    /// 0 = Sunday … 6 = Saturday.
    pub day_of_week: i32,
    /// "HH:MM" format, e.g. "09:00".
    pub start_time: String,
    /// "HH:MM" format. Optional.
    pub end_time: Option<String>,
    pub title: String,
    /// One of: trabalho, mestrado, aula, exercicio, slides, viagem, livre.
    #[serde(rename = "type")]
    pub block_type: String,
    pub note: Option<String>,
    pub sort_order: Option<i32>,
    /// Label names (strings). The executor resolves these to label IDs.
    pub label_names: Option<Vec<String>>,
}

/// `update_block` — partial update of an existing block.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBlockArgs {
    pub block_id: Uuid,
    pub day_of_week: Option<i32>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub block_type: Option<String>,
    pub note: Option<String>,
    pub sort_order: Option<i32>,
    /// When present, replaces the full label set on this block.
    pub label_names: Option<Vec<String>>,
}

/// `delete_block` — remove a block by ID.
#[derive(Debug, Clone, Deserialize)]
pub struct DeleteBlockArgs {
    pub block_id: Uuid,
}

/// `list_rules` — no parameters; returns all rules for the locked routine.
#[derive(Debug, Clone, Deserialize)]
pub struct ListRulesArgs {}

/// `create_rule` — add a new planning rule.
///
/// Note: the `Rule` model in `models/rule.rs` uses `text` (not `title`/`description`).
/// We expose `title` and `description` to the LLM for a friendlier API and the
/// executor concatenates them: `"{title}: {description}"` or just `"{title}"`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateRuleArgs {
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i32>,
}

/// `update_rule` — partial update of an existing rule.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRuleArgs {
    pub rule_id: Uuid,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i32>,
}

/// `delete_rule` — remove a rule by ID.
#[derive(Debug, Clone, Deserialize)]
pub struct DeleteRuleArgs {
    pub rule_id: Uuid,
}

/// `list_labels` — no parameters; returns the user's labels.
#[derive(Debug, Clone, Deserialize)]
pub struct ListLabelsArgs {}

/// `undo_last_action` — reverses the most recent (non-undone) tool mutation.
#[derive(Debug, Clone, Deserialize)]
pub struct UndoLastActionArgs {}

// ---------------------------------------------------------------------------
// Schema builder helpers
// ---------------------------------------------------------------------------

fn string_prop(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn optional_string_prop(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn integer_prop(description: &str) -> Value {
    json!({ "type": "integer", "description": description })
}

fn uuid_prop(description: &str) -> Value {
    json!({ "type": "string", "format": "uuid", "description": description })
}

fn block_type_enum_prop() -> Value {
    json!({
        "type": "string",
        "enum": ["trabalho", "mestrado", "aula", "exercicio", "slides", "viagem", "livre"],
        "description": "Block type. Must be one of the listed values."
    })
}

fn label_names_prop() -> Value {
    json!({
        "type": "array",
        "items": { "type": "string" },
        "description": "Label names to attach. The executor resolves names to IDs automatically."
    })
}

// ---------------------------------------------------------------------------
// Tool schema definitions
// ---------------------------------------------------------------------------

fn list_blocks_schema() -> ToolSchema {
    ToolSchema {
        name: "list_blocks".to_string(),
        description:
            "Lista os blocos da rotina atual. Use antes de fazer atualizações para descobrir IDs reais."
                .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "day_of_week": integer_prop(
                    "Filter by day: 0=Sun, 1=Mon, 2=Tue, 3=Wed, 4=Thu, 5=Fri, 6=Sat. \
                     Omit to return all days."
                )
            },
            "required": []
        }),
    }
}

fn create_block_schema() -> ToolSchema {
    ToolSchema {
        name: "create_block".to_string(),
        description: "Cria um novo bloco na rotina atual.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "day_of_week": integer_prop("Day: 0=Sun, 1=Mon, 2=Tue, 3=Wed, 4=Thu, 5=Fri, 6=Sat."),
                "start_time": string_prop("Start time in HH:MM format, e.g. \"09:00\"."),
                "end_time": optional_string_prop("End time in HH:MM format. Optional."),
                "title": string_prop("Block title (max 255 chars)."),
                "type": block_type_enum_prop(),
                "note": optional_string_prop("Optional freeform notes for the block."),
                "sort_order": integer_prop("Display sort order (0-based). Optional."),
                "label_names": label_names_prop()
            },
            "required": ["day_of_week", "start_time", "title", "type"]
        }),
    }
}

fn update_block_schema() -> ToolSchema {
    ToolSchema {
        name: "update_block".to_string(),
        description: "Atualiza campos de um bloco existente. Todos os campos exceto block_id são opcionais (atualização parcial).".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "block_id": uuid_prop("UUID of the block to update (obtain from list_blocks)."),
                "day_of_week": integer_prop("New day: 0=Sun … 6=Sat."),
                "start_time": optional_string_prop("New start time in HH:MM format."),
                "end_time": optional_string_prop("New end time in HH:MM format. Send null to clear."),
                "title": optional_string_prop("New title."),
                "type": block_type_enum_prop(),
                "note": optional_string_prop("New note. Send null to clear."),
                "sort_order": integer_prop("New sort order."),
                "label_names": label_names_prop()
            },
            "required": ["block_id"]
        }),
    }
}

fn delete_block_schema() -> ToolSchema {
    ToolSchema {
        name: "delete_block".to_string(),
        description: "Remove um bloco da rotina. Esta ação pode ser desfeita com undo_last_action."
            .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "block_id": uuid_prop("UUID of the block to delete (obtain from list_blocks).")
            },
            "required": ["block_id"]
        }),
    }
}

fn list_rules_schema() -> ToolSchema {
    ToolSchema {
        name: "list_rules".to_string(),
        description: "Lista as regras de planejamento da rotina atual.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
    }
}

fn create_rule_schema() -> ToolSchema {
    ToolSchema {
        name: "create_rule".to_string(),
        description: "Cria uma nova regra de planejamento para a rotina atual.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "title": string_prop("Short rule title (required)."),
                "description": optional_string_prop("Optional longer explanation of the rule."),
                "priority": integer_prop("Display priority / sort order (lower = higher priority). Optional.")
            },
            "required": ["title"]
        }),
    }
}

fn update_rule_schema() -> ToolSchema {
    ToolSchema {
        name: "update_rule".to_string(),
        description: "Atualiza uma regra existente. Todos os campos exceto rule_id são opcionais."
            .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "rule_id": uuid_prop("UUID of the rule to update (obtain from list_rules)."),
                "title": optional_string_prop("New title."),
                "description": optional_string_prop("New description."),
                "priority": integer_prop("New priority / sort order.")
            },
            "required": ["rule_id"]
        }),
    }
}

fn delete_rule_schema() -> ToolSchema {
    ToolSchema {
        name: "delete_rule".to_string(),
        description:
            "Remove uma regra da rotina. Esta ação pode ser desfeita com undo_last_action."
                .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "rule_id": uuid_prop("UUID of the rule to delete (obtain from list_rules).")
            },
            "required": ["rule_id"]
        }),
    }
}

fn list_labels_schema() -> ToolSchema {
    ToolSchema {
        name: "list_labels".to_string(),
        description: "Lista as etiquetas (labels) disponíveis para o usuário.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
    }
}

fn undo_last_action_schema() -> ToolSchema {
    ToolSchema {
        name: "undo_last_action".to_string(),
        description: "Desfaz a última mutação desta conversa. Use quando o usuário pedir \"desfazer\" ou \"undo\". Não tente construir operações inversas manualmente.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns all tool schemas advertised to the LLM on every request.
pub fn all_tool_schemas() -> Vec<ToolSchema> {
    vec![
        list_blocks_schema(),
        create_block_schema(),
        update_block_schema(),
        delete_block_schema(),
        list_rules_schema(),
        create_rule_schema(),
        update_rule_schema(),
        delete_rule_schema(),
        list_labels_schema(),
        undo_last_action_schema(),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- all_tool_schemas ---------------------------------------------------

    const EXPECTED_TOOL_NAMES: &[&str] = &[
        "list_blocks",
        "create_block",
        "update_block",
        "delete_block",
        "list_rules",
        "create_rule",
        "update_rule",
        "delete_rule",
        "list_labels",
        "undo_last_action",
    ];

    #[test]
    fn all_tool_schemas_returns_all_expected_names() {
        let schemas = all_tool_schemas();
        assert!(
            !schemas.is_empty(),
            "all_tool_schemas() must not return an empty vec"
        );
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        for expected in EXPECTED_TOOL_NAMES {
            assert!(names.contains(expected), "Missing tool schema: {expected}");
        }
    }

    #[test]
    fn all_tool_schemas_count_matches_expected() {
        let schemas = all_tool_schemas();
        assert_eq!(
            schemas.len(),
            EXPECTED_TOOL_NAMES.len(),
            "Tool count mismatch: got {}, expected {}",
            schemas.len(),
            EXPECTED_TOOL_NAMES.len()
        );
    }

    #[test]
    fn each_schema_has_non_empty_description() {
        for schema in all_tool_schemas() {
            assert!(
                !schema.description.is_empty(),
                "Tool '{}' has empty description",
                schema.name
            );
        }
    }

    #[test]
    fn each_schema_parameters_is_object() {
        for schema in all_tool_schemas() {
            assert_eq!(
                schema.parameters["type"], "object",
                "Tool '{}' parameters must have type=object",
                schema.name
            );
        }
    }

    #[test]
    fn mutation_tools_have_ptbr_description() {
        let schemas = all_tool_schemas();
        // Spot-check a few key PT-BR phrases
        let create_block = schemas.iter().find(|s| s.name == "create_block").unwrap();
        assert!(
            create_block.description.contains("rotina"),
            "create_block description should mention 'rotina' in PT-BR"
        );
        let undo = schemas
            .iter()
            .find(|s| s.name == "undo_last_action")
            .unwrap();
        assert!(
            undo.description.contains("Desfaz"),
            "undo_last_action description should contain 'Desfaz'"
        );
    }

    // ---- ListBlocksArgs deserialization ------------------------------------

    #[test]
    fn list_blocks_args_no_filter() {
        let json = serde_json::json!({});
        let args: ListBlocksArgs = serde_json::from_value(json).unwrap();
        assert!(args.day_of_week.is_none());
    }

    #[test]
    fn list_blocks_args_with_day() {
        let json = serde_json::json!({ "day_of_week": 1 });
        let args: ListBlocksArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.day_of_week, Some(1));
    }

    // ---- CreateBlockArgs deserialization -----------------------------------

    #[test]
    fn create_block_args_minimal() {
        let json = serde_json::json!({
            "day_of_week": 2,
            "start_time": "09:00",
            "title": "Academia",
            "type": "exercicio"
        });
        let args: CreateBlockArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.day_of_week, 2);
        assert_eq!(args.start_time, "09:00");
        assert_eq!(args.title, "Academia");
        assert_eq!(args.block_type, "exercicio");
        assert!(args.end_time.is_none());
        assert!(args.note.is_none());
        assert!(args.label_names.is_none());
    }

    #[test]
    fn create_block_args_full() {
        let json = serde_json::json!({
            "day_of_week": 1,
            "start_time": "07:00",
            "end_time": "08:00",
            "title": "Trabalho Remoto",
            "type": "trabalho",
            "note": "Foco total, sem interrupcoes",
            "sort_order": 3,
            "label_names": ["importante", "urgente"]
        });
        let args: CreateBlockArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.day_of_week, 1);
        assert_eq!(args.end_time.as_deref(), Some("08:00"));
        assert_eq!(args.note.as_deref(), Some("Foco total, sem interrupcoes"));
        assert_eq!(args.sort_order, Some(3));
        let labels = args.label_names.unwrap();
        assert_eq!(labels, vec!["importante", "urgente"]);
    }

    // ---- UpdateBlockArgs deserialization -----------------------------------

    #[test]
    fn update_block_args_only_required() {
        let id = Uuid::now_v7();
        let json = serde_json::json!({ "block_id": id.to_string() });
        let args: UpdateBlockArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.block_id, id);
        assert!(args.title.is_none());
        assert!(args.start_time.is_none());
    }

    #[test]
    fn update_block_args_partial_fields() {
        let id = Uuid::now_v7();
        let json = serde_json::json!({
            "block_id": id.to_string(),
            "title": "Novo Título",
            "type": "mestrado"
        });
        let args: UpdateBlockArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.title.as_deref(), Some("Novo Título"));
        assert_eq!(args.block_type.as_deref(), Some("mestrado"));
        assert!(args.start_time.is_none());
    }

    // ---- DeleteBlockArgs deserialization -----------------------------------

    #[test]
    fn delete_block_args_roundtrip() {
        let id = Uuid::now_v7();
        let json = serde_json::json!({ "block_id": id.to_string() });
        let args: DeleteBlockArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.block_id, id);
    }

    // ---- ListRulesArgs deserialization -------------------------------------

    #[test]
    fn list_rules_args_empty_object() {
        let json = serde_json::json!({});
        let _args: ListRulesArgs = serde_json::from_value(json).unwrap();
    }

    // ---- CreateRuleArgs deserialization ------------------------------------

    #[test]
    fn create_rule_args_minimal() {
        let json = serde_json::json!({ "title": "Sem reuniões antes das 10h" });
        let args: CreateRuleArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.title, "Sem reuniões antes das 10h");
        assert!(args.description.is_none());
        assert!(args.priority.is_none());
    }

    #[test]
    fn create_rule_args_full() {
        let json = serde_json::json!({
            "title": "Blocos de foco",
            "description": "Pelo menos 2h de foco por dia",
            "priority": 1
        });
        let args: CreateRuleArgs = serde_json::from_value(json).unwrap();
        assert_eq!(
            args.description.as_deref(),
            Some("Pelo menos 2h de foco por dia")
        );
        assert_eq!(args.priority, Some(1));
    }

    // ---- UpdateRuleArgs deserialization ------------------------------------

    #[test]
    fn update_rule_args_only_id() {
        let id = Uuid::now_v7();
        let json = serde_json::json!({ "rule_id": id.to_string() });
        let args: UpdateRuleArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.rule_id, id);
        assert!(args.title.is_none());
    }

    // ---- DeleteRuleArgs deserialization ------------------------------------

    #[test]
    fn delete_rule_args_roundtrip() {
        let id = Uuid::now_v7();
        let json = serde_json::json!({ "rule_id": id.to_string() });
        let args: DeleteRuleArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.rule_id, id);
    }

    // ---- ListLabelsArgs deserialization ------------------------------------

    #[test]
    fn list_labels_args_empty_object() {
        let json = serde_json::json!({});
        let _args: ListLabelsArgs = serde_json::from_value(json).unwrap();
    }

    // ---- UndoLastActionArgs deserialization --------------------------------

    #[test]
    fn undo_last_action_args_empty_object() {
        let json = serde_json::json!({});
        let _args: UndoLastActionArgs = serde_json::from_value(json).unwrap();
    }

    // ---- Schema structure sanity checks ------------------------------------

    #[test]
    fn create_block_required_fields() {
        let schema = create_block_schema();
        let required = schema.parameters["required"].as_array().unwrap();
        let required_names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required_names.contains(&"day_of_week"));
        assert!(required_names.contains(&"start_time"));
        assert!(required_names.contains(&"title"));
        assert!(required_names.contains(&"type"));
    }

    #[test]
    fn update_block_only_block_id_required() {
        let schema = update_block_schema();
        let required = schema.parameters["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "block_id");
    }

    #[test]
    fn delete_block_only_block_id_required() {
        let schema = delete_block_schema();
        let required = schema.parameters["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "block_id");
    }

    #[test]
    fn create_rule_only_title_required() {
        let schema = create_rule_schema();
        let required = schema.parameters["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "title");
    }

    #[test]
    fn update_rule_only_rule_id_required() {
        let schema = update_rule_schema();
        let required = schema.parameters["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "rule_id");
    }

    #[test]
    fn delete_rule_only_rule_id_required() {
        let schema = delete_rule_schema();
        let required = schema.parameters["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "rule_id");
    }

    #[test]
    fn block_type_enum_contains_all_types() {
        let schema = create_block_schema();
        let type_prop = &schema.parameters["properties"]["type"];
        let enum_values = type_prop["enum"].as_array().unwrap();
        let enum_strings: Vec<&str> = enum_values.iter().map(|v| v.as_str().unwrap()).collect();
        for t in &[
            "trabalho",
            "mestrado",
            "aula",
            "exercicio",
            "slides",
            "viagem",
            "livre",
        ] {
            assert!(enum_strings.contains(t), "Missing block type '{t}' in enum");
        }
    }

    #[test]
    fn list_blocks_day_of_week_is_integer() {
        let schema = list_blocks_schema();
        let day_prop = &schema.parameters["properties"]["day_of_week"];
        assert_eq!(day_prop["type"], "integer");
    }
}
