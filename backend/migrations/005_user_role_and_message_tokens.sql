-- Phase 3 Slice A: Admin role + LLM token usage columns
--
-- Adds a CHECK-constrained role column to users so the app can distinguish
-- regular users from admins without a separate table.  The partial index
-- covers the admin-list query (SELECT * FROM users WHERE role = 'admin')
-- efficiently because ≥99% of rows will carry role='user' and are excluded.
--
-- Adds three nullable columns to messages for LLM token tracking:
--   input_tokens  — prompt tokens billed on this turn
--   output_tokens — completion tokens billed on this turn
--   model         — exact model string returned by the provider (e.g. 'gemini-2.0-flash')
--
-- Only assistant rows produced by an LLM call will have non-NULL values.
-- User/tool/system rows and all pre-migration rows stay NULL (no backfill).
-- The token columns are consumed by Slice C rollup queries and the Phase 3
-- admin dashboard metrics endpoint.
--
-- DOWN (manual revert):
--   DROP INDEX IF EXISTS users_role_idx;
--   ALTER TABLE users DROP COLUMN IF EXISTS role;
--   ALTER TABLE messages DROP COLUMN IF EXISTS input_tokens;
--   ALTER TABLE messages DROP COLUMN IF EXISTS output_tokens;
--   ALTER TABLE messages DROP COLUMN IF EXISTS model;

-- Admin role ---------------------------------------------------------------

ALTER TABLE users
    ADD COLUMN role TEXT NOT NULL DEFAULT 'user'
    CHECK (role IN ('user', 'admin'));

-- Partial index: only admin rows are indexed; stays tiny in production.
CREATE INDEX users_role_idx ON users (role) WHERE role = 'admin';

-- LLM token usage persistence -----------------------------------------------

ALTER TABLE messages
    ADD COLUMN input_tokens  INT,
    ADD COLUMN output_tokens INT,
    ADD COLUMN model         TEXT;
