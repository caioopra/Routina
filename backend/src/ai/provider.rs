use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ai::error::ProviderError;

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID assigned by the provider (or generated client-side).
    pub id: String,
    /// Tool/function name to invoke.
    pub name: String,
    /// Arguments as a JSON object.
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Set on `Role::Tool` messages to reference the tool call being answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Present on `Role::Assistant` messages that include tool invocations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tool schema (provider-agnostic)
// ---------------------------------------------------------------------------

/// Provider-agnostic tool definition.  Each provider implementation converts
/// this into its own wire format (Gemini `FunctionDeclaration`, Claude `tools`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    /// JSON Schema object describing the function parameters.
    pub parameters: Value,
}

// ---------------------------------------------------------------------------
// Stream events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    Other(String),
}

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    /// A text token delta from the model.
    Token(String),
    /// The model wants to call a tool.
    ToolCall(ToolCall),
    /// The stream ended normally.
    Done { finish_reason: FinishReason },
    /// An error occurred mid-stream.
    Error(String),
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Short identifier used in SSE `provider` events (e.g. `"gemini"`).
    fn name(&self) -> &'static str;

    /// Start a streaming completion.  Returns a boxed `Stream` of `ProviderEvent`s.
    ///
    /// The return type is `Pin<Box<dyn Stream<...>>>` rather than `impl Stream`
    /// so that `LlmProvider` remains dyn-compatible (required for `Arc<dyn LlmProvider>`).
    ///
    /// System messages in `messages` (if any) are handled by each impl in the
    /// way its API expects (e.g. Gemini's `system_instruction` field).
    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Pin<Box<dyn Stream<Item = ProviderEvent> + Send>>, ProviderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_constructors_set_correct_roles() {
        let sys = Message::system("hello");
        assert_eq!(sys.role, Role::System);

        let user = Message::user("hello");
        assert_eq!(user.role, Role::User);

        let asst = Message::assistant("ok");
        assert_eq!(asst.role, Role::Assistant);

        let tool = Message::tool_result("call-1", "result");
        assert_eq!(tool.role, Role::Tool);
        assert_eq!(tool.tool_call_id.as_deref(), Some("call-1"));
    }

    #[test]
    fn finish_reason_equality() {
        assert_eq!(FinishReason::Stop, FinishReason::Stop);
        assert_ne!(FinishReason::Stop, FinishReason::ToolCalls);
        assert_eq!(
            FinishReason::Other("SAFETY".to_string()),
            FinishReason::Other("SAFETY".to_string())
        );
    }

    #[test]
    fn tool_schema_serializes() {
        let schema = ToolSchema {
            name: "create_block".to_string(),
            description: "Creates a new block in a routine.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "routine_id": { "type": "string" },
                    "title": { "type": "string" }
                },
                "required": ["routine_id", "title"]
            }),
        };
        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(json["name"], "create_block");
        assert!(json["parameters"]["properties"]["routine_id"].is_object());
    }
}
