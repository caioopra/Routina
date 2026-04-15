# Phase 2 Plan — AI Chat with Routine Tool-Calling

**Status:** Planning (2026-04-15)
**Baseline:** Phase 1 merged at commit `0b683cc` — JWT auth, routines/blocks/labels/rules CRUD, frontend routine detail page.

---

## 1. Goals

When Phase 2 ships, a user can:

- Open a chat panel on the routine detail page and have a multi-turn conversation with an LLM that has full read access to the active routine (blocks, labels, rules).
- Ask the LLM to create, update, or delete blocks and rules directly; the routine view updates in real time as the LLM's tool calls execute.
- Switch the active LLM provider between Gemini (primary) and Claude (secondary) from a settings toggle without losing conversation context.

---

## 2. Scope Cut

### In scope (Phase 2)

- Conversations and messages persisted in Postgres (the `conversations` and `messages` tables are already designed in `docs/schema.md` but the migration is not written yet).
- `LlmProvider` trait with Gemini streaming implementation (primary) and Claude streaming implementation (secondary).
- System prompt that injects the current routine's full state (blocks, labels, rules) at conversation start and refreshes it after each tool mutation.
- SSE streaming handler (`POST /api/chat/message`) with `token`, `tool_call`, `routine_updated`, `provider`, and `done` events as specified in `docs/api.md`.
- Tool-calling loop: LLM requests a tool → backend executes it against the DB → tool result fed back to LLM → LLM continues streaming.
- Tools: `list_blocks`, `create_block`, `update_block`, `delete_block`, `list_rules`, `create_rule`, `update_rule`, `delete_rule`, `list_labels`.
- Conversation list endpoint (`GET /api/conversations`) and message history endpoint (`GET /api/conversations/:id/messages`).
- Provider settings endpoint (`GET /api/settings/providers`, `PUT /api/settings/provider`) that writes to `users.preferences`.
- Frontend: chat panel component embedded in the routine detail page, `useSSE` hook, conversation list sidebar, provider toggle in a settings area.

### Out of scope (Phase 2, deferred to Phase 3 or later)

- Goals, events, subtasks — those tables exist but no AI tools will target them in Phase 2.
- Proactive suggestions or scheduled LLM runs (push model); Phase 2 is request/response only.
- Streaming cancellation (client-side abort mid-stream).
- Token usage tracking / cost accounting UI.
- Fine-tuning or system-prompt editor exposed to the user.
- Conversation branching or deletion from the UI.

---

## 3. Slices

### Slice A — Foundation (no HTTP, no UI)

**Owner:** `ai-prompt` (trait + providers + prompts), `database` (migration)

**Deliverables:**

- `backend/migrations/002_conversations.sql` — creates `conversations` and `messages` tables with indexes per `docs/schema.md`. Adds `llm_provider` key to the `users.preferences` JSONB column (no structural migration needed, JSONB is already there).
- `backend/src/ai/mod.rs` — public module declaration.
- `backend/src/ai/provider.rs` — `LlmProvider` async trait:
  ```
  pub trait LlmProvider: Send + Sync {
      fn name(&self) -> &str;
      async fn chat_stream(
          &self,
          messages: Vec<ChatMessage>,
          tools: Option<Vec<ToolSchema>>,
          system: &str,
      ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>, LlmError>;
  }
  ```
- `backend/src/ai/types.rs` — `ChatMessage`, `ToolSchema`, `ToolCall`, `StreamEvent` (Token, ToolUse, ToolResult, Done), `LlmError`.
- `backend/src/ai/gemini.rs` — `GeminiProvider`: HTTP streaming via `reqwest` against `generativelanguage.googleapis.com`; parses `generateContent` SSE chunks; maps to `StreamEvent`.
- `backend/src/ai/claude.rs` — `ClaudeProvider`: HTTP streaming against `api.anthropic.com/v1/messages`; parses Anthropic SSE format; maps to `StreamEvent`.
- `backend/src/ai/prompts/system.rs` — `build_system_prompt(routine: &RoutineDetail) -> String` using a Handlebars-style or plain-format string that serializes blocks, rules, and labels into the context window.
- `backend/src/ai/tools.rs` — `all_tools() -> Vec<ToolSchema>` defining the nine tool schemas for both providers' function-calling formats.
- Wire `ai` module into `backend/src/lib.rs`.

**Depends on:** Phase 1 (blocks/labels/rules models already in place).

**Tests:**
- Unit tests in `gemini.rs` and `claude.rs` that parse recorded SSE fixture bytes (no live API calls) and assert correct `StreamEvent` sequences.
- Unit test in `prompts/system.rs` verifying that a constructed `RoutineDetail` fixture produces a prompt containing expected block titles and rule text.
- Migration smoke test: `sqlx migrate run` against the test DB, assert `conversations` table exists.

---

### Slice B — Chat Endpoint + Conversation History (pure conversation, no tools)

**Owner:** `backend` (SSE handler, conversation persistence), `frontend` (chat panel, `useSSE`, conversation list)

**Deliverables:**

Backend:
- `backend/src/routes/chat.rs` — three handlers:
  - `POST /api/chat/message`: authenticates user, resolves or creates a conversation, builds message history from DB, calls `LlmProvider::chat_stream`, fans out SSE events to the client, persists user message and assistant reply to `messages` table. Tool calling disabled in this slice.
  - `GET /api/conversations`: returns list of user's conversations sorted by `updated_at DESC`.
  - `GET /api/conversations/:id/messages`: returns all messages for the conversation, ownership-checked.
- `AppState` extended with `Arc<dyn LlmProvider>` (provider selected from `Config::llm_default_provider`).
- Wire `chat::router()` into `routes/mod.rs` under `/api/chat` and `/api/conversations`.

Frontend:
- `frontend/src/api/chat.js` — `sendMessage(body)` (returns a raw `Response` for SSE), `listConversations()`, `getMessages(id)`.
- `frontend/src/hooks/useSSE.js` — hook that opens an SSE-over-fetch stream, parses `event:` / `data:` lines, exposes `{ tokens, isStreaming, error }` and fires callbacks on `routine_updated` and `done`.
- `frontend/src/stores/chatStore.js` — Zustand store: `conversations`, `activeConversationId`, `messages`, `isStreaming`, `sendMessage`, `loadHistory`.
- `frontend/src/components/chat/ChatPanel.jsx` — slide-in panel (right side on desktop, full-screen overlay on mobile) with message list, input field, send button, streaming token display, and scroll-to-bottom behavior.
- `frontend/src/components/chat/ConversationList.jsx` — collapsible sidebar list of past conversations.
- `frontend/src/pages/RoutineDetail.jsx` updated to include a "Chat" button that toggles `ChatPanel`.

**Depends on:** Slice A (trait + at least Gemini provider working).

**Tests:**
- Integration test `backend/tests/chat_tests.rs`: POST a message, assert SSE stream contains at least one `token` event and a terminal `done` event; assert message persisted in DB.
- Integration test: GET `/api/conversations` returns the just-created conversation.
- Frontend: `useSSE.test.js` with an MSW handler that streams chunked text; assert tokens accumulate correctly and `isStreaming` flips false on `done`.
- Frontend: `ChatPanel.test.jsx` — render with mocked store, type a message, submit, assert message appears in list.

---

### Slice C — Tool Execution + Claude Provider + Settings

**Owner:** `ai-prompt` (tool-use loop, Claude provider finalized), `backend` (tool executor, settings endpoint), `frontend` (routine refresh on `routine_updated`, settings toggle)

**Deliverables:**

Backend:
- `backend/src/ai/executor.rs` — `ToolExecutor` struct that holds a `PgPool` and `user_id`; implements `execute(tool_call: &ToolCall) -> Result<serde_json::Value, AppError>` dispatching to existing route-layer DB logic for each of the nine tools.
- Chat handler in `chat.rs` updated to run the tool-use loop: when the stream yields a `ToolUse` event, pause streaming, execute the tool, persist a `tool_use` message and a `tool_result` message, re-invoke `LlmProvider::chat_stream` with the extended history, resume SSE output.
- `routine_updated` SSE event emitted after each successful tool mutation (payload: full routine object from existing `GET /api/routines/:id` query).
- `backend/src/routes/settings.rs` — `GET /api/settings/providers` and `PUT /api/settings/provider`; writes `users.preferences->>'llm_provider'`. Wire into `routes/mod.rs` under `/api/settings`.
- Provider factory in `AppState` changed from a single static provider to a per-request resolver that reads `users.preferences->>'llm_provider'` and returns the matching `Arc<dyn LlmProvider>`.

Frontend:
- `chatStore.js` updated: on `routine_updated` event, call `routineStore.setRoutine(data.routine)` so the weekly grid updates without a page reload.
- `frontend/src/components/chat/ToolCallIndicator.jsx` — small inline badge shown while a tool is executing (e.g., "Creating block...").
- `frontend/src/pages/Settings.jsx` (new page) with provider toggle (Gemini / Claude radio) wired to `PUT /api/settings/provider`. Add Settings link to the Planner header.
- `App.jsx` updated with `/settings` route.

**Depends on:** Slice B (conversation persistence, SSE pipeline).

**Tests:**
- Integration test: POST a message whose response will contain a `create_block` tool call (use a stubbed provider that returns a fixed tool-call response); assert block appears in DB and `routine_updated` SSE event contains it.
- Unit test for `ToolExecutor::execute` for each of the nine tools using a test DB transaction.
- Integration test: `PUT /api/settings/provider` with `{ provider: "claude" }`, then `GET /api/settings/providers`, assert `active` is `"claude"`.
- Frontend: `Settings.test.jsx` — render page, click Claude toggle, assert API called with correct body.

---

### Slice D — Hardening, Error Handling, Rate Limiting, and Prompt Iteration

**Owner:** `backend` (error recovery, rate limiting), `ai-prompt` (prompt tuning), `testing` (coverage gaps, LLM mock strategy)

**Deliverables:**

Backend:
- Graceful SSE error events: if the LLM call fails mid-stream, emit `event: error` with `{ message }` and close the stream cleanly instead of dropping the connection.
- Retry logic in `GeminiProvider` and `ClaudeProvider` for transient HTTP 429 / 503 errors (exponential backoff, max 2 retries).
- Per-user rate limiting on `POST /api/chat/message`: configurable via `Config` (e.g., `LLM_RATE_LIMIT_RPM`, default 20). Use a token bucket in memory (acceptable for single-instance deployment on fly.io).
- Context window management: if conversation history exceeds a configurable token budget (`LLM_MAX_CONTEXT_TOKENS`, default 100 000 for Gemini 2.5 Flash), truncate oldest non-system messages before sending, preserving system prompt and the last N turns.
- `api.md` updated with the `event: error` format and settings endpoint request/response details.

AI-prompt:
- System prompt revised after Slices B–C testing: adjust tool descriptions, add few-shot examples for common edits (move a block, duplicate a day), tighten JSON schema constraints.

Testing:
- `backend/tests/` — a `MockLlmProvider` (deterministic, returns scripted responses) added to `tests/common/` for use across all chat integration tests; replaces any tests that made live API calls.
- Coverage report run; any route handler below 60% line coverage gets a targeted test.

**Depends on:** Slices A–C all passing.

**Tests:**
- Integration test: send 21 messages rapidly from the same user, assert the 21st returns 429.
- Unit test: context truncation function called with a history exceeding the token budget produces a truncated list that still starts with the system message.
- End-to-end smoke test (CI-safe, uses `MockLlmProvider`): full create-block flow from user message to `routine_updated` event to DB assertion.

---

## 4. Decisions (resolved 2026-04-15)

1. **Tool execution model — DIRECT WRITES.**
   LLM tool calls execute immediately against the DB; routine view updates in real time. An `undo_last_action` tool is added to the schema in Slice C so the user can reverse a mistaken mutation conversationally. No pending-change/confirmation UI.

2. **Conversation ↔ routine binding — LOCKED.**
   Each conversation is bound to exactly one `routine_id` at creation. The backend rejects any tool call that references a different routine. Starting a new routine = starting a new conversation.

3. **History retention — STORE ALL, WINDOW CONTEXT.**
   Messages are persisted indefinitely in the DB. Context sent to the LLM is a rolling window (last N turns, ~32K token budget) plus the system prompt. No user-facing knob in Phase 2.

4. **Persistent user context (new).**
   Users have a `planner_context` TEXT field on `users` — a self-authored "about me" (job, weekly intent, long-term goals) that is always injected into the system prompt alongside the routine state. Editable from the UI; the LLM can also propose edits via a tool in a later slice. This replaces the old idea of `users.preferences` carrying context — `preferences` stays for provider toggle etc.

5. **Default model — `gemini-3.1-flash-preview`.**
   Configurable via `GEMINI_MODEL` env var. Claude implementation follows in Slice C.

---

## 5. Risks

- **LLM flakiness in tests.** Any test that calls a live Gemini or Claude API will be slow, non-deterministic, and will fail in CI without credentials. Mitigation: `MockLlmProvider` introduced in Slice D's `tests/common/` should be backported to Slices B and C tests before those slices are merged. The `LlmProvider` trait makes substitution straightforward.

- **Streaming under the Vite proxy.** Vite's dev-server proxy (`/api` → `localhost:3000`) buffers responses by default, which breaks SSE. The `vite.config.js` proxy must set `changeOrigin: true` and the Axum handler must flush each SSE frame immediately (no buffering). This needs a manual smoke test in dev; it is not caught by Vitest.

- **Cost on Gemini free tier.** The free tier for Gemini 2.5 Flash allows 10 requests/minute and 250 requests/day per project. During active development with multiple devs or automated tests hitting the live API, this will be exhausted quickly. The `MockLlmProvider` guard and the per-user rate limiter in Slice D both help, but API key management (separate keys for dev vs. prod) should be set up before Slice B is tested end-to-end.

- **Tool-call JSON reliability.** Both Gemini and Claude can hallucinate tool arguments that fail schema validation (wrong field names, wrong types). The `ToolExecutor` must validate inputs before touching the DB and return structured error results back to the LLM rather than propagating `AppError` to the SSE stream. Failure to do this will produce silent corruption or cryptic client errors.
