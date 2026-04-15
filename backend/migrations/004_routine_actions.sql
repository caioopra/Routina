-- Phase 2 Slice C: Audit log for LLM tool-driven routine mutations
--
-- Every time the ToolExecutor mutates a routine (create/update/delete a block
-- or rule) it writes one row here. This enables the undo_last_action tool:
-- find the newest undone_at IS NULL row for the given conversation, reverse
-- the mutation, then stamp undone_at to preserve the audit trail.
--
-- DOWN (manual revert):
--   DROP INDEX IF EXISTS idx_routine_actions_user;
--   DROP INDEX IF EXISTS idx_routine_actions_routine_created;
--   DROP INDEX IF EXISTS idx_routine_actions_conversation_undone;
--   DROP TABLE IF EXISTS routine_actions;

CREATE TABLE routine_actions (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    routine_id       UUID        NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    -- Nullable so audit rows survive if the originating conversation is deleted.
    conversation_id  UUID        REFERENCES conversations(id) ON DELETE SET NULL,
    action_type      TEXT        NOT NULL CHECK (action_type IN (
                                     'create_block',
                                     'update_block',
                                     'delete_block',
                                     'create_rule',
                                     'update_rule',
                                     'delete_rule'
                                 )),
    -- The id of the block or rule that was mutated.
    target_id        UUID        NOT NULL,
    -- Full row snapshot BEFORE the mutation (NULL for create operations).
    payload_before   JSONB,
    -- Full row snapshot AFTER the mutation (NULL for delete operations).
    payload_after    JSONB,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- NULL until the action is reversed by undo_last_action.
    -- Kept as a timestamp (not a hard DELETE) to preserve the audit trail.
    undone_at        TIMESTAMPTZ
);

-- Primary undo query: find the most-recent non-undone action in a conversation.
-- The partial index on undone_at IS NULL keeps this small in the common case.
CREATE INDEX idx_routine_actions_conversation_undone
    ON routine_actions(conversation_id, created_at DESC)
    WHERE undone_at IS NULL;

-- Routine-scoped queries (admin / debug / future per-routine history view).
CREATE INDEX idx_routine_actions_routine_created
    ON routine_actions(routine_id, created_at DESC);

-- Ownership checks (authorization middleware).
CREATE INDEX idx_routine_actions_user
    ON routine_actions(user_id);
