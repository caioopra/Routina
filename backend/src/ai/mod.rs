//! AI / LLM integration layer.
//!
//! Provides:
//! - `provider` — trait definition and shared types (`Message`, `ToolSchema`,
//!   `ProviderEvent`, `FinishReason`, `ToolCall`).
//! - `gemini` — Gemini streaming REST implementation.
//! - `claude` — Claude (Anthropic Messages API) streaming implementation.
//! - `prompts` — system prompt builders (PT-BR).
//! - `error` — `ProviderError` type.
//! - `tools` — typed argument structs, tool schemas, and the `ToolExecutor`.
pub mod claude;
pub mod error;
pub mod gemini;
pub mod prompts;
pub mod provider;
pub mod tools;
