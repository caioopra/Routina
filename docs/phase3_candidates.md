# Phase 3 Candidates

Parking lot for ideas, deferred items, and not-yet-prioritized work. Nothing here is committed scope. Promote an entry to a real plan doc (e.g. `phase3_plan.md`) when it's ready to be worked on.

---

## 1. Admin role + super-user capabilities

**Motivation:** the initial deployment is for Caio's personal use. The same account that plans routines should also have privileged controls over the app itself — switching models, reading usage metrics, kill-switching features, debugging other (future) users.

**Shape:**
- New `users.role` column (TEXT NOT NULL DEFAULT 'user', CHECK role IN ('user', 'admin')). Migration `005_user_role.sql`.
- `AdminUser` Axum extractor that 403s non-admins. Sibling of the existing `CurrentUser`.
- Admin-only router mounted under `/api/admin/` with:
  - `GET /api/admin/metrics/usage` — aggregate token spend by user, day, provider
  - `GET /api/admin/metrics/requests` — request count, p95 latency by route
  - `GET /api/admin/users` — list users, impersonation? (later)
  - `POST /api/admin/providers/config` — change default model per provider without a redeploy (runtime config via DB-backed settings)
  - `POST /api/admin/rate-limit/reset/:user_id` — manual unblock
  - `GET /api/admin/errors/recent` — tail of error-level tracing events
- Frontend: a separate `/admin` route, protected by `role === 'admin'` from `GET /auth/me`. Dark-themed admin console; reuse the existing design system.
- Seed: add a mechanism to promote a user to admin via CLI (`cargo run --bin promote -- --email caio@...`) — avoid baking any email into code.

**Auth integration:** JWT already carries `sub`. Options: (a) add `role` to JWT claims at issue time (stale on role change until token expires), or (b) re-read role from DB on each admin request (safer, one extra query). Start with (b), move to (a) only if latency becomes a problem.

**Out of scope for this slice:** multi-tenancy, audit log of admin actions (tempting: `admin_actions` table), per-permission granularity beyond the single admin/user flag.

---

## 2. Observability + metrification

**Motivation:** today, tracing spans stream to stdout in text format. You can't slice by user, conversation, or tool beyond grepping a single process's logs. No historical view, no dashboards, no cost tracking.

**Tiered plan — pick up in order; each tier stands alone:**

### Tier 1 — Cost rollup (highest value for AI app)
- Persist `usage` from `ProviderEvent::Done` into `messages` table (new columns `input_tokens INT`, `output_tokens INT`, nullable).
- Materialized view or nightly rollup: `user_usage_daily(user_id, date, provider, input_tokens_sum, output_tokens_sum, msg_count)`.
- Admin metrics endpoint reads this. User's own `/me` page could show "this week: X tokens".
- Optional: enforce a monthly spend cap per user at the rate-limit layer.

### Tier 2 — Structured logs + aggregator
- `LOG_FORMAT=json` env toggle (`tracing_subscriber::fmt().json()`).
- Fly.io ships stdout; wire to **Axiom** (generous free tier, AI-app friendly) or **Grafana Cloud Loki**.
- Saved queries: errors by provider, slowest 10 conversations, tool-call failure rates.

### Tier 3 — Prometheus metrics endpoint
- `axum-prometheus` crate → `/metrics` endpoint (admin-guarded or allowlisted).
- Built-in: request count / latency / status per route.
- Custom metrics: `llm_tool_call_duration_seconds{tool=...}`, `llm_tokens_total{provider=,kind=}`.
- Scraped by Grafana Cloud (free tier: 10k series).

### Tier 4 — OpenTelemetry tracing
- `tracing-opentelemetry` → **Honeycomb** or Grafana Tempo.
- Visual span waterfall across chat turn → rounds → tool calls.
- Worth it only when debugging latency requires correlating spans; today's stdout tracing is enough.

### Dashboards
- Grafana dashboards (or Axiom saved views) for: token spend, active users, request rate, tool call breakdown, error rate by provider. Embed live snapshots into the admin console.

---

## 3. Product features deferred from Phase 2

- **Goals, events, subtasks** — tables exist (Phase 1), no AI tools target them. Phase 3 could add `create_goal`, `log_event`, etc.
- **AI routine summarization** — weekly "here's what you did" digest, written by the assistant.
- **Scheduled LLM runs** — proactive suggestions, e.g., Sunday evening "review the week" nudge. Requires a job runner (or a Fly.io scheduled machine).
- **Conversation branching** — fork a conversation from any message, try alternative approaches.
- **Streaming cancellation UI polish** — already have the Stop button; could add "canceled mid-response" indicator on the message bubble.
- **Token budget UI** — show the rolling window cap in the chat header so the user knows when old messages are dropping.

---

## 4. Security / robustness items deferred from Phase 2 reviews

- **Rate limit on `/auth/login`** — per-IP (or per-email) fixed window to block credential stuffing. Reuse the existing `RateLimitState` pattern keyed on `IpAddr` + username.
- **`tool_call` SSE surface area** — current event emits `args` verbatim. Current tool schemas only carry user-owned data, so no exploit today; revisit when tools with server-derived or cross-user data are added.
- **Audit log of admin actions** — once admin endpoints exist, log every mutation (who, what, when) to a dedicated table.
- **Token-budget rolling window cap** — current chat handler sends last 40 messages to the LLM regardless of token length. Real cap would be budget-based (truncate to ~32K tokens of history).
- **Cost cap per user per day** — prevents runaway tool-use loops from bankrupting the account. Needs the Tier 1 cost rollup to exist.

---

## 5. Developer experience

- **Preview deployments** — per-PR fly.io apps for manual QA.
- **Seed data for dev** — one command that creates a demo user with a populated routine so new-machine setup is fast.
- **`make reset` target** — drop + recreate DB + migrate + seed.
- **E2E frontend tests** — Playwright smoke test hitting a real backend via docker-compose.

---

**How to use this doc:**
- Add ideas freely, keep them small (a paragraph each).
- When an item is ready to work on, extract it into a dedicated `phase3_X_plan.md` with real scope, slicing, and task owners — matching the rigor of `phase2_plan.md`.
- Delete entries that are obsolete or shipped.
