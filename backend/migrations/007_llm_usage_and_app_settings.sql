-- Phase 3 Slice C: LLM usage daily rollup and runtime app settings
--
-- llm_usage_daily
--   Stores a daily rollup of LLM token consumption per user/provider/model.
--   The composite primary key (day, user_id, provider, model) allows
--   idempotent upsert increments: the application issues
--     INSERT ... ON CONFLICT (day, user_id, provider, model)
--     DO UPDATE SET input_tokens  = llm_usage_daily.input_tokens  + EXCLUDED.input_tokens,
--                   output_tokens = llm_usage_daily.output_tokens + EXCLUDED.output_tokens,
--                   request_count = llm_usage_daily.request_count + EXCLUDED.request_count,
--                   estimated_cost_usd = ...
--   after every LLM assistant turn.
--
-- app_settings
--   A flat key-value store for runtime configuration that admins can update
--   without a code deploy (default provider, model names, budget caps, etc.).
--   Seeded with sensible defaults using ON CONFLICT DO NOTHING so re-running
--   migrations does not overwrite admin-edited values.
--
-- DOWN (manual revert):
--   DROP TRIGGER IF EXISTS app_settings_set_updated_at ON app_settings;
--   DROP FUNCTION IF EXISTS set_updated_at;
--   DROP INDEX IF EXISTS llm_usage_daily_user_day;
--   DROP TABLE IF EXISTS llm_usage_daily;
--   DROP TABLE IF EXISTS app_settings;

-- Daily LLM usage rollup ---------------------------------------------------

CREATE TABLE llm_usage_daily (
    day                DATE        NOT NULL,
    user_id            UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider           TEXT        NOT NULL,
    model              TEXT        NOT NULL,
    input_tokens       BIGINT      NOT NULL DEFAULT 0,
    output_tokens      BIGINT      NOT NULL DEFAULT 0,
    request_count      INT         NOT NULL DEFAULT 0,
    estimated_cost_usd NUMERIC(10,6) NOT NULL DEFAULT 0,
    PRIMARY KEY (day, user_id, provider, model)
);

-- Secondary index for per-user day-range queries (e.g. "usage in the last 30
-- days for user X") and the admin metrics endpoint.
CREATE INDEX llm_usage_daily_user_day ON llm_usage_daily (user_id, day DESC);

-- Runtime application settings ---------------------------------------------

CREATE TABLE app_settings (
    key        TEXT PRIMARY KEY
                   CHECK (key IN (
                       'llm_default_provider',
                       'llm_gemini_model',
                       'llm_claude_model',
                       'budget_monthly_usd',
                       'budget_warn_pct',
                       'chat_enabled'
                   )),
    value      TEXT        NOT NULL
                   CHECK (char_length(value) <= 1024),
    updated_by UUID        REFERENCES users(id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Reusable updated_at trigger function (shared across tables)
CREATE OR REPLACE FUNCTION set_updated_at() RETURNS trigger AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER app_settings_set_updated_at
    BEFORE UPDATE ON app_settings
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Seed default values.  ON CONFLICT DO NOTHING makes this idempotent: if an
-- admin has already updated a setting its current value is preserved.
INSERT INTO app_settings (key, value) VALUES
    ('llm_default_provider',    'gemini'),
    ('llm_gemini_model',        'gemini-2.5-flash-preview-05-20'),
    ('llm_claude_model',        'claude-sonnet-4-20250514'),
    ('budget_monthly_usd',      '5.00'),
    ('budget_warn_pct',         '80'),
    ('chat_enabled',            'true')
ON CONFLICT (key) DO NOTHING;
