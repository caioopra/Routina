-- Phase 3 Slice C: Per-user LLM rate limit overrides
--
-- user_rate_limits
--   Stores optional per-user overrides for daily LLM token and request limits.
--   When a row is absent for a user the application falls back to the global
--   limits defined in app_settings.  When present the per-user values take
--   precedence, allowing admins to grant higher limits (e.g. for testers) or
--   impose lower ones (e.g. for trial accounts).
--
--   Both limit columns are nullable: NULL means "no limit enforced for this
--   user" at that dimension.  This lets an admin set only a request cap without
--   constraining token volume (or vice versa).
--
--   set_by  — the admin who last wrote this row; SET NULL on admin deletion so
--             the override row is preserved even if the admin account is removed.
--   override_reason — free-text explanation for auditing purposes.
--
-- Relationship: one-to-one extension of users; PRIMARY KEY = user_id.
--
-- DOWN (manual revert):
--   DROP TABLE IF EXISTS user_rate_limits;

CREATE TABLE user_rate_limits (
    user_id              UUID    PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    daily_token_limit    BIGINT,
    daily_request_limit  INT,
    override_reason      TEXT,
    set_by               UUID    REFERENCES users(id) ON DELETE SET NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
