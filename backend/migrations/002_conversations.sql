-- Phase 2: Chat/AI tables — conversations and messages
--
-- DOWN (manual revert):
--   DROP TABLE IF EXISTS messages;
--   DROP TABLE IF EXISTS conversations;

-- Conversations
-- Each row represents one chat session between a user and the AI.
-- routine_id is optional: when set, the conversation is scoped to that
-- specific routine (e.g. "help me edit Abril 2026").
CREATE TABLE conversations (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title       TEXT,
    routine_id  UUID        REFERENCES routines(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- List endpoint: GET /api/conversations ordered by most-recent-first per user
CREATE INDEX idx_conversations_user_created ON conversations(user_id, created_at DESC);

-- Optional filter: conversations that touch a specific routine
CREATE INDEX idx_conversations_routine ON conversations(routine_id);

-- Messages
-- Stores every turn in a conversation, including LLM tool-call turns.
--
-- Tool-call flow:
--   1. Assistant turn (role='assistant') with tool_calls JSONB:
--        [{ "id": "call_abc", "name": "create_block", "args": {...} }]
--   2. One tool-result turn per call (role='tool') with matching tool_call_id:
--        tool_call_id = "call_abc", content = "<result text>"
--
-- content is nullable so an assistant turn that consists solely of tool-calls
-- (no prose) does not need to store an empty string.
CREATE TABLE messages (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id  UUID        NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role             TEXT        NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content          TEXT,
    tool_calls       JSONB,
    tool_call_id     TEXT,
    provider         TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- History fetch: GET /api/conversations/:id/messages ordered chronologically
CREATE INDEX idx_messages_conversation_created ON messages(conversation_id, created_at ASC);
