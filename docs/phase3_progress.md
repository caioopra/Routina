# Phase 3 Progress & Resume Guide

**Last session:** 2026-04-15 (paused mid-Slice A between Feature 1 and Feature 2).
**Plan:** `docs/phase3_plan.md` (5 slices A–E, decisions already resolved in §4).
**Specialist briefs:** `docs/phase3_planning/{security,ai_governance,observability,data_model,admin_ui}.md`.

---

## Current git state

- `main` — stable baseline (Phase 2 shipped, 9 commits ahead of last-known `origin/main`).
- `develop` — Phase 3 work branch. Contains migration 005 + Feature 1 merged via `--no-ff`.
- No active feature branch. Delete any stale ones before resuming.

To resume, checkout develop and create the next feature branch:

```bash
git checkout develop
git status            # confirm clean
git log --oneline -5  # confirm you see the merge of phase 3 slice A feature 1
```

---

## Workflow rules (agreed this session — keep using them)

1. **Branch per feature**, not per slice. Name: `feat/phase3-<slice>-<short-name>`.
2. **Serial**, not parallel — one feature branch at a time inside the shared working tree (no worktree isolation for sub-agents; memory `feedback_agent_team.md` flags that as unreliable).
3. Per feature cycle:
   1. Branch off `develop`.
   2. Spawn **implementer** (backend / frontend / database / ai-prompt / infra).
   3. Spawn **reviewer(s)** in parallel against the branch diff (`code-reviewer` always; `security-reviewer` for auth/sensitive surfaces).
   4. If findings: spawn a **fix-wave** agent on the same branch.
   5. Run verification: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `npm test -- --run`, `npm run build`. All must pass.
   6. `git merge --no-ff <branch>` into develop; delete the branch.
4. **Semantic conventional commits** on every commit (feat/fix/chore/test/docs/refactor).
5. **Stop at end of slice** for user decision on whether to proceed to next slice or merge develop → main.
6. **LLM API keys NOT required** for any Phase 3 development — all tests use `MockLlmProvider`.

---

## Slice A — Role Infrastructure

**Status:** partially shipped. Migration + Feature 1 on `develop`. Feature 2 not started.

### Shipped on `develop`

**Migration 005** (`47acecf` on main, inherited by develop)
- `users.role TEXT NOT NULL DEFAULT 'user' CHECK (...)` + partial index on `role='admin'`.
- `messages.input_tokens`, `messages.output_tokens`, `messages.model` — all nullable.
- `docs/schema.md` updated.

**Feature 1 — Backend role infrastructure** (merged `8fea2c4` on develop)
- `CurrentUser.role: String` (auth.rs extended).
- `AdminUser` extractor in `middleware/admin.rs` — returns 403 via `AppError::Forbidden.into_response()`.
- `/api/admin/dashboard` stub in `routes/admin.rs` as proof-of-gating.
- Per-email login rate-limit: 10 attempts / 15-min sliding window, clears on success, opportunistic sweep every 100 calls to bound memory.
- Login timing-equalization via dummy argon2 hash (`OnceLock`) to prevent email enumeration.
- Tests: `admin_route_tests.rs` (5), `auth_rate_limit_tests.rs` (6), `login_timing_equalized_for_unknown_email` in `auth_tests.rs`.
- Reviewed by `code-reviewer` + `security-reviewer`; all findings fixed in-branch.
- 387 backend tests, zero clippy warnings.

### Pending — Feature 2: frontend role gating

**Branch to create:** `feat/phase3-a-frontend-role` (branch from `develop`).

**Scope** (see `docs/phase3_plan.md` §3 Slice A deliverables):

1. `frontend/src/stores/authStore.js`
   - Add `role: null` to initial state.
   - Populate `role` from `/api/auth/me` response in whichever action loads the profile (`loadMe` / `fetchProfile` / similar — grep for `/auth/me` calls).
   - Expose a `useIsAdmin()` selector or equivalent convenience.
2. `frontend/src/components/admin/AdminRoute.jsx` (new file + test)
   - Wrapper component: if `role !== 'admin'`, redirect to `/` (react-router `<Navigate>`).
   - If `role` is still `null` (loading), render a small loading state rather than flashing a redirect.
   - Export as default.
3. Route wiring in `App.jsx` (or wherever routes live)
   - Stub `/admin/dashboard` route rendering a minimal `<AdminDashboard>` placeholder page wrapped in `<AdminRoute>`. The actual dashboard content lands in Slice D; this slice only proves the gate works.
4. MSW handler update (`frontend/src/test/mocks/handlers.js`)
   - `GET /api/auth/me` mock must include `role` in its response body (default `"user"`; expose a way in tests to set it to `"admin"`).

**Tests:**
- `AdminRoute.test.jsx`: `role === 'user'` → redirects to `/`. `role === 'admin'` → renders children. `role === null` → renders loading state.
- `authStore.test.js`: `loadMe` action stores `role` from the response.
- Existing 198 frontend tests must stay green.

**Review:** `code-reviewer` only (no security-sensitive surface; backend handles authz — frontend gating is UX-only).

### Dispatch prompt (copy-paste ready for next session)

Use `subagent_type: frontend`. Prompt body:

> Phase 3 Slice A Feature 2 — frontend role gating. You are on branch `feat/phase3-a-frontend-role` (already checked out). Commit with semantic messages; a code-reviewer will inspect the branch diff.
>
> ### Scope
>
> 1. **authStore** (`frontend/src/stores/authStore.js`): add `role: null` to initial state. Populate `role` from `/api/auth/me` response in the existing profile-loading action. Add a `useIsAdmin()` selector that returns `role === 'admin'`.
> 2. **AdminRoute** (`frontend/src/components/admin/AdminRoute.jsx` — new): wrap children. If `role === null` render a minimal loading state; if `role !== 'admin'` redirect to `/` via `<Navigate replace>`. If admin, render children.
> 3. **App.jsx**: add a `/admin/dashboard` route wrapped in `<AdminRoute>`, rendering a minimal placeholder. Actual pages land in Slice D.
> 4. **MSW handler** (`frontend/src/test/mocks/handlers.js`): `GET /api/auth/me` mock response must include a `role` field (default `"user"`, expose a way to flip to `"admin"` for tests).
>
> ### Tests
>
> - `frontend/src/components/admin/AdminRoute.test.jsx`: redirects on user, renders on admin, loading state on null.
> - `frontend/src/stores/authStore.test.js`: `loadMe` action stores role from response.
> - Existing 198 tests stay green.
>
> ### Verify
>
> `npm test -- --run`, `npm run build`, prettier clean.
>
> ### Commits
>
> Semantic; one or two commits OK.
>
> ### Report
>
> Files changed, test count delta.

After implementer completes, dispatch `code-reviewer` with this prompt:

> Review branch `feat/phase3-a-frontend-role` (diff against `develop`). Under 300 words. Focus: role hydration races (what if `/auth/me` hasn't returned yet and the user navigates to `/admin/dashboard` directly?), redirect correctness, MSW mock shape, missing loading state flashing the redirect. Output the usual verdict/critical/important/nit/praise structure.

If findings, spawn backend/frontend agent for fix-wave as in Feature 1. Then verify + merge `--no-ff` into develop + delete branch.

---

## After Slice A finishes

Before starting Slice B, surface to the user:
- What landed (tests, clippy, commits).
- Whether to merge `develop → main` now or at end of Phase 3.
- Slice B kickoff requires user confirmation (audit log + step-up auth is bigger scope).

Slice B summary for context (read full detail in `docs/phase3_plan.md` §3):
- Migration 006 for `audit_log` table.
- `emit_audit` helper + integration into admin mutation handlers.
- `POST /api/admin/confirm` → short-lived action-scoped confirm JWT (5 min TTL).
- All destructive admin endpoints require `x-confirm-token` header.
- Auth event logging (login success/fail, refresh).
- `GET /api/admin/audit` paginated reader.
- Reviewed by `security-reviewer` (primary) + `code-reviewer`.

Subsequent: Slice C (cost tracking + runtime config), Slice D (admin console UI), Slice E (observability wiring).

---

## Useful commands

```bash
# Resume
git checkout develop && git pull          # pull if origin has moved
git log --oneline --graph -10              # confirm state
make migrate                               # ensure local DB matches
make test                                  # sanity check

# Start Feature 2
git checkout -b feat/phase3-a-frontend-role develop
# spawn frontend agent with the prompt above

# After review+fixes
cd frontend && npm test -- --run && npm run build
cd ../backend && cargo test && cargo clippy --all-targets -- -D warnings
git checkout develop
git merge --no-ff feat/phase3-a-frontend-role -m "merge: phase 3 slice A feature 2 — frontend role gating"
git branch -d feat/phase3-a-frontend-role
```

---

## Things to watch for

- **The `.sqlx/` directory** must be regenerated with `make prepare` any time a `sqlx::query!` macro is added or changed. Pre-commit hook enforces this.
- **Prettier** on frontend will silently rewrite files on save but the pre-commit hook will block you with a warning; run `cd frontend && npm run format` before committing.
- **No branch pushes without explicit approval** — session stopped at feature-2 start, no push yet of `develop` or merged work. `main` is also unpushed from a prior session.
