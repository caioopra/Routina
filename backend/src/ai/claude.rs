//! Claude (Anthropic Messages API) streaming provider implementation.
//!
//! Uses `POST https://api.anthropic.com/v1/messages` with `"stream": true`.
//! The response body is a sequence of server-sent events; each SSE line carries
//! one of the following event types:
//!
//! | SSE event type         | Description                                          |
//! |------------------------|------------------------------------------------------|
//! | `message_start`        | Envelope with usage metadata; ignored here.          |
//! | `content_block_start`  | Declares a new content block (text or tool_use).     |
//! | `content_block_delta`  | Incremental delta: `text_delta` or `input_json_delta`|
//! | `content_block_stop`   | Closes the current block.                            |
//! | `message_delta`        | Carries the final `stop_reason`.                     |
//! | `message_stop`         | Terminal event; stream ends.                         |
//! | `error`                | Error from the server (e.g. 529 overloaded).         |
//!
//! Reference (verified via context7 2026-04-15):
//!   https://docs.anthropic.com/en/api/messages-streaming
//!
//! Tool-use blocks arrive as:
//!   1. `content_block_start` with `{ type: "tool_use", id, name }`
//!   2. One or more `content_block_delta` with `{ type: "input_json_delta", partial_json }`
//!   3. `content_block_stop` — at this point we emit the complete `ToolCall`.
//!
//! We accumulate `partial_json` fragments per block index and assemble the
//! full JSON string before emitting `ProviderEvent::ToolCall`.

use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::ai::error::ProviderError;
use crate::ai::provider::{
    FinishReason, LlmProvider, Message, ProviderEvent, Role, TokenUsage, ToolCall, ToolSchema,
};

// ---------------------------------------------------------------------------
// Default model constant
// ---------------------------------------------------------------------------

/// Default Claude model.  Override with the `CLAUDE_MODEL` env var
/// (or set `Config::llm_claude_model` before constructing `ClaudeProvider`).
pub const DEFAULT_CLAUDE_MODEL: &str = "claude-sonnet-4-5";

/// Anthropic API version header value.
const ANTHROPIC_API_VERSION: &str = "2023-06-01";

// ---------------------------------------------------------------------------
// Anthropic SSE wire types
// ---------------------------------------------------------------------------

/// Usage counts reported inside `message_start` and `message_delta` events.
#[derive(Debug, Deserialize, Default)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

/// Top-level SSE event wrapper: `event: <type>\ndata: <json>`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
enum AnthropicEvent {
    MessageStart {
        message: MessageStartPayload,
    },
    ContentBlockStart {
        index: usize,
        content_block: ContentBlockMeta,
    },
    ContentBlockDelta {
        index: usize,
        delta: ContentDelta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: MessageDeltaPayload,
        #[serde(default)]
        usage: AnthropicUsage,
    },
    MessageStop,
    #[serde(rename = "error")]
    ApiError {
        error: AnthropicErrorPayload,
    },
    /// Catch-all for unknown event types (ping, etc.)
    #[serde(other)]
    Unknown,
}

/// Payload for `message_start` — carries initial input token count.
#[derive(Debug, Deserialize)]
struct MessageStartPayload {
    #[serde(default)]
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlockMeta {
    Text,
    ToolUse { id: String, name: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Deserialize)]
struct MessageDeltaPayload {
    stop_reason: Option<String>,
    #[allow(dead_code)]
    stop_sequence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorPayload {
    #[allow(dead_code)]
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

// ---------------------------------------------------------------------------
// Request builder
// ---------------------------------------------------------------------------

/// Build the Anthropic Messages API request body.
///
/// System messages are extracted and passed via the `system` field (a string).
/// Non-system messages become the `messages` array.
/// Tool definitions are placed in the `tools` array.
fn build_claude_request(messages: &[Message], tools: &[ToolSchema], model: &str) -> Value {
    // Collect all system messages into a single string (usually there is exactly one).
    let system_parts: Vec<&str> = messages
        .iter()
        .filter(|m| m.role == Role::System)
        .map(|m| m.content.as_str())
        .collect();

    // Build the contents array — Claude uses "user" / "assistant" roles.
    // Tool result messages become "user" messages with a `tool_result` content block.
    let contents: Vec<Value> = messages
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|m| {
            // Tool result messages.
            if m.role == Role::Tool {
                let tool_use_id = m.tool_call_id.as_deref().unwrap_or("unknown");
                let result_value: Value = serde_json::from_str(&m.content)
                    .unwrap_or_else(|_| json!({ "output": m.content }));
                return json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": result_value.to_string()
                    }]
                });
            }

            // Assistant messages that carry tool calls.
            if m.role == Role::Assistant
                && let Some(tool_calls) = &m.tool_calls
            {
                let content_blocks: Vec<Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.args
                        })
                    })
                    .collect();
                // Include text content if present alongside tool calls.
                let mut blocks = Vec::new();
                if !m.content.is_empty() {
                    blocks.push(json!({ "type": "text", "text": m.content }));
                }
                blocks.extend(content_blocks);
                return json!({ "role": "assistant", "content": blocks });
            }

            let role = match m.role {
                Role::User | Role::Tool => "user",
                Role::Assistant => "assistant",
                Role::System => unreachable!(),
            };

            json!({
                "role": role,
                "content": m.content
            })
        })
        .collect();

    let mut body = json!({
        "model": model,
        "max_tokens": 8192,
        "stream": true,
        "messages": contents
    });

    if !system_parts.is_empty() {
        body["system"] = json!(system_parts.join("\n\n"));
    }

    if !tools.is_empty() {
        let tool_defs: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters
                })
            })
            .collect();
        body["tools"] = json!(tool_defs);
    }

    body
}

// ---------------------------------------------------------------------------
// SSE parser
// ---------------------------------------------------------------------------

/// Internal state for a single in-progress tool-use block.
#[derive(Debug)]
struct PendingToolBlock {
    id: String,
    name: String,
    json_fragments: String,
}

/// Per-invocation streaming state used while processing SSE events.
#[derive(Default)]
struct StreamState {
    /// Map from SSE block index → pending tool block data.
    pending_tool_blocks: HashMap<usize, PendingToolBlock>,
    /// The accumulated `stop_reason` from `message_delta`.
    stop_reason: Option<String>,
    /// Accumulated token usage across `message_start` and `message_delta`.
    usage: TokenUsage,
    /// Set to `true` once at least one usage field was seen.
    has_usage: bool,
}

/// Parse a single SSE payload (the `data:` part, after stripping the prefix).
///
/// Returns `(events_to_emit, should_stop)`.  When `should_stop` is `true` the
/// caller must stop processing the stream.
fn parse_sse_data(data: &str, state: &mut StreamState) -> (Vec<ProviderEvent>, bool) {
    let event: AnthropicEvent = match serde_json::from_str(data) {
        Ok(e) => e,
        Err(err) => {
            tracing::warn!("Failed to parse Anthropic SSE data: {err}\ndata={data}");
            return (
                vec![ProviderEvent::Error(format!("SSE parse error: {err}"))],
                false,
            );
        }
    };

    match event {
        AnthropicEvent::MessageStart { message } => {
            // `message_start` carries the initial input token count.
            if let Some(n) = message.usage.input_tokens {
                state.usage.input_tokens = state.usage.input_tokens.saturating_add(n);
                state.has_usage = true;
            }
            (vec![], false)
        }

        AnthropicEvent::ContentBlockStart {
            index,
            content_block,
        } => {
            if let ContentBlockMeta::ToolUse { id, name } = content_block {
                state.pending_tool_blocks.insert(
                    index,
                    PendingToolBlock {
                        id,
                        name,
                        json_fragments: String::new(),
                    },
                );
            }
            (vec![], false)
        }

        AnthropicEvent::ContentBlockDelta { index, delta } => {
            match delta {
                ContentDelta::TextDelta { text } => {
                    if text.is_empty() {
                        (vec![], false)
                    } else {
                        (vec![ProviderEvent::Token(text)], false)
                    }
                }
                ContentDelta::InputJsonDelta { partial_json } => {
                    // Accumulate JSON fragments for the pending tool block.
                    if let Some(block) = state.pending_tool_blocks.get_mut(&index) {
                        block.json_fragments.push_str(&partial_json);
                    }
                    (vec![], false)
                }
            }
        }

        AnthropicEvent::ContentBlockStop { index } => {
            // If this block was a tool_use block, emit the completed ToolCall.
            if let Some(block) = state.pending_tool_blocks.remove(&index) {
                let args: Value = serde_json::from_str(&block.json_fragments)
                    .unwrap_or_else(|_| Value::Object(Default::default()));
                let event = ProviderEvent::ToolCall(ToolCall {
                    id: block.id,
                    name: block.name,
                    args,
                });
                (vec![event], false)
            } else {
                (vec![], false)
            }
        }

        AnthropicEvent::MessageDelta { delta, usage } => {
            state.stop_reason = delta.stop_reason;
            // `message_delta` carries the output token count for this response.
            if let Some(n) = usage.output_tokens {
                state.usage.output_tokens = state.usage.output_tokens.saturating_add(n);
                state.has_usage = true;
            }
            (vec![], false)
        }

        AnthropicEvent::MessageStop => {
            let finish_reason = match state.stop_reason.as_deref() {
                Some("end_turn") => FinishReason::Stop,
                Some("tool_use") => FinishReason::ToolCalls,
                Some("max_tokens") => FinishReason::Length,
                Some(other) => FinishReason::Other(other.to_string()),
                None => FinishReason::Stop,
            };
            let usage = if state.has_usage {
                Some(state.usage)
            } else {
                None
            };
            (
                vec![ProviderEvent::Done {
                    finish_reason,
                    usage,
                }],
                true,
            )
        }

        AnthropicEvent::ApiError { error } => (
            vec![ProviderEvent::Error(format!(
                "Anthropic API error: {}",
                error.message
            ))],
            true,
        ),

        // ping, and other unknown events are silently ignored.
        AnthropicEvent::Unknown => (vec![], false),
    }
}

// ---------------------------------------------------------------------------
// Provider struct
// ---------------------------------------------------------------------------

pub struct ClaudeProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl ClaudeProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    fn name(&self) -> &'static str {
        "claude"
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Pin<Box<dyn Stream<Item = ProviderEvent> + Send>>, ProviderError> {
        let body = build_claude_request(messages, tools, &self.model);

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let status_code = status.as_u16();
            let body_text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status_code,
                body: body_text,
            });
        }

        // Process the SSE stream line by line.  We need mutable state across
        // chunks for tool-call JSON accumulation, so we use `async_stream::stream!`.
        let byte_stream = response.bytes_stream();

        // We collect the byte chunks, then process each SSE frame.
        // The `state` is moved into the stream closure.
        let api_key = self.api_key.clone(); // not actually needed post-connect, but for completeness
        let _ = api_key; // suppress unused warning

        let event_stream = async_stream::stream! {
            let mut stream_state = StreamState::default();
            // Buffer for incomplete lines across byte chunks.
            let mut line_buf = String::new();
            // The SSE event type from the most recent `event:` header line.
            let mut current_event_type = String::new();

            let mut pinned = std::pin::pin!(byte_stream);

            while let Some(chunk_result) = pinned.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        yield ProviderEvent::Error(format!("Stream read error: {e}"));
                        return;
                    }
                };

                let text = String::from_utf8_lossy(&bytes);
                line_buf.push_str(&text);

                // Process complete lines (terminated by '\n').
                while let Some(pos) = line_buf.find('\n') {
                    let line = line_buf[..pos].trim_end_matches('\r').to_owned();
                    line_buf = line_buf[pos + 1..].to_owned();

                    if let Some(event_type) = line.strip_prefix("event:") {
                        current_event_type = event_type.trim().to_owned();
                    } else if let Some(data) = line.strip_prefix("data:") {
                        let data = data.trim();
                        if data.is_empty() || data == "[DONE]" {
                            continue;
                        }

                        let (events, should_stop) =
                            parse_sse_data(data, &mut stream_state);

                        for ev in events {
                            yield ev;
                        }

                        if should_stop {
                            return;
                        }
                    } else if line.is_empty() {
                        // Blank line = end of SSE frame; reset event type.
                        current_event_type.clear();
                    }
                    // Lines without a recognised prefix (e.g. `: ping`) are ignored.
                }
            }
        };

        Ok(Box::pin(event_stream) as Pin<Box<dyn Stream<Item = ProviderEvent> + Send>>)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper: run a sequence of SSE data lines through the parser and collect
    // the emitted events.
    // -----------------------------------------------------------------------

    fn run_sse_sequence(lines: &[&str]) -> Vec<ProviderEvent> {
        let mut state = StreamState::default();
        let mut events = Vec::new();
        for &line in lines {
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let (evs, stop) = parse_sse_data(data, &mut state);
                events.extend(evs);
                if stop {
                    break;
                }
            }
        }
        events
    }

    // -----------------------------------------------------------------------
    // Fixture: text-only turn
    // -----------------------------------------------------------------------

    /// Realistic SSE transcript for a plain text reply:
    /// "Olá! Vou ajudar."
    const TEXT_ONLY_SSE: &[&str] = &[
        r#"data: {"type":"message_start","message":{"id":"msg_01","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-5","stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":1}}}"#,
        r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Ol\u00e1! "}}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Vou ajudar."}}"#,
        r#"data: {"type":"content_block_stop","index":0}"#,
        r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":5}}"#,
        r#"data: {"type":"message_stop"}"#,
    ];

    #[test]
    fn text_only_turn_emits_tokens_then_done() {
        let events = run_sse_sequence(TEXT_ONLY_SSE);

        let tokens: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                ProviderEvent::Token(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(tokens, vec!["Olá! ", "Vou ajudar."]);

        let done = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::Done { .. }));
        assert!(done.is_some(), "expected Done event");
        assert!(matches!(
            done.unwrap(),
            ProviderEvent::Done {
                finish_reason: FinishReason::Stop,
                ..
            }
        ));
    }

    #[test]
    fn text_only_no_tool_calls() {
        let events = run_sse_sequence(TEXT_ONLY_SSE);
        let tool_calls: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ProviderEvent::ToolCall(_)))
            .collect();
        assert!(tool_calls.is_empty());
    }

    // -----------------------------------------------------------------------
    // Fixture: tool-use turn with multiple input_json_delta fragments
    // -----------------------------------------------------------------------

    /// SSE for a tool-use turn where `create_block` args arrive in 3 fragments.
    const TOOL_USE_SSE: &[&str] = &[
        r#"data: {"type":"message_start","message":{"id":"msg_02","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-5","stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":200,"output_tokens":1}}}"#,
        // Text block before the tool call (optional prefix text).
        r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Criando bloco..."}}"#,
        r#"data: {"type":"content_block_stop","index":0}"#,
        // Tool use block starts.
        r#"data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_01","name":"create_block"}}"#,
        // Three JSON fragments that together form the complete args object.
        r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"day_of_week\":"}}"#,
        r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"1,\"start_time\":\""}}"#,
        r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"09:00\",\"title\":\"Academia\",\"type\":\"exercicio\"}"}}"#,
        r#"data: {"type":"content_block_stop","index":1}"#,
        r#"data: {"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":40}}"#,
        r#"data: {"type":"message_stop"}"#,
    ];

    #[test]
    fn tool_use_turn_emits_text_then_tool_call_then_done() {
        let events = run_sse_sequence(TOOL_USE_SSE);

        // 1. Text token.
        let token = events.iter().find(|e| matches!(e, ProviderEvent::Token(_)));
        assert!(token.is_some(), "expected at least one Token event");
        assert!(matches!(token.unwrap(), ProviderEvent::Token(t) if t == "Criando bloco..."));

        // 2. ToolCall.
        let tool_call_event = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::ToolCall(_)))
            .expect("expected a ToolCall event");
        if let ProviderEvent::ToolCall(tc) = tool_call_event {
            assert_eq!(tc.id, "toolu_01");
            assert_eq!(tc.name, "create_block");
            assert_eq!(tc.args["day_of_week"], 1);
            assert_eq!(tc.args["start_time"], "09:00");
            assert_eq!(tc.args["title"], "Academia");
            assert_eq!(tc.args["type"], "exercicio");
        }

        // 3. Done with ToolCalls reason.
        let done = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::Done { .. }));
        assert!(done.is_some(), "expected Done event");
        assert!(matches!(
            done.unwrap(),
            ProviderEvent::Done {
                finish_reason: FinishReason::ToolCalls,
                ..
            }
        ));
    }

    #[test]
    fn tool_json_fragments_assembled_correctly() {
        // Only look at the ToolCall event to verify the JSON assembly.
        let events = run_sse_sequence(TOOL_USE_SSE);
        let tc_events: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                ProviderEvent::ToolCall(tc) => Some(tc),
                _ => None,
            })
            .collect();
        assert_eq!(tc_events.len(), 1);
        let tc = tc_events[0];
        // Verify the assembled object has all 4 expected keys.
        assert!(tc.args.is_object());
        assert!(tc.args.get("day_of_week").is_some());
        assert!(tc.args.get("start_time").is_some());
        assert!(tc.args.get("title").is_some());
        assert!(tc.args.get("type").is_some());
    }

    // -----------------------------------------------------------------------
    // Fixture: error event
    // -----------------------------------------------------------------------

    /// SSE stream that begins with an API-level error event.
    const ERROR_SSE: &[&str] =
        &[r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#];

    #[test]
    fn error_event_emits_provider_error() {
        let events = run_sse_sequence(ERROR_SSE);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ProviderEvent::Error(msg) if msg.contains("Overloaded")));
    }

    // -----------------------------------------------------------------------
    // Fixture: max_tokens stop reason
    // -----------------------------------------------------------------------

    const MAX_TOKENS_SSE: &[&str] = &[
        r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"truncated..."}}"#,
        r#"data: {"type":"content_block_stop","index":0}"#,
        r#"data: {"type":"message_delta","delta":{"stop_reason":"max_tokens","stop_sequence":null},"usage":{"output_tokens":8192}}"#,
        r#"data: {"type":"message_stop"}"#,
    ];

    #[test]
    fn max_tokens_stop_reason_maps_to_length() {
        let events = run_sse_sequence(MAX_TOKENS_SSE);
        let done = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::Done { .. }))
            .expect("expected Done event");
        assert!(matches!(
            done,
            ProviderEvent::Done {
                finish_reason: FinishReason::Length,
                ..
            }
        ));
    }

    // -----------------------------------------------------------------------
    // Fixture: unknown stop reason
    // -----------------------------------------------------------------------

    #[test]
    fn unknown_stop_reason_maps_to_other() {
        let lines = &[
            r#"data: {"type":"message_delta","delta":{"stop_reason":"safety","stop_sequence":null},"usage":{}}"#,
            r#"data: {"type":"message_stop"}"#,
        ];
        let events = run_sse_sequence(lines);
        let done = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::Done { .. }))
            .expect("expected Done event");
        assert!(matches!(
            done,
            ProviderEvent::Done {
                finish_reason: FinishReason::Other(s),
                ..
            } if s == "safety"
        ));
    }

    // -----------------------------------------------------------------------
    // Build request tests
    // -----------------------------------------------------------------------

    #[test]
    fn build_request_places_system_in_system_field() {
        let messages = vec![
            Message::system("Você é um assistente."),
            Message::user("Oi!"),
        ];
        let body = build_claude_request(&messages, &[], "claude-sonnet-4-5");
        assert!(body["system"].is_string());
        assert!(
            body["system"]
                .as_str()
                .unwrap()
                .contains("Você é um assistente.")
        );
        let contents = body["messages"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
    }

    #[test]
    fn build_request_no_system_omits_system_field() {
        let messages = vec![Message::user("Ola")];
        let body = build_claude_request(&messages, &[], "claude-sonnet-4-5");
        assert!(body.get("system").is_none());
    }

    #[test]
    fn build_request_includes_tools_as_input_schema() {
        let tools = vec![ToolSchema {
            name: "list_blocks".to_string(),
            description: "Lista blocos".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
        }];
        let messages = vec![Message::user("ok")];
        let body = build_claude_request(&messages, &tools, "claude-sonnet-4-5");
        let tool_defs = body["tools"].as_array().unwrap();
        assert_eq!(tool_defs.len(), 1);
        assert_eq!(tool_defs[0]["name"], "list_blocks");
        assert!(tool_defs[0]["input_schema"].is_object());
    }

    #[test]
    fn build_request_no_tools_omits_tools_field() {
        let messages = vec![Message::user("ok")];
        let body = build_claude_request(&messages, &[], "claude-sonnet-4-5");
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn build_request_tool_result_message_becomes_user_with_tool_result_block() {
        let messages = vec![
            Message::user("Crie um bloco"),
            Message::tool_result("toolu_01", r#"{"id":"abc"}"#),
        ];
        let body = build_claude_request(&messages, &[], "claude-sonnet-4-5");
        let contents = body["messages"].as_array().unwrap();
        // Second message should be a user message with a tool_result content block.
        let tool_msg = &contents[1];
        assert_eq!(tool_msg["role"], "user");
        let content_blocks = tool_msg["content"].as_array().unwrap();
        assert_eq!(content_blocks[0]["type"], "tool_result");
        assert_eq!(content_blocks[0]["tool_use_id"], "toolu_01");
    }

    #[test]
    fn build_request_stream_is_true() {
        let messages = vec![Message::user("ok")];
        let body = build_claude_request(&messages, &[], "claude-sonnet-4-5");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn build_request_model_field_set() {
        let messages = vec![Message::user("ok")];
        let body = build_claude_request(&messages, &[], "claude-sonnet-4-5");
        assert_eq!(body["model"], "claude-sonnet-4-5");
    }

    #[test]
    fn provider_name_is_claude() {
        let p = ClaudeProvider::new("key", DEFAULT_CLAUDE_MODEL);
        assert_eq!(p.name(), "claude");
    }

    // -----------------------------------------------------------------------
    // Empty text delta is filtered
    // -----------------------------------------------------------------------

    #[test]
    fn empty_text_delta_not_emitted() {
        let lines = &[
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":""}}"#,
            r#"data: {"type":"content_block_stop","index":0}"#,
            r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{}}"#,
            r#"data: {"type":"message_stop"}"#,
        ];
        let events = run_sse_sequence(lines);
        let tokens: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ProviderEvent::Token(_)))
            .collect();
        assert!(
            tokens.is_empty(),
            "empty text delta should not emit a Token event"
        );
    }

    // -----------------------------------------------------------------------
    // Multiple tool calls in one message
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_tool_calls_emitted_in_order() {
        let lines = &[
            // Block 0: first tool.
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"list_blocks"}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#,
            r#"data: {"type":"content_block_stop","index":0}"#,
            // Block 1: second tool.
            r#"data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t2","name":"create_block"}}"#,
            r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"day_of_week\":3,\"start_time\":\"10:00\",\"title\":\"Aula\",\"type\":\"aula\"}"}}"#,
            r#"data: {"type":"content_block_stop","index":1}"#,
            r#"data: {"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{}}"#,
            r#"data: {"type":"message_stop"}"#,
        ];
        let events = run_sse_sequence(lines);
        let tool_calls: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                ProviderEvent::ToolCall(tc) => Some(tc),
                _ => None,
            })
            .collect();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].id, "t1");
        assert_eq!(tool_calls[0].name, "list_blocks");
        assert_eq!(tool_calls[1].id, "t2");
        assert_eq!(tool_calls[1].name, "create_block");
    }

    // -----------------------------------------------------------------------
    // Invalid JSON in tool args degrades gracefully
    // -----------------------------------------------------------------------

    #[test]
    fn invalid_tool_json_produces_empty_args_object() {
        let lines = &[
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"delete_block"}}"#,
            // Intentionally broken JSON fragment.
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{not valid json"}}"#,
            r#"data: {"type":"content_block_stop","index":0}"#,
            r#"data: {"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{}}"#,
            r#"data: {"type":"message_stop"}"#,
        ];
        let events = run_sse_sequence(lines);
        let tc_event = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::ToolCall(_)))
            .expect("expected ToolCall even with broken JSON");
        if let ProviderEvent::ToolCall(tc) = tc_event {
            assert!(tc.args.is_object(), "args should fall back to empty object");
        }
    }

    // -----------------------------------------------------------------------
    // TokenUsage extraction from message_start + message_delta
    // -----------------------------------------------------------------------

    /// Full SSE sequence that carries usage in both message_start (input) and
    /// message_delta (output).  Matches the real Claude wire format.
    const TEXT_WITH_USAGE_SSE: &[&str] = &[
        r#"data: {"type":"message_start","message":{"id":"msg_u1","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-5","stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":250,"output_tokens":1}}}"#,
        r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Oi!"}}"#,
        r#"data: {"type":"content_block_stop","index":0}"#,
        r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":60}}"#,
        r#"data: {"type":"message_stop"}"#,
    ];

    #[test]
    fn usage_extracted_from_message_start_and_message_delta() {
        let events = run_sse_sequence(TEXT_WITH_USAGE_SSE);
        let done = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::Done { .. }))
            .expect("expected Done event");
        if let ProviderEvent::Done { usage, .. } = done {
            let u = usage.expect("expected Some(TokenUsage)");
            assert_eq!(u.input_tokens, 250, "input tokens from message_start");
            assert_eq!(u.output_tokens, 60, "output tokens from message_delta");
        }
    }

    #[test]
    fn usage_none_when_no_token_counts_in_stream() {
        // TEXT_ONLY_SSE has usage fields but they are "output_tokens":1 in
        // message_start and "output_tokens":5 in message_delta, so has_usage is
        // still set.  Use a sequence with no numeric counts at all.
        let lines = &[
            r#"data: {"type":"message_start","message":{"id":"msg_x","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-5","stop_reason":null,"stop_sequence":null,"usage":{}}}"#,
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ok"}}"#,
            r#"data: {"type":"content_block_stop","index":0}"#,
            r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{}}"#,
            r#"data: {"type":"message_stop"}"#,
        ];
        let events = run_sse_sequence(lines);
        let done = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::Done { .. }))
            .expect("expected Done event");
        if let ProviderEvent::Done { usage, .. } = done {
            assert!(
                usage.is_none(),
                "expected None when no token counts present"
            );
        }
    }
}
