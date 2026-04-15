# Phase 3 Admin UI Plan

## 1. Information Architecture

Pages in priority order:

**MVP (Phase 3):**
1. **Dashboard** — KPI cards: total users, token spend today/month, active conversations, chat kill-switch toggle. Highest signal-to-noise; opens first on `/admin`.
2. **Providers & Models** — select active provider, active model per provider. Step-up auth required to save. Replaces the `ProviderToggle` per-user hack with a global admin control.
3. **Users** — list users (email, role, joined date, last active, usage total). Role grant/revoke. Rate-limit manual reset. Soft-delete. No impersonation writes in Phase 3.
4. **Audit Log** — timeline of admin mutations and auth events from the `audit_log` table. Filter by action, actor, date range.

**Phase 4 (defer):**
- Usage & Cost breakdown (requires observability Tier 1 rollup materialized view to be non-trivial to build)
- Errors / recent log tail (requires structured JSON logging + aggregator, observability Tier 2)
- Grafana / metrics dashboard embed (observability Tier 3 prerequisite)
- Per-user cost cap configuration
- Kill-switches per feature beyond `chat_enabled`

---

## 2. Route Structure

Use nested routes under `/admin`, not a single tabbed page.

```
/admin              → redirect to /admin/dashboard
/admin/dashboard    → KPIs + kill-switch
/admin/providers    → model config
/admin/users        → user list + role actions
/admin/audit        → audit log timeline
```

Rationale: bookmarkable URLs, React Router `<Outlet>` shares the admin shell (sidebar + impersonation banner slot) across all pages, browser back/forward work naturally. A single-page tab approach loses URL state and makes deep-linking into the audit log impossible.

Admin shell: persistent left sidebar (collapsible on narrow viewports), reuses the same topbar as the planner. The sidebar items map 1:1 to routes.

---

## 3. Role Gating

**Two layers, both required:**

**Layer 1 — Client-side guard.**
An `<AdminRoute>` wrapper component reads `user.role` from `useAuthStore`. If `role !== 'admin'`, it redirects to `/` (the planner home). The guard runs on every render, so a role change takes effect when the auth store refreshes (i.e., on next `/auth/me` poll or page load). The admin nav entry in the main sidebar is conditionally rendered only when `role === 'admin'`.

**Layer 2 — Backend 403.**
Every `/api/admin/*` handler uses the `AdminUser` extractor (per security brief §2), which reads `role` from DB on every request. A non-admin token gets a 403 JSON response regardless of what the frontend shows.

**Direct URL access by a non-admin:** return **403**, not 404. Reasoning: the `/admin` routes are not a secret (anyone can read the source); returning 404 to "hide" them provides no real security and creates confusion when debugging. The 403 is honest about the access control and consistent with how the API responds. The client-side guard redirects before the backend is hit in normal flows; the 403 is a backstop.

---

## 4. Design Language

**Recommendation: reuse the dark purple planner aesthetic.**

Tradeoff: a distinct red/gray "danger zone" theme signals "powerful controls" but adds a maintenance burden (two design systems) and makes the admin feel like a separate app. Since the solo-dev deployment means Caio is always both user and admin in the same session, visual continuity reduces context-switch friction. Danger is signaled instead at the action level: destructive buttons use `bg-red-600` and the step-up auth modal acts as the "think before clicking" gate.

---

## 5. Metrics Rendering

**Recommendation: native charts from `/api/admin/metrics/*` using `recharts`.**

Grafana iframe embedding requires either a public Grafana instance or a signed iframe embed token — both are out of scope until observability Tier 3 is built. Native `recharts` renders against our own API endpoints, works offline, respects the dark theme without iframe styling hacks, and is ~10 KB gzipped. Use it for the Dashboard KPI cards and any future usage charts.

When observability Tier 3 ships (Prometheus + Grafana Cloud), add an optional iframe panel to the Dashboard page for the Grafana board — the native cards stay as the primary view.

---

## 6. Data Fetching Pattern

| Page | Pattern | Rationale |
|---|---|---|
| Dashboard | Poll every 30 s via React Query `refetchInterval` | KPIs are useful as near-live; SSE is heavier than needed for 4 numbers |
| Providers & Models | On-demand (load once, refetch on save) | Config changes rarely; stale data on load is fine |
| Users | On-demand + manual refresh button | List is stable; no need for background churn |
| Audit Log | On-demand + manual refresh button | Historical read; infinite scroll pagination, not a live feed |
| Kill-switch toggle | Optimistic mutation, no polling | Toggle state is written by this UI; no other writer to race with |

---

## 7. Destructive Action Confirmation

Three severity tiers:

**Tier 1 — Reversible mutations** (rate-limit reset, role change): standard `<ConfirmDialog>` — a modal with Cancel / Confirm buttons. Single click to proceed.

**Tier 2 — Consequential but recoverable** (kill-switch toggle, provider model change): step-up auth modal — re-enter password. Backend issues a confirm token (security brief §5); frontend submits it with the mutation. No type-to-confirm; the password re-entry is the gate.

**Tier 3 — Irreversible or high-blast-radius** (user soft-delete, bulk data wipe): type-to-confirm ("type DELETE to continue") AND step-up auth. Both gates must pass. The confirm input is a controlled `<input>` that enables the submit button only on exact match.

---

## 8. Impersonation UX

Phase 3 ships read-only "view-as" per the security brief §7.

**Entry:** Admin navigates to a user row in `/admin/users`, clicks "View as". Step-up auth is required (Tier 2). On success, the backend issues a short-lived impersonation JWT (15 min, non-renewable).

**Active indicator:** a full-width, fixed, non-dismissible red banner at the very top of the viewport — above the planner topbar — reading: "Viewing as [user email] — impersonation active. [Exit]". The banner uses `bg-red-700` with white text. The topbar and sidebar sit below it; the banner is `position: fixed; top: 0; z-index: 9999`.

**Exit:** clicking "Exit" in the banner clears the impersonation token from the auth store and reloads the admin's own token from `localStorage` / cookie. The admin lands back on `/admin/users`.

**authStore integration:** add an `impersonation: { active: boolean, targetEmail: string | null }` field. The `setAuth` action detects `impersonated_by` in the parsed JWT and sets this field. A new `useImpersonation()` selector reads it; the banner is rendered at the `App` root level conditionally on `impersonation.active`.

---

## 9. Responsive / Mobile

**Desktop-first with degraded mobile** — not fully responsive.

The admin console targets 1024 px+ viewports. Below 900 px (the existing planner breakpoint): the admin sidebar collapses to a hamburger menu, the DataTable switches to a card-stack layout, and metric cards stack vertically. Forms remain usable. No admin features are blocked on mobile, but the experience is not optimized — the tradeoff is accepted because admin use from a phone is an edge case and the planner's mobile experience is the real product surface.

---

## 10. Component Inventory

| Component | Location | Purpose |
|---|---|---|
| `AdminRoute` | `components/admin/AdminRoute.jsx` | Role guard; redirects non-admins to `/` |
| `AdminShell` | `components/admin/AdminShell.jsx` | Layout wrapper: sidebar + `<Outlet>` + impersonation banner slot |
| `AdminSidebar` | `components/admin/AdminSidebar.jsx` | Left nav with route links to the 4 admin pages |
| `ImpersonationBanner` | `components/admin/ImpersonationBanner.jsx` | Fixed red banner shown when impersonation JWT is active |
| `MetricCard` | `components/admin/MetricCard.jsx` | KPI display: label, value, delta badge, optional sparkline |
| `KillSwitchToggle` | `components/admin/KillSwitchToggle.jsx` | Toggle with step-up auth flow for `chat_enabled` |
| `DataTable` | `components/admin/DataTable.jsx` | Sortable, paginated table; card-stack on mobile |
| `ConfirmDialog` | `components/admin/ConfirmDialog.jsx` | Tier 1 confirmation modal (Cancel / Confirm) |
| `StepUpModal` | `components/admin/StepUpModal.jsx` | Password re-entry → confirm token fetch; used for Tier 2 + 3 |
| `TypeToConfirmInput` | `components/admin/TypeToConfirmInput.jsx` | Controlled input that enables submit only on exact string match; Tier 3 |
| `AuditTimeline` | `components/admin/AuditTimeline.jsx` | Infinite-scroll list of `audit_log` rows with action badges |
| `ProviderForm` | `components/admin/ProviderForm.jsx` | Provider + model selects; save triggers StepUpModal |
| `UserRow` | `components/admin/UserRow.jsx` | Single row in the users DataTable; inline role badge + action menu |

---

## Summary

**Top 3 decisions:**

1. **Nested routes (`/admin/dashboard`, `/admin/users`, etc.)** over a single tabbed page. Bookmarkable, shell-composable, back/forward safe.

2. **Dark purple design system (no separate admin theme).** Danger is gated at the action level (step-up auth + type-to-confirm) rather than the page level. Reduces maintenance surface and keeps the admin feeling like part of the same app.

3. **Non-dismissible red impersonation banner at `z-index: 9999`.** A persistent, unmissable signal that the admin is acting as another user — impossible to forget, impossible to close accidentally.

**Component inventory:** `AdminRoute`, `AdminShell`, `AdminSidebar`, `ImpersonationBanner`, `MetricCard`, `KillSwitchToggle`, `DataTable`, `ConfirmDialog`, `StepUpModal`, `TypeToConfirmInput`, `AuditTimeline`, `ProviderForm`, `UserRow`.
