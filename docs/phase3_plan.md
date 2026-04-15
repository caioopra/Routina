# Phase 3 Plan — Admin Console, Governance, and Observability

**Status:** Planning (2026-04-15)
**Baseline:** Phase 2 merged — multi-turn LLM chat with tool-calling, Gemini + Claude providers, SSE streaming, per-user provider settings.

---

## 1. Goals

When Phase 3 ships, Caio can:

- Log in as admin and see a live dashboard with token spend, user list, active conversations, and provider error rates — all rendered from our own API, no external tool required.
- Toggle the LLM provider/model at runtime (no redeploy), disable the chat feature globally with one click, and have every change recorded in a permanent audit log.
- Trust that destructive actions (user deletion, model change, kill-switch) require password re-confirmation and cannot be triggered by CSRF or accidental clicks.
- Observe the production system through structured JSON logs shipped to Axiom, with 8 saved query panels covering latency, cost, error rate, and tool-call breakdown.

---

## 2. Scope Cut

### In scope (Phase 3)

- `users.role` column + `AdminUser` Axum extractor + CLI `promote` binary
- `audit_log` table capturing every admin mutation and auth event
- Step-up confirm-token flow for destructive admin operations
- Login rate-limit (per-email, 10 attempts / 15-minute window)
- `app_settings` key-value table: kill-switch, default provider/model, budget caps
- `llm_usage_daily` rollup table + hourly Tokio background task
- `messages.input_tokens`, `messages.output_tokens`, `messages.model` columns
- `backend/src/ai/pricing.rs` — hard-coded model pricing table
- Monthly budget cap: soft warn at $4, hard 429 at $5 (admin-editable in `app_settings`)
- Token-context truncation function in `context.rs` (32K window, drops oldest non-system messages)
- Admin API: `/api/admin/dashboard`, `/api/admin/users`, `/api/admin/settings`, `/api/admin/audit`
- Admin frontend: `/admin/dashboard`, `/admin/users`, `/admin/providers`, `/admin/audit`
- Impersonation: read-only "view-as" only — admin can read another user's data, no write-as
- Non-dismissible impersonation banner (`z-index: 9999`, red)
- JSON log toggle (`LOG_FORMAT=json`) + Fly.io → Axiom log drain
- Tool-call SSE arg allowlist per `toolLabels.js` (surface hardening)

### Out of scope (Phase 3)

- OTel tracing (`tracing-opentelemetry`, OTLP exporter) — Phase 4 candidate
- Prometheus `/metrics` endpoint + Grafana Cloud — Phase 4 candidate
- Per-user cost cap configuration UI — Phase 4 candidate
- System prompt WYSIWYG editor — Phase 4 candidate
- Write-as impersonation (admin acting as user) — Phase 4 candidate
- Goals/events/subtasks AI tools — Phase 4 candidate
- Scheduled LLM runs (proactive suggestions) — Phase 4 candidate
- Preview deployments / per-PR fly.io apps — Phase 4 candidate
- Multi-tenancy or granular RBAC beyond user/admin — Phase 4 candidate

---

## 3. Slices

### Slice A — Role Infrastructure

**Purpose:** Establish the admin identity layer that every subsequent slice depends on.

**Owner agents:** `database`, `backend`, `frontend`

**Deliverables:**

- `backend/migrations/005_user_role_and_message_tokens.sql`
  - `ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin'))`
  - `ALTER TABLE messages ADD COLUMN input_tokens INT, ADD COLUMN output_tokens INT, ADD COLUMN model TEXT`
- `backend/src/bin/promote.rs` — `cargo run --bin promote -- --email <addr>` sets `role = 'admin'` for that email; reads `DATABASE_URL` from env; exits non-zero if email not found
- `backend/src/middleware/auth.rs` — extend `load_user` to select `role`; add `role: String` to `CurrentUser`
- `backend/src/middleware/admin.rs` — `AdminUser` extractor: wraps `CurrentUser`, returns 403 if `role != "admin"`; DB read happens inside existing `load_user` (no extra query)
- `backend/src/routes/mod.rs` — mount `/api/admin` router behind `AdminUser`
- `frontend/src/stores/authStore.js` — add `role` field; populate from `/auth/me` response
- `frontend/src/components/admin/AdminRoute.jsx` — redirects non-admins to `/`; reads `useAuthStore().role`
- Login rate-limit: second `DashMap<String, RateLimitBucket>` in `AppState` keyed on normalized email; checked before DB lookup in `POST /api/auth/login`; clears on successful login; returns 429 with `Retry-After` header when exceeded (10 attempts / 15-minute sliding window)

**Depends on:** Phase 2 baseline

**Tests:**
- Integration: `POST /api/auth/login` with wrong password 11 times → 429 on 11th
- Integration: `GET /api/admin/dashboard` with a non-admin token → 403
- Integration: `GET /api/admin/dashboard` with admin token → 200
- Unit: `AdminUser` extractor returns 403 when `CurrentUser.role == "user"`
- Frontend: `AdminRoute` redirects when store has `role == "user"`

**Review after:** Verify migration runs cleanly on the CI test DB; verify `promote` binary works end-to-end; confirm no existing JWT handling is broken.

---

### Slice B — Audit Log + Step-Up Auth

**Purpose:** Record every admin mutation and auth event with a tamper-evident trail; gate destructive endpoints behind password re-confirmation.

**Owner agents:** `backend`, `security-reviewer`

**Deliverables:**

- `backend/migrations/006_audit_log.sql`
  ```sql
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
  CREATE INDEX audit_log_actor_created  ON audit_log (actor_id, created_at DESC);
  CREATE INDEX audit_log_action_created ON audit_log (action, created_at DESC);
  ```
  Retention enforced by a nightly DELETE (created_at < now() - 90 days) run inside the same hourly Tokio background task introduced in Slice C.
- `backend/src/middleware/audit.rs` — `emit_audit` async fn called explicitly from each admin mutation handler (not a Tower layer, to keep payload control per-action); strips secrets from `payload` before insert
- `POST /api/admin/confirm` — re-checks admin's password; returns a signed confirm JWT valid 5 minutes, action-scoped in claims (`{ action: "provider.update", exp }`) so it cannot be replayed against a different endpoint; requires `AdminUser`
- All admin mutation handlers accept `x-confirm-token` header; validate the action claim before proceeding; designated destructive actions: kill-switch toggle, user soft-delete, role grant/revoke, model change, rate-limit global reset
- Auth events logged: login success/failure, token refresh (in existing auth handlers)
- `GET /api/admin/audit` — paginated (cursor-based, `before` UUID param), filterable by `action` prefix and date range; returns 50 rows per page

**Depends on:** Slice A (AdminUser extractor, audit_log table references users)

**Tests:**
- Integration: toggle kill-switch without confirm token → 403; with expired confirm token → 403; with valid confirm token → 200 + audit row inserted
- Integration: confirm token for `action: provider.update` rejected on kill-switch endpoint
- Integration: `POST /api/auth/login` failure writes `auth.login.fail` row to audit_log
- Unit: `emit_audit` strips a key named `password` from payload JSONB before insert

**Review after:** security-reviewer checks confirm token claims validation, audit payload stripping, and that no read-only admin endpoints emit audit rows.

---

### Slice C — LLM Cost/Usage + Runtime Config

**Purpose:** Persist token usage per message, roll up to a daily table, enforce budget caps, and allow runtime provider/model/kill-switch changes without a redeploy.

**Owner agents:** `backend`, `ai-prompt`, `database`

**Deliverables:**

- `backend/migrations/007_llm_usage_and_app_settings.sql`
  - `llm_usage_daily` table (PRIMARY KEY `(day, user_id, provider, model)`, `estimated_cost_usd NUMERIC(10,6)`)
  - `app_settings` table (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_by UUID, updated_at TIMESTAMPTZ)
  - Seed rows: `llm_default_provider = gemini`, `llm_gemini_model = gemini-2.5-flash-preview-05-20`, `llm_claude_model = claude-sonnet-4-20250514`, `budget_monthly_usd = 5.00`, `budget_warn_pct = 80`, `chat_enabled = true`
- `backend/migrations/008_user_rate_limits.sql` — `user_rate_limits` table (user_id UNIQUE FK, daily_token_limit BIGINT nullable, daily_request_limit INT nullable, override_reason TEXT, set_by UUID FK)
- `backend/src/ai/pricing.rs` — `price_for(provider, model) -> Option<ModelPrice>` static match; `estimate_cost_usd(...)` with `tracing::warn!` on unknown model
- `backend/src/ai/context.rs` — `truncate_to_budget(messages, max_tokens: u32) -> Vec<ChatMessage>` walking reverse, char/4 estimate, always keeps system message + last user message, prepends truncation notice if dropped; default max 28 000 tokens
- `chat.rs` — on assistant message insert: bind `input_tokens`, `output_tokens`, `model` from `total_usage`; add pre-call budget check (query `llm_usage_daily` for current calendar month, return 429 with `budget_exceeded` if ≥ hard limit, include `budget_warning: true` in `done` event if ≥ soft limit)
- `AppState` gains `SettingsCache { inner: Arc<RwLock<HashMap<String,String>>>, refreshed_at: Instant }`; stale after 60 s; a kill-switch write invalidates immediately
- Hourly Tokio background task in `main.rs`: (1) upsert `llm_usage_daily` for today from `messages`; (2) delete `audit_log` rows older than 90 days
- Admin endpoints: `GET /api/admin/settings`, `POST /api/admin/settings` (confirm token required for model change), `GET /api/admin/metrics/usage` (wraps `llm_usage_daily` grouped by day/provider), `POST /api/admin/users/:id/rate-limit` (set per-user override), `POST /api/admin/users/:id/delete` (soft-delete: sets `deleted_at`, confirm token required)

**Depends on:** Slices A + B (AdminUser, audit emission, app_settings table, audit_log)

**Tests:**
- Integration: send a message, assert `messages` row has non-null `input_tokens`, `output_tokens`, `model`
- Integration: mock monthly spend at $5.01 → `POST /api/chat/message` returns 429 with `budget_exceeded`
- Unit: `truncate_to_budget` with 50-message history → returned slice fits within 28K estimated tokens, first element is system message
- Unit: `estimate_cost_usd("gemini", "unknown-model", 1000, 1000)` returns 0.0 and emits warn log
- Integration: `POST /api/admin/settings` changes `llm_default_provider` → `GET /api/admin/settings` reflects new value; next `chat.rs` resolve (after cache TTL) uses new provider
- Integration: rollup task runs for today → `llm_usage_daily` row exists with correct token sums

**Review after:** confirm budget check cannot be bypassed via race condition; verify `app_settings` seed runs idempotently on re-migration.

---

### Slice D — Admin Console Frontend

**Purpose:** Ship the four admin pages (dashboard, providers, users, audit log) wired to the Slice C endpoints, with correct confirmation flows and impersonation UX.

**Owner agents:** `frontend`, `testing`

**Deliverables:**

- React Router nested routes: `/admin` → redirect to `/admin/dashboard`; `/admin/dashboard`, `/admin/providers`, `/admin/users`, `/admin/audit` all wrapped in `AdminRoute` and `AdminShell`
- Components in `frontend/src/components/admin/`:
  - `AdminRoute.jsx` — role guard
  - `AdminShell.jsx` — layout: persistent left sidebar + `<Outlet>` + impersonation banner slot
  - `AdminSidebar.jsx` — nav links matching the 4 routes
  - `ImpersonationBanner.jsx` — `position: fixed; top: 0; z-index: 9999; bg-red-700`; shows target email; non-dismissible; "Exit" clears impersonation token from authStore and reloads admin token
  - `MetricCard.jsx` — label, value, delta badge, optional sparkline via `recharts` `<LineChart>`
  - `KillSwitchToggle.jsx` — reads `chat_enabled` from dashboard endpoint; toggle triggers `StepUpModal`
  - `DataTable.jsx` — sortable, paginated; card-stack layout below 900 px
  - `ConfirmDialog.jsx` — Tier 1 modal (Cancel / Confirm)
  - `StepUpModal.jsx` — password re-entry → `POST /api/admin/confirm` → stores confirm token for one use
  - `TypeToConfirmInput.jsx` — controlled input enabling submit only on exact match string
  - `AuditTimeline.jsx` — infinite-scroll over `GET /api/admin/audit` cursor pages; action badge coloring
  - `ProviderForm.jsx` — provider select + model select; save triggers `StepUpModal`
  - `UserRow.jsx` — inline role badge, "View as" button, action menu (rate-limit reset, soft-delete)
- `frontend/src/stores/authStore.js` — add `impersonation: { active: boolean, targetEmail: string | null }`; `setAuth` detects `impersonated_by` claim in parsed JWT and sets this field
- `frontend/src/api/settings.js` — extend with admin API calls (dashboard metrics, settings CRUD, audit log pagination, user management)
- `recharts` added as a dependency (for `MetricCard` sparklines and dashboard bar/line charts)
- Data fetching: dashboard polls every 30 s via React Query `refetchInterval`; other pages load on-demand; kill-switch uses optimistic mutation

**Depends on:** Slices A + B + C (all admin endpoints must exist)

**Tests:**
- `AdminRoute.test.jsx` — non-admin role → redirect to `/`, admin role → renders children
- `StepUpModal.test.jsx` — submits password, receives confirm token, calls onSuccess with token
- `KillSwitchToggle.test.jsx` — toggle click opens StepUpModal; on confirm, calls correct endpoint
- `ImpersonationBanner.test.jsx` — renders when `impersonation.active` is true; Exit click calls `exitImpersonation` action
- `AuditTimeline.test.jsx` — renders first page; scroll trigger fetches next page cursor
- `DataTable.test.jsx` — sorts on header click; shows card layout when viewport mocked to 800 px
- MSW handlers for all admin endpoints added to `frontend/src/test/mocks/handlers.js`

**Review after:** visual QA of the dashboard on 1280 px and 375 px viewports; verify impersonation banner cannot be dismissed; verify `recharts` bundle size impact is acceptable.

---

### Slice E — Observability Wiring

**Purpose:** Switch production logs to structured JSON, ship them to Axiom, and verify the 8 saved dashboard panels work.

**Owner agents:** `infra`, `backend`

**Deliverables:**

- `backend/Cargo.toml` — enable `json` feature on `tracing-subscriber` (already a dep)
- `backend/src/main.rs` — `LOG_FORMAT=json` gate:
  ```rust
  match std::env::var("LOG_FORMAT").as_deref() {
      Ok("json") => tracing_subscriber::fmt().json().with_env_filter(...).init(),
      _          => tracing_subscriber::fmt().with_env_filter(...).init(),
  }
  ```
- `tower-http` `TraceLayer` already in use — ensure `matched_path` (not raw URI) is the `path` field in request log lines (cardinality guard)
- `chat.rs` spans updated: `chat.turn` emits `input_tokens`, `output_tokens`, `estimated_cost_usd`, `model` fields; `chat.round` emits `input_tokens`, `output_tokens`
- Tool-call SSE arg allowlist: `backend/src/routes/chat.rs` reads allowed keys from a const per tool name before emitting `tool_call` SSE event; `frontend/src/components/chat/toolLabels.js` defines the display-side matching allowlist
- `fly.toml` — `[env] LOG_FORMAT = "json"`
- Fly.io log drain setup documented in `docs/runbook.md` (one-time CLI command, uses `AXIOM_TOKEN` secret)
- `.env.example` — add `LOG_FORMAT=json` and `AXIOM_TOKEN=` entries
- Axiom: create dataset `planner-prod`, configure the 8 saved APL query panels per the observability brief (active turns, p95 latency, token spend, tool-call failure rate, HTTP error rate, provider error rate, turns/hour, tool breakdown)

**Depends on:** Slices A–D (all span fields and endpoints must exist before panels are useful)

**Tests:**
- Unit: `main.rs` initializes JSON formatter when `LOG_FORMAT=json` is set (test via env override in test binary)
- Integration: `POST /api/chat/message` with `LOG_FORMAT=json` set → log output contains `input_tokens` field as valid JSON number
- Smoke: `cargo build` with `json` feature enabled passes without warnings
- Manual: fly deploy to staging; verify Axiom dataset receives structured events within 2 minutes of drain creation

**Review after:** confirm no `user_id` or `conversation_id` values appear as Axiom panel group-by dimensions (only in log fields); verify `AXIOM_TOKEN` is not echoed in any log line; run `cargo clippy -- -D warnings` clean.

---

## 4. Decisions

| # | Decision | Rationale | Source |
|---|----------|-----------|--------|
| 1 | **Resolve role from DB on every admin request, not from JWT claims.** | A demoted admin loses access on the next request, not at token expiry (~15 min). The DB round-trip already exists in `load_user`. | security.md §2 |
| 2 | **Audit log present from day one; `actor_email` denormalized.** | The solo admin is the only observer of anomalous activity. Denormalized email survives user deletion. | security.md §3 |
| 3 | **Step-up uses short-lived action-scoped confirm tokens, not MFA.** | No second device required; prevents CSRF and accidental double-clicks; action name in claims blocks replay across endpoints. | security.md §5 |
| 4 | **Pricing table hard-coded in `backend/src/ai/pricing.rs`, not DB-editable.** | Pricing changes a few times a year; a code change produces a Git history entry and avoids a migration + admin form just to update two floats. `tracing::warn!` on unknown model. | ai_governance.md §3 |
| 5 | **Hourly `INSERT ... ON CONFLICT` upsert task, not a materialized view.** | fly.io managed Postgres does not support `REFRESH MATERIALIZED VIEW CONCURRENTLY` without superuser. Incremental upserts are simpler to test and reason about. | ai_governance.md §2 |
| 6 | **Budget: $5/mo hard block, $4 soft warn; stored in `app_settings`, admin-editable.** | Enough for heavy testing; avoids surprise LLM bills; admin can raise without a redeploy. | ai_governance.md §4 |
| 7 | **Axiom only for logs + events-as-metrics. OTel traces deferred to Phase 4.** | Fly.io native drain → Axiom requires zero sidecar. Free tier covers 500 GB/month. Grafana + Prometheus adds two moving parts with no incremental value at solo scale. | observability.md §1 |
| 8 | **`LOG_FORMAT=json` env toggle; production sets it in `fly.toml [env]`, local dev stays text.** | Avoids noisy JSON in local terminal while ensuring structured ingest in production. | observability.md §2 |
| 9 | **Admin routes nested under `/admin/*`, not tabs.** | Bookmarkable URLs, browser back/forward, React Router `<Outlet>` shares the shell. | admin_ui.md §2 |
| 10 | **Reuse planner's dark purple design system for admin; no second theme.** | One design system to maintain. Destructive intent is communicated at the action level via step-up auth and `bg-red-600` buttons, not a separate color palette. | admin_ui.md §4 |
| 11 | **Impersonation banner: non-dismissible, `position: fixed; top: 0; z-index: 9999`, `bg-red-700`.** | Cannot be hidden accidentally; unmissable during any admin write. | admin_ui.md §8 / security.md §7 |
| 12 | **Migrations 005–008, all additive, no breaking changes.** | New columns have defaults; new tables have no FKs that existing rows violate. Zero-downtime deploy. | data_model.md §8 |
| 13 | **Login rate-limit: per-email sliding window, 10 attempts / 15 min.** | Per-IP fails behind NAT. Per-email targets credential stuffing without penalizing shared networks. Bucket cleared on successful login. | security.md §9 |
| 14 | **No automatic provider fallback. Manual-only via `app_settings`.** | Silent mid-conversation provider switch produces incoherent tool-call behavior (Gemini and Claude have different function-calling wire formats). Admin changes the global default if a provider is down. | ai_governance.md §6 |
| 15 | **`app_settings` value column is `TEXT`, not `JSONB`.** The data_model.md brief proposed JSONB; ai_governance.md and security.md both assumed TEXT scalar values. TEXT is chosen because all current and planned setting values are simple strings/booleans/numbers. JSONB adds no benefit and requires casting on every read. Override: data_model.md JSONB recommendation dropped. | Simpler read/write with `serde_json::from_str` on application side where structure is needed. | data_model.md §5 (overridden) |
| 16 | **Token-context truncation ships in Phase 3 (not Phase 4).** | A 8-round tool-use loop can already exceed 100K tokens in Phase 2. Deferring risks silent cost overruns before budget caps are even meaningful. | ai_governance.md §8 |

---

## 5. Risks

**Risk 1 — Confirm token clock skew invalidates step-up on slow machines.**
If the backend and client clocks diverge by more than the 5-minute confirm token TTL, all destructive admin ops will fail with 401. Mitigation: use server-side time exclusively for token expiry validation; document that `fly.io` machine time is synchronized via NTP and is reliable.

**Risk 2 — Hourly rollup job falls behind during a burst of LLM activity.**
If the rollup upsert is slower than 1 hour (e.g., large `messages` table scan), budget cap checks see stale data and undercount actual spend. Mitigation: the rollup query is date-scoped to today only (bounded scan); add `EXPLAIN ANALYZE` to CI smoke test; add `tracing::warn!` if upsert wall time exceeds 30 seconds.

**Risk 3 — `app_settings` cache TTL of 60 s means a kill-switch toggle takes up to 60 s to propagate to in-flight chat streams.**
Admin expects immediate effect. Mitigation: the kill-switch write path bypasses the TTL by invalidating the in-memory cache immediately (set `refreshed_at = Instant::now() - 61s`). Document this in the admin UI tooltip: "Kill-switch takes effect within one request cycle (< 1 s)."

**Risk 4 — Admin console at `/admin` is accessible to any authenticated user before the `AdminUser` extractor blocks the API.**
The frontend `AdminRoute` guard is a UX courtesy, not a security boundary. A regular user can navigate to `/admin/dashboard` in their browser. Mitigation: every `/api/admin/*` handler requires `AdminUser`; 403 JSON is returned regardless of frontend state. The frontend 403 response handler redirects to `/`.

**Risk 5 — Axiom drain setup is a manual one-time step not captured in CI.**
If the drain is not configured in a new fly.io app or after a machine replacement, logs go to ephemeral stdout only and panels show empty. Mitigation: document the `fly logs drain create` command in `docs/runbook.md`; add a startup log line `tracing::info!(log_drain = "axiom", "structured logging active")` that is visible in Axiom immediately after a successful drain config.

---

## 6. Agent Team Assignments

| Slice | Task | Agent |
|-------|------|-------|
| A | Migration 005 (role + message token columns) | `database` |
| A | `promote` CLI binary | `backend` |
| A | `load_user` role extension + `CurrentUser.role` + `AdminUser` extractor | `backend` |
| A | Login rate-limit (per-email DashMap bucket in AppState + login handler check) | `backend` |
| A | `AdminRoute.jsx` + `authStore` role field | `frontend` |
| A | Integration tests: rate-limit, AdminUser 403, promote binary | `testing` |
| B | Migration 006 (audit_log table + indexes) | `database` |
| B | `emit_audit` function + admin mutation handler instrumentation | `backend` |
| B | `POST /api/admin/confirm` confirm-token endpoint | `backend` |
| B | `GET /api/admin/audit` paginated endpoint | `backend` |
| B | Auth event logging in existing auth handlers | `backend` |
| B | Integration + unit tests for audit log and confirm token | `testing` |
| B | Step-up auth flow review, confirm token claim validation | `security-reviewer` |
| C | Migrations 007 + 008 (llm_usage_daily, app_settings, user_rate_limits) | `database` |
| C | `pricing.rs` hard-coded pricing table | `ai-prompt` |
| C | `context.rs` token-budget truncation function | `ai-prompt` |
| C | `chat.rs` — bind token/model columns on insert, pre-call budget check | `backend` |
| C | `SettingsCache` in `AppState` + hourly rollup + 90-day audit purge Tokio task | `backend` |
| C | Admin settings/metrics/rate-limit/delete endpoints | `backend` |
| C | Integration + unit tests for budget cap, rollup, pricing, truncation | `testing` |
| D | All 13 admin React components in `components/admin/` | `frontend` |
| D | Nested `/admin/*` React Router routes + `AdminShell` layout | `frontend` |
| D | `authStore` impersonation field + `setAuth` JWT detection | `frontend` |
| D | `recharts` dependency + MetricCard sparklines | `frontend` |
| D | MSW handlers for all admin endpoints | `testing` |
| D | Component tests (AdminRoute, StepUpModal, KillSwitchToggle, ImpersonationBanner, AuditTimeline, DataTable) | `testing` |
| D | Visual QA review of admin console layout and confirmation flows | `code-reviewer` |
| E | `tracing-subscriber` json feature + `LOG_FORMAT` gate in `main.rs` | `backend` |
| E | `matched_path` cardinality guard on TraceLayer; span field additions in `chat.rs` | `backend` |
| E | Tool-call SSE arg allowlist enforcement in `chat.rs` | `backend` |
| E | `fly.toml` `LOG_FORMAT=json` env + `.env.example` entries | `infra` |
| E | Fly.io log drain setup (Axiom); `docs/runbook.md` drain command | `infra` |
| E | Axiom dataset creation + 8 saved APL dashboard panels | `infra` |
| E | Unit + integration tests for JSON log output + allowlist enforcement | `testing` |
| E | Final `cargo clippy -- -D warnings` clean pass; `npm run build` clean pass | `code-reviewer` |
