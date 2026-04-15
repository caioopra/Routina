# Phase 3 Data Model Plan

> Peer briefs (`security.md`, `ai_governance.md`, `observability.md`) were absent at write time.
> Decisions below are independently reasoned. Where a conflict emerges after those briefs land,
> defer to the peer's recommendation and note it here before migration files are written.

---

## 1. `users.role` Column

Use `TEXT` with a `CHECK` constraint, not a Postgres ENUM type.

**Rationale:** ENUMs require `ALTER TYPE ... ADD VALUE` to extend (DDL-level change, cannot be rolled
back inside a transaction pre-PG12). TEXT + CHECK is trivially extended by updating the constraint.
For a solo-dev app with at most two or three roles this is the right trade-off.

```sql
-- Part of migration 005
ALTER TABLE users
    ADD COLUMN role TEXT NOT NULL DEFAULT 'user'
        CHECK (role IN ('user', 'admin'));

-- Backfill: all existing rows already receive DEFAULT 'user'; no explicit UPDATE needed.
-- If a specific user must be promoted to admin at deploy time, use a seed step:
--   UPDATE users SET role = 'admin' WHERE email = '<owner-email>';
-- This is intentional and outside the migration file itself (env-specific).
```

No index needed — admin dashboard queries will filter on `role = 'admin'` over a table that will
never exceed hundreds of rows in this application.

---

## 2. `audit_log` Table

Covers security-relevant events beyond routine mutations (which already have `routine_actions`).
Shape deferred to the security brief's event classes; the DDL below is the default if that brief
does not specify otherwise.

```sql
-- Part of migration 006
CREATE TABLE audit_log (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID        REFERENCES users(id) ON DELETE SET NULL,
    actor_role  TEXT        NOT NULL DEFAULT 'user',
    event_class TEXT        NOT NULL,  -- e.g. 'auth.login', 'auth.logout', 'settings.update',
                                       --      'user.role_change', 'rate_limit.override'
    event_data  JSONB       NOT NULL DEFAULT '{}',
    ip_address  TEXT,
    user_agent  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_log_user_created
    ON audit_log(user_id, created_at DESC);

CREATE INDEX idx_audit_log_event_class_created
    ON audit_log(event_class, created_at DESC);
```

**Retention:** No partitioning for now. This is a solo-dev app; rows grow slowly. Revisit with
`pg_partman` only if the table exceeds ~1M rows. A simple periodic `DELETE FROM audit_log WHERE
created_at < now() - INTERVAL '180 days'` run as a cron job (or a pg_cron schedule) is sufficient.
Note: `user_id` is `SET NULL` not `CASCADE` — deleting a user must not erase security history.

---

## 3. Token Usage Columns on `messages`

Add nullable columns. Only assistant rows produced by an LLM call will have values; user, system,
and tool rows leave them NULL.

```sql
-- Part of migration 005 (same file as role column, or 006 depending on sequencing preference)
ALTER TABLE messages
    ADD COLUMN input_tokens  INT,
    ADD COLUMN output_tokens INT;
```

**Index implications:** The rollup table (see §4) is built from these columns. No index on the
messages table itself is needed for rollup — the rollup job or trigger will aggregate periodically,
not scan messages live. A query like `SUM(output_tokens) GROUP BY provider` on messages is a full
table scan regardless; the rollup table handles it.

---

## 4. LLM Usage Rollup Table

**Grain:** daily × user × provider × model. This is the minimal grain that supports per-user quota
enforcement, per-provider cost breakdowns, and trend charts without re-scanning the full messages
table.

**Refresh strategy:** Scheduled job (application-level cron or pg_cron). Materialized views cannot
be refreshed incrementally without additional extensions. Triggers on every messages INSERT are
overly chatty and add latency to every LLM response write. A daily (or hourly) rollup job that does
an `INSERT ... ON CONFLICT DO UPDATE` is the simplest reliable approach.

```sql
-- Part of migration 007
CREATE TABLE llm_usage_daily (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id        UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    usage_date     DATE        NOT NULL,
    provider       TEXT        NOT NULL,  -- 'gemini', 'claude', etc.
    model          TEXT        NOT NULL,
    input_tokens   BIGINT      NOT NULL DEFAULT 0,
    output_tokens  BIGINT      NOT NULL DEFAULT 0,
    request_count  INT         NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (user_id, usage_date, provider, model)
);

CREATE INDEX idx_llm_usage_user_date
    ON llm_usage_daily(user_id, usage_date DESC);

CREATE INDEX idx_llm_usage_date_provider
    ON llm_usage_daily(usage_date DESC, provider);
```

Rollup upsert (run daily, parameterised on target date):

```sql
-- Upsert for a given :target_date
INSERT INTO llm_usage_daily (user_id, usage_date, provider, model,
                              input_tokens, output_tokens, request_count)
SELECT
    c.user_id,
    :target_date                    AS usage_date,
    m.provider,
    'unknown'                       AS model,   -- extend when model column added to messages
    COALESCE(SUM(m.input_tokens),  0),
    COALESCE(SUM(m.output_tokens), 0),
    COUNT(*)
FROM messages m
JOIN conversations c ON c.id = m.conversation_id
WHERE m.role = 'assistant'
  AND m.provider IS NOT NULL
  AND m.created_at >= :target_date
  AND m.created_at <  :target_date + INTERVAL '1 day'
GROUP BY c.user_id, m.provider
ON CONFLICT (user_id, usage_date, provider, model)
DO UPDATE SET
    input_tokens  = EXCLUDED.input_tokens,
    output_tokens = EXCLUDED.output_tokens,
    request_count = EXCLUDED.request_count,
    updated_at    = now();
```

If the ai_governance brief specifies a different grain or adds a `model` column to `messages`, update
the rollup query and add `model` to the `messages` ALTER in migration 005/006. The table DDL already
has a `model` column to accommodate this.

---

## 5. `app_settings` Table

Use a **key-value JSONB** approach, not typed columns. Typed columns require a new migration every
time a new config key is added; a key-value store lets the application define its own schema in code.

```sql
-- Part of migration 007
CREATE TABLE app_settings (
    key         TEXT        PRIMARY KEY,
    value       JSONB       NOT NULL,
    description TEXT,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID        REFERENCES users(id) ON DELETE SET NULL
);
```

Seed the known keys at migration time:

```sql
INSERT INTO app_settings (key, value, description) VALUES
    ('llm.default_provider',  '"gemini"',         'Primary LLM provider'),
    ('llm.fallback_provider', '"claude"',          'Fallback LLM provider'),
    ('llm.kill_switch',       'false',             'Set true to disable all LLM calls'),
    ('features.chat_enabled', 'true',              'Feature flag for chat UI')
ON CONFLICT (key) DO NOTHING;
```

The application reads this table at startup (and optionally re-reads per request for kill-switch
checks). No index needed beyond the primary key.

---

## 6. Per-User Rate-Limit Override

**Decision: yes, a table is needed.** A solo admin wanting to raise one user's daily cap is a
legitimate and recurring operation. Hard-coding it in `users.preferences` JSONB is possible but
invisible — a dedicated table is auditable and queryable.

```sql
-- Part of migration 008
CREATE TABLE user_rate_limits (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id             UUID        NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    daily_token_limit   BIGINT,     -- NULL = use global default from app_settings
    daily_request_limit INT,        -- NULL = use global default
    override_reason     TEXT,       -- admin note
    set_by              UUID        REFERENCES users(id) ON DELETE SET NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

The rate-limit check at request time does:
1. Look up `user_rate_limits` for the user (single row, PK lookup).
2. If NULL values, fall back to `app_settings` global defaults.
3. Compare against today's row in `llm_usage_daily`.

---

## 7. Admin Dashboard Denormalization

The rollup table (`llm_usage_daily`) is sufficient for cost/usage charts. No additional
denormalization is needed beyond these indexes:

- `idx_llm_usage_user_date` covers per-user daily token charts.
- `idx_llm_usage_date_provider` covers global provider cost breakdowns.
- `idx_audit_log_event_class_created` covers security event feeds.

The admin user list query (`SELECT id, email, name, role FROM users`) is a full table scan over a
tiny table — no index added.

---

## 8. Migration Sequencing

| File | Purpose | Notes |
|------|---------|-------|
| `005_user_role_and_message_tokens.sql` | `users.role` TEXT column + `messages.input_tokens / output_tokens` | Non-breaking; defaults cover all existing rows |
| `006_audit_log.sql` | `audit_log` table + indexes | Additive; no FK to break |
| `007_llm_usage_and_app_settings.sql` | `llm_usage_daily` table + `app_settings` table + seed rows | Additive; includes seed INSERT with ON CONFLICT |
| `008_user_rate_limits.sql` | `user_rate_limits` table | Additive; depends on `users` (005 must run first) |

**Rollout note for 005:** The `role` column gets `DEFAULT 'user'`. No explicit `UPDATE` is needed
in the migration. The first-user admin promotion is a manual step (`UPDATE users SET role = 'admin'
WHERE email = $ADMIN_EMAIL`) run after deploy, outside the migration file, because the target email
is environment-specific.

**No migration is breaking.** All four are purely additive (new columns with defaults, new tables).
They can run against a live database with zero downtime.

## Migration sequence

- `005_user_role_and_message_tokens.sql` — add `users.role` TEXT CHECK + `messages` token columns
- `006_audit_log.sql` — security event log table with indexes
- `007_llm_usage_and_app_settings.sql` — daily usage rollup table + key-value runtime config
- `008_user_rate_limits.sql` — per-user token/request cap overrides (admin-managed)
