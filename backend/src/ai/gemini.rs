//! Gemini streaming provider implementation.
//!
//! Uses the `generateContent` REST endpoint with `alt=sse` to receive
//! server-sent events.  Each SSE `data:` line is a JSON object following the
//! `GenerateContentResponse` schema.
//!
//! Reference:
//!   https://ai.google.dev/api/generate-content#v1beta.models.streamGenerateContent
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
// Gemini wire types (only the fields we actually use)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiContent {
    parts: Option<Vec<GeminiPart>>,
    #[allow(dead_code)]
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiPart {
    text: Option<String>,
    function_call: Option<GeminiFunctionCall>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiFunctionCall {
    name: String,
    args: Option<Value>,
}

// ---------------------------------------------------------------------------
// Request builders
// ---------------------------------------------------------------------------

/// Convert our generic `Message` slice into the Gemini `contents` array.
/// System messages are separated out and placed in `system_instruction`.
fn build_gemini_request(messages: &[Message], tools: &[ToolSchema]) -> Value {
    // Gemini uses a separate top-level `system_instruction` field.
    let system_parts: Vec<Value> = messages
        .iter()
        .filter(|m| m.role == Role::System)
        .map(|m| json!({ "text": m.content }))
        .collect();

    let contents: Vec<Value> = messages
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|m| {
            let role = match m.role {
                Role::User | Role::Tool => "user",
                Role::Assistant => "model",
                Role::System => unreachable!(),
            };

            // Tool result messages get wrapped as function_response parts.
            if m.role == Role::Tool {
                let tool_call_id = m.tool_call_id.as_deref().unwrap_or("unknown");
                let result_value: Value = serde_json::from_str(&m.content)
                    .unwrap_or_else(|_| json!({ "output": m.content }));
                return json!({
                    "role": role,
                    "parts": [{
                        "function_response": {
                            "name": tool_call_id,
                            "response": result_value
                        }
                    }]
                });
            }

            // Check if this assistant message has pending tool calls to emit.
            if m.role == Role::Assistant
                && let Some(tool_calls) = &m.tool_calls
            {
                let parts: Vec<Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "function_call": {
                                "name": tc.name,
                                "args": tc.args
                            }
                        })
                    })
                    .collect();
                return json!({ "role": "model", "parts": parts });
            }

            json!({
                "role": role,
                "parts": [{ "text": m.content }]
            })
        })
        .collect();

    let mut body = json!({ "contents": contents });

    if !system_parts.is_empty() {
        body["system_instruction"] = json!({ "parts": system_parts });
    }

    if !tools.is_empty() {
        let function_declarations: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                })
            })
            .collect();

        body["tools"] = json!([{ "function_declarations": function_declarations }]);
        body["tool_config"] = json!({
            "function_calling_config": { "mode": "AUTO" }
        });
    }

    body
}

/// Parse a single Gemini SSE `data:` JSON payload into zero or more
/// `ProviderEvent`s.
fn parse_sse_chunk(json_text: &str) -> Vec<ProviderEvent> {
    let response: GeminiResponse = match serde_json::from_str(json_text) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Failed to parse Gemini SSE chunk: {e}");
            return vec![ProviderEvent::Error(format!("Parse error: {e}"))];
        }
    };

    // Extract token usage when present (usually only in the last chunk).
    let usage = response.usage_metadata.as_ref().and_then(|u| {
        let input = u.prompt_token_count?;
        let output = u.candidates_token_count?;
        Some(TokenUsage {
            input_tokens: input,
            output_tokens: output,
        })
    });

    let mut events = Vec::new();

    let candidates = response.candidates.unwrap_or_default();
    for candidate in candidates {
        if let Some(content) = candidate.content {
            for part in content.parts.unwrap_or_default() {
                if let Some(text) = part.text
                    && !text.is_empty()
                {
                    events.push(ProviderEvent::Token(text));
                }
                if let Some(fc) = part.function_call {
                    // Gemini does not provide a per-call ID; we generate one.
                    let id = format!("gemini-call-{}", uuid::Uuid::now_v7());
                    events.push(ProviderEvent::ToolCall(ToolCall {
                        id,
                        name: fc.name,
                        args: fc.args.unwrap_or(Value::Object(Default::default())),
                    }));
                }
            }
        }

        if let Some(reason) = candidate.finish_reason {
            let finish = match reason.as_str() {
                "STOP" => FinishReason::Stop,
                "MAX_TOKENS" => FinishReason::Length,
                "TOOL_CALLS" | "FUNCTION_CALL" => FinishReason::ToolCalls,
                other => FinishReason::Other(other.to_string()),
            };
            events.push(ProviderEvent::Done {
                finish_reason: finish,
                usage,
            });
        }
    }

    events
}

// ---------------------------------------------------------------------------
// Provider struct
// ---------------------------------------------------------------------------

// User-requested default; falls back to GEMINI_MODEL env var if set.
// Target model: "gemini-3.1-flash-preview" (requested in phase2_plan.md §4).
// As of the last verified model list that exact name was not yet live; using
// "gemini-2.5-flash-preview-05-20" as the closest available preview.
// Swap this constant to "gemini-3.1-flash-preview" once it appears in the
// models endpoint.
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash-preview-05-20";

pub struct GeminiProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl GeminiProvider {
    /// Construct from explicit values (useful in tests / Slice B wiring).
    ///
    /// Pass `DEFAULT_GEMINI_MODEL` (or read from `Config::llm_gemini_model`)
    /// for the `model` argument in production.
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            client: reqwest::Client::new(),
        }
    }

    fn streaming_url(&self) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.model, self.api_key
        )
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &'static str {
        "gemini"
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Pin<Box<dyn Stream<Item = ProviderEvent> + Send>>, ProviderError> {
        let body = build_gemini_request(messages, tools);

        let response = self
            .client
            .post(self.streaming_url())
            .header("Content-Type", "application/json")
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

        // The response is an SSE stream.  Each line starting with `data:` is a
        // complete JSON object.  We use reqwest's byte_stream and parse manually
        // to avoid pulling in a heavy SSE crate.
        let byte_stream = response.bytes_stream();

        let event_stream = byte_stream
            // Accumulate bytes into lines and parse each data: line.
            .flat_map(|chunk_result| {
                let events: Vec<Result<ProviderEvent, ProviderError>> = match chunk_result {
                    Err(e) => vec![Err(ProviderError::Http(e))],
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).into_owned();
                        let mut found_events = Vec::new();
                        for line in text.lines() {
                            let line = line.trim();
                            if let Some(data) = line.strip_prefix("data:") {
                                let data = data.trim();
                                if data == "[DONE]" || data.is_empty() {
                                    continue;
                                }
                                for ev in parse_sse_chunk(data) {
                                    found_events.push(Ok(ev));
                                }
                            }
                        }
                        found_events
                    }
                };
                futures_util::stream::iter(events)
            })
            .filter_map(|result| async move {
                match result {
                    Ok(ev) => Some(ev),
                    Err(e) => Some(ProviderEvent::Error(e.to_string())),
                }
            });

        Ok(Box::pin(event_stream) as Pin<Box<dyn Stream<Item = ProviderEvent> + Send>>)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- parse_sse_chunk ----------------------------------------------------

    /// Sample JSON that Gemini returns for a plain text delta.
    const TEXT_CHUNK: &str = r#"{
        "candidates": [{
            "content": {
                "parts": [{"text": "Olá! Como posso ajudar?"}],
                "role": "model"
            }
        }]
    }"#;

    /// Sample with finish_reason STOP (no text).
    const STOP_CHUNK: &str = r#"{
        "candidates": [{
            "content": { "parts": [], "role": "model" },
            "finishReason": "STOP"
        }]
    }"#;

    /// Sample with a function_call part.
    const TOOL_CHUNK: &str = r#"{
        "candidates": [{
            "content": {
                "parts": [{
                    "functionCall": {
                        "name": "create_block",
                        "args": {
                            "routine_id": "abc-123",
                            "title": "Academia",
                            "day_of_week": 1,
                            "start_time": "07:00"
                        }
                    }
                }],
                "role": "model"
            },
            "finishReason": "TOOL_CALLS"
        }]
    }"#;

    /// Two text deltas and a STOP across three chunks, as they arrive in a real
    /// streaming transcript.
    const MULTI_PART_CHUNK: &str = r#"{
        "candidates": [{
            "content": {
                "parts": [
                    {"text": "Vou "},
                    {"text": "criar o bloco."}
                ],
                "role": "model"
            }
        }]
    }"#;

    #[test]
    fn parse_text_delta() {
        let events = parse_sse_chunk(TEXT_CHUNK);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ProviderEvent::Token(t) if t == "Olá! Como posso ajudar?"));
    }

    #[test]
    fn parse_stop_event() {
        let events = parse_sse_chunk(STOP_CHUNK);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            ProviderEvent::Done {
                finish_reason: FinishReason::Stop,
                ..
            }
        ));
    }

    #[test]
    fn parse_tool_call() {
        let events = parse_sse_chunk(TOOL_CHUNK);
        // Expect a ToolCall followed by a Done(ToolCalls)
        let tool_call = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::ToolCall(_)))
            .expect("expected a ToolCall event");

        if let ProviderEvent::ToolCall(tc) = tool_call {
            assert_eq!(tc.name, "create_block");
            assert_eq!(tc.args["routine_id"], "abc-123");
            assert_eq!(tc.args["title"], "Academia");
        }

        let done = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::Done { .. }))
            .expect("expected a Done event");

        assert!(matches!(
            done,
            ProviderEvent::Done {
                finish_reason: FinishReason::ToolCalls,
                ..
            }
        ));
    }

    #[test]
    fn parse_multi_part_returns_multiple_tokens() {
        let events = parse_sse_chunk(MULTI_PART_CHUNK);
        let tokens: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                ProviderEvent::Token(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tokens, vec!["Vou ", "criar o bloco."]);
    }

    #[test]
    fn parse_invalid_json_returns_error_event() {
        let events = parse_sse_chunk("not json {{{");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ProviderEvent::Error(_)));
    }

    #[test]
    fn parse_unknown_finish_reason() {
        let chunk = r#"{"candidates": [{"content": {"parts": [], "role": "model"}, "finishReason": "SAFETY"}]}"#;
        let events = parse_sse_chunk(chunk);
        assert!(
            matches!(&events[0], ProviderEvent::Done { finish_reason: FinishReason::Other(s), .. } if s == "SAFETY")
        );
    }

    // ---- build_gemini_request -----------------------------------------------

    #[test]
    fn request_separates_system_message() {
        let messages = vec![
            Message::system("Você é um assistente de planejamento."),
            Message::user("Crie um bloco."),
        ];
        let body = build_gemini_request(&messages, &[]);

        assert!(body["system_instruction"]["parts"][0]["text"].is_string());
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
    }

    #[test]
    fn request_includes_function_declarations() {
        let tools = vec![ToolSchema {
            name: "create_block".to_string(),
            description: "Creates a block.".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
        }];
        let messages = vec![Message::user("ok")];
        let body = build_gemini_request(&messages, &tools);

        let decls = &body["tools"][0]["function_declarations"];
        assert_eq!(decls[0]["name"], "create_block");
    }

    #[test]
    fn request_no_tools_omits_tools_field() {
        let messages = vec![Message::user("ok")];
        let body = build_gemini_request(&messages, &[]);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn request_tool_result_becomes_function_response() {
        let messages = vec![
            Message::user("Crie um bloco."),
            Message::tool_result("create_block", r#"{"id": "xyz"}"#),
        ];
        let body = build_gemini_request(&messages, &[]);
        let contents = body["contents"].as_array().unwrap();
        // tool result should be the second content item with role "user"
        let tool_content = &contents[1];
        assert_eq!(tool_content["role"], "user");
        assert!(tool_content["parts"][0]["function_response"].is_object());
    }

    #[test]
    fn streaming_url_contains_model_and_key() {
        let p = GeminiProvider::new("my-key", "gemini-2.0-flash");
        let url = p.streaming_url();
        assert!(url.contains("gemini-2.0-flash"));
        assert!(url.contains("my-key"));
        assert!(url.contains("alt=sse"));
    }

    // ---- TokenUsage extraction -----------------------------------------------

    /// Gemini chunk that carries `usageMetadata` alongside a STOP finish reason.
    const STOP_WITH_USAGE_CHUNK: &str = r#"{
        "candidates": [{
            "content": { "parts": [], "role": "model" },
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 120,
            "candidatesTokenCount": 45
        }
    }"#;

    #[test]
    fn parse_usage_metadata_into_done_event() {
        let events = parse_sse_chunk(STOP_WITH_USAGE_CHUNK);
        let done = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::Done { .. }))
            .expect("expected Done event");
        if let ProviderEvent::Done { usage, .. } = done {
            let u = usage.expect("expected Some(TokenUsage)");
            assert_eq!(u.input_tokens, 120);
            assert_eq!(u.output_tokens, 45);
        }
    }

    #[test]
    fn parse_chunk_without_usage_yields_none() {
        // STOP_CHUNK has no usageMetadata.
        let events = parse_sse_chunk(STOP_CHUNK);
        let done = events
            .iter()
            .find(|e| matches!(e, ProviderEvent::Done { .. }))
            .expect("expected Done event");
        if let ProviderEvent::Done { usage, .. } = done {
            assert!(usage.is_none(), "expected None when usageMetadata absent");
        }
    }

    #[test]
    fn token_usage_add_accumulates() {
        let mut total = TokenUsage::default();
        total.add(TokenUsage {
            input_tokens: 100,
            output_tokens: 30,
        });
        total.add(TokenUsage {
            input_tokens: 50,
            output_tokens: 20,
        });
        assert_eq!(total.input_tokens, 150);
        assert_eq!(total.output_tokens, 50);
    }
}
