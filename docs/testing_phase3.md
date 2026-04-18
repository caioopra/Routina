# Testing Phase 3 — Manual QA Guide

Step-by-step guide to manually test the Phase 3 admin console, governance, and observability features locally.

## Prerequisites

- Docker running (for PostgreSQL)
- No LLM API key needed for admin console (only for chat/cost features)

## Step 1: Promote your user to admin

```bash
source backend/.env
psql "$DATABASE_URL" -c "UPDATE users SET role = 'admin' WHERE email = 'test@test.com';"
```

Current dev DB has one user: `test@test.com` with role `user`. No promote CLI exists — use the SQL directly.

## Step 2: Start the app

```bash
make dev
```

Backend on `:3000`, frontend on `:5173`.

## Step 3: Log in and visit admin

1. Go to `http://localhost:5173`, log in as `test@test.com`
2. Navigate to `http://localhost:5173/admin`

## What you'll see

| Page | What's there |
|------|-------------|
| **Dashboard** | 4 metric cards: Total Users, Monthly Cost, Active Provider, Chat Status |
| **Providers** | LLM settings form (default provider, Gemini/Claude models). Save requires password re-entry |
| **Users** | User table with "Set Rate Limit" action (also requires password re-entry) |
| **Audit Log** | Timeline of admin actions + auth events. Filter by action prefix, cursor pagination |

## Key features to test

| Feature | How |
|---------|-----|
| **Kill-switch** | Toggle on Dashboard → password modal → confirm → chat disabled system-wide |
| **Step-up auth** | Change any setting on Providers → password prompt before save |
| **Audit trail** | After any action, check Audit page — everything is logged |
| **Budget check** | Send chat messages (needs LLM API key) → token usage tracked per message |
| **Rate limiting** | Set rate limit on a user from Users page |
| **Role gating** | Log in as a non-admin user → `/admin` redirects to `/` |

## What needs an LLM API key

Chat features (token tracking, budget enforcement, cost metrics on dashboard) require `LLM_GEMINI_API_KEY` or `LLM_CLAUDE_API_KEY` in `.env`. Without them chat returns 503. The admin console itself works fully without keys.

## Quick start one-liner

```bash
source backend/.env && psql "$DATABASE_URL" -c "UPDATE users SET role = 'admin' WHERE email = 'test@test.com';" && make dev
```

Then open `http://localhost:5173/admin`.
