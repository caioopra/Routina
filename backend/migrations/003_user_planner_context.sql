-- Phase 2: Persistent user planner context for LLM system prompt injection
--
-- DOWN (manual revert):
--   ALTER TABLE users DROP COLUMN IF EXISTS planner_context;

-- planner_context is free-form text authored by the user (job, weekly intent,
-- long-term goals, etc.). It is nullable — users start with no context set.
-- The chat system prompt injects this field on every conversation turn so the
-- LLM has standing personal context without the user repeating themselves.
-- No index needed: reads are single-row lookups via the users PK.
ALTER TABLE users ADD COLUMN planner_context TEXT;
