# Phase 3 Security Plan

## 1. Role Storage Shape

**Recommendation:** `users.role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin'))` — single column, migration `005_user_role.sql`.

**Tradeoff:** A `roles`/`user_roles` RBAC table is future-proof but adds join complexity for a two-role system that will stay two roles for the entire solo-dev phase; the CHECK constraint is trivially upgradeable later.

Add the `AdminUser` Axum extractor as a thin wrapper over `CurrentUser` that returns 403 if `user.role != "admin"`. Mount all admin routes under `/api/admin/` behind this extractor — defense-in-depth on top of the router-level `auth_middleware` already in place.

---

## 2. Role Resolution Per Request

**Recommendation:** Read `users.role` from the DB on every admin-gated request (the DB-read-per-request path already exists in `CurrentUser`'s `load_user`).

**Tradeoff:** One extra DB round-trip per admin request is immaterial at solo-dev scale; baking `role` into the JWT means a promoted-or-demoted user keeps the wrong role until their token expires (~15 min access tokens), which is an unacceptable window for a privilege change.

Concretely: extend `load_user` to return the `role` field, add it to `CurrentUser`, and have `AdminUser::from_request_parts` call the same function and gate on `role == "admin"`. Do not add `role` to JWT claims in Phase 3.

---

## 3. Audit Log

**Recommendation:** Add an `audit_log` table for all admin mutations, plus the specific non-admin events listed below.

### Events that MUST be logged

| Event class | Examples |
|---|---|
| Admin mutations | provider config change, rate-limit override, kill-switch toggle, role grant/revoke |
| Impersonation | session start / end (if implemented) |
| Destructive ops | user deletion, bulk routine wipe |
| Auth events | login success/failure, token refresh, password change |

Read-only admin queries (metrics, user list) do NOT need per-row audit entries; access logs at the HTTP layer are sufficient.

### Proposed schema

```sql
CREATE TABLE audit_log (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id        UUID REFERENCES users(id) ON DELETE SET NULL,
    actor_email     TEXT NOT NULL,          -- denormalised; survives user deletion
    impersonating   UUID REFERENCES users(id) ON DELETE SET NULL, -- NULL if not impersonating
    action          TEXT NOT NULL,          -- e.g. 'admin.provider.update', 'auth.login.fail'
    target_type     TEXT,                   -- 'user', 'setting', 'rate_limit', etc.
    target_id       TEXT,                   -- UUID or identifier of the affected resource
    payload         JSONB,                  -- diff or relevant params; strip secrets before insert
    ip              INET,
    user_agent      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX audit_log_actor_created  ON audit_log (actor_id, created_at DESC);
CREATE INDEX audit_log_action_created ON audit_log (action, created_at DESC);
```

**Retention:** Keep 90 days of rows for a solo-dev deployment. Add a nightly `DELETE FROM audit_log WHERE created_at < now() - INTERVAL '90 days'` (pg_cron or a Fly.io scheduled machine). Revisit to 1 year if multi-user compliance is needed.

---

## 4. Top 3 Threats from Introducing an Admin Role

**Threat 1 — Admin token theft via XSS or storage leak.**
The single admin account is a global root. A stolen admin JWT yields unrestricted platform control.
Mitigation: keep access-token TTL short (15 min), enforce `HttpOnly + Secure + SameSite=Lax` cookies instead of localStorage for token storage (Phase 3 prerequisite), apply step-up auth (see §5) on all destructive admin endpoints.

**Threat 2 — Lateral escalation via account takeover.**
An attacker who compromises Caio's regular-user credentials gets admin too, because they're the same account.
Mitigation: enforce the login rate limit (§9), require step-up re-auth for admin mutations (§5), and log all auth events to `audit_log` so anomalous logins are detectable.

**Threat 3 — Accidental destructive action by the admin themselves.**
There is no other admin to notice or reverse mistakes; a misclick on "delete user" or "reset all blocks" has no approval gate.
Mitigation: require a confirmation token (see §5) for destructive endpoints; retain `payload_before` in `audit_log` so a developer can manually undo; soft-delete users rather than hard-delete in Phase 3.

---

## 5. Step-Up Auth for Sensitive Admin Ops

**Recommendation:** Require a short-lived `x-confirm-token` header on destructive admin endpoints, issued by `POST /api/admin/confirm` which re-checks the admin's password and returns a signed, single-use JWT valid for 5 minutes.

**Tradeoff:** Full MFA is out of scope; this is the simplest mechanism that adds a human gate without requiring a second device; it stops CSRF and accidental double-clicks.

Define "destructive" as: kill-switch toggle, user deletion, role grant/revoke, rate-limit global reset, LLM model change. Read and metrics endpoints are exempt. The confirm token should embed the specific action name in its claims so it can't be replayed against a different endpoint.

---

## 6. Kill-Switch Design

**Recommendation:** DB-backed flag in a `app_settings` key-value table; only the admin can toggle it; in-flight SSE streams receive a `{"type":"error","code":"service_disabled"}` event and are closed gracefully.

**Shape:**

```sql
CREATE TABLE app_settings (
    key        TEXT PRIMARY KEY,
    value      JSONB NOT NULL,
    updated_by UUID REFERENCES users(id),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- seed: INSERT INTO app_settings(key, value) VALUES ('chat_enabled', 'true');
```

The chat route reads `chat_enabled` from a cached in-memory flag (refreshed every 30 s via a background task) so the hot path has no extra DB query. Admin toggle writes to DB and invalidates the in-memory cache immediately. All other feature flags live in the same table.

**Tradeoff:** A config file or env-var flag is simpler to deploy but requires a redeploy to toggle; DB-backed enables live control from the admin UI without restart.

---

## 7. Impersonation / "View-As"

**Recommendation:** Defer full impersonation to a post-Phase-3 slice; Phase 3 ships read-only "view-as" only (admin can fetch another user's routine and conversation list but cannot write as them).

**If implemented:**
- Admin calls `POST /api/admin/impersonate/:user_id` with a valid confirm token (§5). Server returns a short-lived (15 min, non-renewable) impersonation JWT with claims `{ sub: target_user_id, impersonated_by: admin_id }`.
- Every request carrying an impersonation JWT writes to `audit_log` with `impersonating` set.
- The frontend checks the `impersonated_by` claim on token parse and renders a persistent red banner: "Viewing as [name] — impersonation active". The banner must not be dismissible.
- Impersonation tokens cannot be refreshed; the admin must re-authenticate to extend the session.

**Tradeoff:** Skipping write-as in Phase 3 eliminates the highest-risk impersonation surface while still providing the "see what the user sees" debugging value.

---

## 8. JWT Structure Changes

**Recommendation:** Add no new fields to JWT claims in Phase 3; resolve role from DB on each admin request (per §2).

**What changes:**
- The `load_user` helper in `middleware/auth.rs` must return `role` as part of the `User` model; `CurrentUser` gains a `role: String` field.
- `AdminUser` extractor is a new type wrapping `CurrentUser` with a role check — no JWT shape change.
- If impersonation (§7) ships, add `impersonated_by: Option<Uuid>` to a new `ImpersonationClaims` struct; issue a separate token type (`TokenKind::Impersonation`) and handle it in a dedicated extractor.

**Migration story for existing valid JWTs:** none required — claims shape is unchanged. Existing tokens continue to work; role is read fresh from DB on every admin-gated call.

---

## 9. Deferred Phase 2 Security Items

### Rate limit on `/auth/login`

**Recommendation:** Per-email sliding window, 10 attempts per 15-minute window, implemented with the existing `RateLimitState` pattern keyed on the normalized email string instead of `Uuid`.

**Tradeoff:** Per-IP is easier to implement but fails behind NAT (shared IP blocks legitimate users); per-email is more precise and directly targets credential stuffing without penalizing shared networks.

Concrete shape: add a second `RateLimitState<String>` field to `AppState` (or make `RateLimitState` generic over the key type). The login handler reads the `email` from the request body before attempting the DB lookup, checks the bucket, and returns 429 before doing the password comparison if the limit is exceeded. On a successful login, clear the bucket for that email.

### `tool_call` SSE args surface

**Recommendation:** Keep the current behavior (emit `args` verbatim) but add an explicit allowlist of safe-to-surface arg keys per tool; strip or redact any key not on the allowlist before emitting the SSE event.

**Tradeoff:** Verbatim emission is fine today because all tool args are user-owned data, but admin tools introduced in Phase 3 may include `user_id` references or internal identifiers that should not round-trip to the frontend.

Define the allowlist in `toolLabels.js` alongside the display labels that already exist there; the Rust side enforces it before writing the SSE event. This is a small surface hardening that costs one day of implementation.

---

## Summary

The three most load-bearing decisions in this plan are:

1. **DB-read role resolution (§2):** Do not bake `role` into the JWT. The correctness guarantee — a demoted admin loses access on the next request, not at token expiry — is worth the single extra DB query, and the infrastructure for it already exists in `load_user`.

2. **Confirm-token step-up auth (§5):** Destructive admin endpoints require a short-lived, action-scoped confirm token issued after password re-check. This is the primary control preventing both CSRF attacks and accidental destructive actions by the only admin account.

3. **`audit_log` table (§3):** Log every admin mutation and auth event with a denormalized `actor_email` column. The solo-dev context means there is no second person to notice anomalous activity; the audit log is the only forensic trail and the only undo path for admin-level operations that do not have the `routine_actions` undo model.
