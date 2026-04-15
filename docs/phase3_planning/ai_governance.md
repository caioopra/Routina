# Phase 3 AI Governance Plan

## 1. Day-1 LLM Metrics

**MVP admin dashboard (ship in Phase 3):**

| Signal | Why it earns its place |
|---|---|
| Tokens in + tokens out | Already carried in `TokenUsage`; free to persist |
| USD cost estimate | Derived from tokens × pricing table; answers "am I over budget?" |
| Error rate per provider | Catches key misconfiguration and provider outages immediately |
| Tool-call count per turn | Direct measure of agentic complexity and cost amplification |

**Deferred to v2:**

- Per-provider latency p50/p95 — useful, but requires a timing column on messages or a separate spans table. Not worth the schema complexity until there is a second real user.
- Model availability tracking — provider health-check polling; premature before multi-user.

**Tradeoff:** skipping latency percentiles in Phase 3 means you cannot identify slow model calls from the dashboard. Mitigation: the existing `chat.round` tracing span already records wall time to stdout; grep is acceptable for a solo deployment.

---

## 2. Storage Model

**Decision: add token columns to `messages` and a daily rollup table refreshed hourly.**

### Migration additions to `messages`

```sql
ALTER TABLE messages
  ADD COLUMN input_tokens  INT,   -- NULL on user/tool rows
  ADD COLUMN output_tokens INT;   -- NULL on user/tool rows
```

Only `assistant` rows carry non-null values. The `chat.rs` insert for assistant messages already passes `provider`; extend that `INSERT` to bind `input_tokens` and `output_tokens` from `total_usage` split across rounds. Because `total_usage` is per-turn (not per-round), attribute the full turn's tokens to the final assistant message of the turn. This is a simplification but avoids a schema per-round token allocation problem.

### Rollup table

```sql
CREATE TABLE llm_usage_daily (
  day          DATE        NOT NULL,
  user_id      UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider     TEXT        NOT NULL,   -- 'gemini' | 'claude'
  model        TEXT        NOT NULL,   -- e.g. 'gemini-2.5-flash-preview'
  input_tokens  BIGINT     NOT NULL DEFAULT 0,
  output_tokens BIGINT     NOT NULL DEFAULT 0,
  msg_count     INT        NOT NULL DEFAULT 0,
  estimated_cost_usd NUMERIC(10,6) NOT NULL DEFAULT 0,
  PRIMARY KEY (day, user_id, provider, model)
);
CREATE INDEX idx_llm_usage_daily_user ON llm_usage_daily (user_id, day DESC);
```

Grain: **daily × user × provider × model**. Hourly grain wastes storage and is unnecessary when the admin view is a week/month table. Monthly roll-ups are a CTE over daily rows — no second table needed.

**Refresh strategy: scheduled INSERT ... ON CONFLICT DO UPDATE, run every hour via a Tokio background task** (or `pg_cron` if available on fly.io Postgres). Not a PostgreSQL materialized view because fly.io managed Postgres does not support `REFRESH MATERIALIZED VIEW CONCURRENTLY` on the free tier without a superuser, and because incremental upserts are simpler to reason about and test.

The refresh query:

```sql
INSERT INTO llm_usage_daily (day, user_id, provider, model,
                              input_tokens, output_tokens, msg_count, estimated_cost_usd)
SELECT
  date_trunc('day', m.created_at)::date AS day,
  c.user_id,
  m.provider,
  -- model stored on messages (see §5 below — add model column to messages)
  m.model,
  SUM(m.input_tokens),
  SUM(m.output_tokens),
  COUNT(*),
  SUM(cost_estimate(m.provider, m.model, m.input_tokens, m.output_tokens))
FROM messages m
JOIN conversations c ON c.id = m.conversation_id
WHERE m.role = 'assistant'
  AND m.input_tokens IS NOT NULL
GROUP BY 1, 2, 3, 4
ON CONFLICT (day, user_id, provider, model)
DO UPDATE SET
  input_tokens  = EXCLUDED.input_tokens,
  output_tokens = EXCLUDED.output_tokens,
  msg_count     = EXCLUDED.msg_count,
  estimated_cost_usd = EXCLUDED.estimated_cost_usd;
```

`cost_estimate` is a thin Postgres function that multiplies by the pricing constants (or reads from a `llm_pricing` table — see §3).

**Tradeoff:** an hourly background task adds one Tokio `spawn` to `main.rs` and a `SELECT` + `INSERT` every hour. At solo-dev scale this is negligible. The downside is up-to-1-hour staleness on the admin dashboard; that is acceptable.

---

## 3. Cost Estimation

**Decision: hard-coded `static` pricing map in `backend/src/ai/pricing.rs`, not a DB table.**

```rust
// backend/src/ai/pricing.rs
pub struct ModelPrice {
    pub input_per_1m:  f64,   // USD per million input tokens
    pub output_per_1m: f64,
}

pub fn price_for(provider: &str, model: &str) -> Option<ModelPrice> {
    match (provider, model) {
        ("gemini", m) if m.starts_with("gemini-2.5-flash") =>
            Some(ModelPrice { input_per_1m: 0.15, output_per_1m: 0.60 }),
        ("claude", m) if m.starts_with("claude-sonnet-4") =>
            Some(ModelPrice { input_per_1m: 3.00, output_per_1m: 15.00 }),
        _ => None,
    }
}

pub fn estimate_cost_usd(provider: &str, model: &str, input: u32, output: u32) -> f64 {
    price_for(provider, model).map_or(0.0, |p| {
        (input as f64 / 1_000_000.0) * p.input_per_1m
            + (output as f64 / 1_000_000.0) * p.output_per_1m
    })
}
```

**Where it lives:** `backend/src/ai/pricing.rs`. Called from both the rollup job and any in-request cost check.

**Rationale:** an admin-editable DB table is overkill when there is one admin (you) and model pricing changes at most a few times a year. A code change + redeploy is the right ceremony for a pricing update — it produces a Git history entry, is reviewable, and avoids a migration + admin UI just to change two floats. An external config file (`pricing.toml`) would require parsing logic with no benefit over a `match` in Rust.

**Tradeoff:** a new model whose name does not match the `starts_with` guard silently returns `0.0` cost. Add a `tracing::warn!` in `estimate_cost_usd` when `price_for` returns `None` so this is observable.

---

## 4. Budget Caps

**Decision: monthly cap per user, soft warn at 80%, hard 429 at 100%.**

**Recommended initial limits for solo-dev:**

| Limit | Value | Rationale |
|---|---|---|
| Monthly budget per user | $5.00 USD | Enough for heavy testing; trivial cost if only you use it |
| Soft-warn threshold | $4.00 (80%) | Banner in chat panel: "You have used 80% of your monthly LLM budget" |
| Hard block threshold | $5.00 (100%) | Return HTTP 429 with `{ "code": "budget_exceeded" }`; SSE stream never opens |

**Implementation:** at the top of `send_message`, after provider resolution, query `llm_usage_daily` for the current calendar month. If cumulative `estimated_cost_usd ≥ hard_limit`, return 429 before touching the LLM. If `≥ soft_limit`, include `"budget_warning": true` in the `done` SSE event so the frontend can show the banner.

Budget limits live in the `app_settings` table (see §5) so the admin can change them without redeploying.

**Tradeoff:** per-session caps are finer-grained but require storing session start cost and comparing mid-stream — complex and low-value at solo scale. Daily caps are simpler than monthly but would false-positive during legitimate heavy-use test days. Monthly with a $5 hard cap is the right balance.

---

## 5. Runtime Provider/Model Config

**Decision: `app_settings` key-value table, cached in `AppState` with a 60-second TTL.**

### Schema

```sql
CREATE TABLE app_settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_by UUID REFERENCES users(id),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Initial rows seeded by migration:

```
llm_default_provider   gemini
llm_gemini_model       gemini-2.5-flash-preview-05-20
llm_claude_model       claude-sonnet-4-20250514
budget_monthly_usd     5.00
budget_warn_pct        80
```

### Read/cache strategy

`AppState` holds a `SettingsCache { inner: Arc<RwLock<HashMap<String,String>>>, last_refreshed: Instant }`. On each `send_message` call: if `last_refreshed` is older than 60 seconds, re-read all rows from `app_settings` into the cache. Otherwise use cached values. This is one `SELECT *` every 60s across all active users — negligible at this scale.

**Who can write:** admin-only via `POST /api/admin/settings` (requires `AdminUser` extractor from §1 of candidates doc). Writes go directly to the DB; the next turn within 60s will pick them up.

### Live-swap behavior

If an admin changes `llm_gemini_model` while a user's SSE stream is in progress, the in-flight request continues with the model it already opened a stream against — `stream_completion` was called before the cache refresh. The new model takes effect on the next turn. This is intentional: mid-stream model switching would produce incoherent responses. Document this in the admin UI: "Model changes take effect on the next conversation turn (up to 60 seconds)."

Also add a `model` column to `messages` (`TEXT`, nullable) so the rollup table can break down cost by model:

```sql
ALTER TABLE messages ADD COLUMN model TEXT;
```

**Tradeoff:** 60s TTL means an admin change takes up to 60s to propagate. A WebSocket/Postgres `LISTEN`/`NOTIFY` approach would be instant but is engineering overkill for a solo deployment.

---

## 6. Model Fallback

**Decision: no automatic fallback. Keep strict per-user provider selection. Add an admin-visible error metric only.**

Automatic fallback is tempting but introduces subtle problems: the user selected a provider for a reason (cost, quality, data-residency preference), and silently switching mid-conversation produces inconsistent tool-call behavior because Gemini and Claude have different function-calling wire formats. A partial conversation with mixed providers is harder to debug than a clean error.

**What to do instead:**

- On provider stream error, return the existing `provider_error` SSE event and let the user retry manually.
- Log error counts per provider to the admin dashboard (error rate signal from §1).
- The admin can change the global default provider in `app_settings` if a provider is down — that is the "circuit breaker" in practice.

**Tradeoff:** during a real Gemini outage the user must manually switch to Claude in settings. Acceptable for a solo deployment; revisit when there are multiple users who cannot self-service.

---

## 7. Tracing Field Additions

Add these fields to existing spans in `chat.rs`:

| Span | New fields |
|---|---|
| `chat.turn` | `input_tokens` (u32), `output_tokens` (u32), `estimated_cost_usd` (f64), `model` (str) |
| `chat.round` | `input_tokens` (u32), `output_tokens` (u32) — per-round subtotals |
| `chat.tool_call` | no change needed; `duration_ms` already present |

Record `total_usage` into `chat.turn` just before yielding the `done` event.

**Cardinality concern:** do NOT label Prometheus metrics (Tier 3, future) with `user_id` if you expect more than a few hundred users — that would create one series per user and blow the 10k series free tier. For tracing spans (not metrics), `user_id` is fine because spans are individual events, not aggregated label sets. At current solo-dev scale even a `user_id` Prometheus label is fine; add a comment to revisit at 100 users.

---

## 8. Token-Budget Rolling Window

**Decision: implement in Phase 3 as a pre-call truncation function in `context.rs`. Target: fit within 32K input tokens.**

The current 40-message hard cap (`MAX_HISTORY = 40`) is blind to actual token length. A 40-message conversation with large tool results can easily exceed 100K tokens, driving up cost and hitting model context limits.

**Proposed algorithm (`backend/src/ai/context.rs`):**

1. Walk the `llm_messages` slice in reverse (newest first).
2. Estimate tokens per message: `ceil(len(content) / 4)` — crude but provider-agnostic; no tiktoken dependency needed at this scale.
3. Keep messages until cumulative estimate reaches 28K tokens (leaving 4K headroom for the system prompt and new turn).
4. Always keep the system message (index 0) and at least the last user message.
5. If messages were dropped, prepend a synthetic `user` message: `"[Earlier context truncated to fit context window]"`.

This is a Phase 3 item, not Phase 4, because the current tool-use loop can generate 8 rounds × multiple tool results per round, easily producing 10K+ tokens of context in a single turn.

**Tradeoff:** character/4 token estimation undershoots CJK text and overshoots English. A proper tokenizer (`tiktoken-rs` for Claude, `sentencepiece` for Gemini) would be accurate but adds two heavy dependencies. The 4K headroom buffer compensates for estimation error at typical English usage.

---

## 9. System Prompt Runtime Editing

**Decision: Phase 4 concern. Do not expose system-prompt editing in Phase 3.**

The system prompt is composed at runtime from three sources (static `base.txt`, `users.planner_context`, routine state). The static `base.txt` file is code — changing it affects every user simultaneously and should go through a PR + deploy. `users.planner_context` is already user-editable at runtime. Routine state is dynamic by design.

An admin WYSIWYG editor for `base.txt` would require: a DB column or file override mechanism, versioning/rollback, and careful testing of how prompt changes interact with tool schemas. That is a meaningful feature with real regression risk. It earns its own planning slice when there is evidence that the static prompt is a bottleneck.

**Tradeoff:** if the prompt needs a hotfix, a redeploy is required. On fly.io (`fly deploy`) this takes under 2 minutes, which is acceptable.

---

## Summary

**Top 3 decisions:**

1. **Pricing table in code, not DB** — `static`-style `match` in `backend/src/ai/pricing.rs`; model price changes go through Git, not an admin form. `tracing::warn!` when a model has no entry.

2. **Daily rollup via hourly upsert job** — `llm_usage_daily` table keyed `(day, user_id, provider, model)`; refreshed by a Tokio background task every 60 seconds (incremental `INSERT ... ON CONFLICT`), not a materialized view, to stay compatible with fly.io managed Postgres.

3. **Monthly budget cap: soft warn at 80%, hard 429 at 100%** — initial limit $5/user/month stored in `app_settings` (admin-editable); check runs at the top of `send_message` before any LLM call; no automatic provider fallback on budget exceed.
