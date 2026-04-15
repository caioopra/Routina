//! AI / LLM integration layer.
//!
//! Provides:
//! - `provider` — trait definition and shared types (`Message`, `ToolSchema`,
//!   `ProviderEvent`, `FinishReason`, `ToolCall`).
//! - `gemini` — Gemini streaming REST implementation.
//! - `prompts` — system prompt builders (PT-BR).
//! - `error` — `ProviderError` type.
pub mod error;
pub mod gemini;
pub mod prompts;
pub mod provider;
