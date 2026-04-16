-- Phase 3 Slice B: Audit log table
--
-- Provides a persistent, append-only record of admin-initiated actions across
-- the application (role promotions, user management, impersonation, etc.).
-- This is separate from routine_actions, which tracks LLM tool-driven routine
-- mutations.  audit_log captures administrative operations performed by admins
-- on behalf of (or affecting) any user.
--
-- actor_id      — the admin performing the action; SET NULL if the user is
--                 deleted so the log row is preserved.
-- actor_email   — denormalised snapshot of the admin's email at write time;
--                 survives user deletion without requiring a JOIN.
-- impersonating — when an admin acts on behalf of another user via the
--                 impersonation feature, this column holds that user's id.
-- action        — short identifier for the operation, e.g. 'promote_user',
--                 'demote_user', 'delete_user', 'impersonate_start'.
-- target_type   — the entity kind affected, e.g. 'user', 'routine', 'block'.
-- target_id     — the entity's id (stored as TEXT to accommodate both UUIDs
--                 and future non-UUID keys without a schema change).
-- payload       — arbitrary JSONB context; what changed, before/after, etc.
-- ip            — INET of the requester at write time (from X-Forwarded-For or
--                 the direct socket address).
-- user_agent    — raw User-Agent header, useful for forensic investigation.
-- created_at    — immutable write timestamp; no updated_at because rows are
--                 never modified after insert.
--
-- DOWN (manual revert):
--   DROP INDEX IF EXISTS audit_log_action_created;
--   DROP INDEX IF EXISTS audit_log_actor_created;
--   DROP TABLE IF EXISTS audit_log;

CREATE TABLE audit_log (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id      UUID REFERENCES users(id) ON DELETE SET NULL,
    actor_email   TEXT NOT NULL,
    impersonating UUID REFERENCES users(id) ON DELETE SET NULL,
    action        TEXT NOT NULL,
    target_type   TEXT,
    target_id     TEXT,
    payload       JSONB,
    ip            INET,
    user_agent    TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Composite index: all actions by a given actor, newest first.
-- Serves the admin "my activity" view and per-actor forensic queries.
CREATE INDEX audit_log_actor_created  ON audit_log (actor_id, created_at DESC);

-- Composite index: all occurrences of an action type, newest first.
-- Serves the admin dashboard action-type breakdown and alerting queries.
CREATE INDEX audit_log_action_created ON audit_log (action, created_at DESC);
