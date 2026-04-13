# API Contract

Base URL: `/api` (proxied from Vite dev server in development)

## Authentication

### `POST /auth/register`
Create a new user account.
- **Request:** `{ email, name, password }`
- **Response:** `{ user: { id, email, name }, token, refresh_token }`
- **Errors:** 409 (email exists), 422 (validation)

### `POST /auth/login`
Authenticate with email/password.
- **Request:** `{ email, password }`
- **Response:** `{ user: { id, email, name }, token, refresh_token }`
- **Errors:** 401 (invalid credentials)

### `POST /auth/refresh`
Refresh an expired JWT.
- **Request:** `{ refresh_token }`
- **Response:** `{ token, refresh_token }`
- **Errors:** 401 (invalid/expired refresh token)

### `GET /auth/me`
Get current user profile.
- **Headers:** `Authorization: Bearer <token>`
- **Response:** `{ id, email, name, preferences }`
- **Errors:** 401 (unauthorized)

---

## Routines

All routine endpoints require `Authorization: Bearer <token>`.

### `GET /api/routines`
List user's routines.
- **Response:** `[{ id, name, period, is_active, meta, created_at }]`

### `POST /api/routines`
Create a new routine.
- **Request:** `{ name, period?, meta? }`
- **Response:** `{ id, name, period, is_active, meta, created_at }`

### `GET /api/routines/:id`
Get routine with all blocks.
- **Response:** `{ id, name, period, is_active, meta, blocks: [...], rules: [...], summary: [...] }`

### `PUT /api/routines/:id`
Update routine metadata.
- **Request:** `{ name?, period?, is_active?, meta? }`

### `DELETE /api/routines/:id`
Delete routine and all associated data.

---

## Blocks

### `GET /api/routines/:id/blocks`
List blocks for a routine, grouped by day.
- **Query:** `?day=0` (optional filter by day_of_week)
- **Response:** `[{ id, day_of_week, start_time, end_time, title, type, note, sort_order, subtasks: [...], labels: [...] }]`

### `POST /api/routines/:id/blocks`
Create a new block.
- **Request:** `{ day_of_week, start_time, end_time?, title, type, note?, sort_order? }`

### `PUT /api/blocks/:id`
Update a block.
- **Request:** `{ day_of_week?, start_time?, end_time?, title?, type?, note?, sort_order? }`

### `DELETE /api/blocks/:id`
Delete a block.

---

## Labels

### `GET /api/labels`
List user's labels (including defaults).

### `POST /api/labels`
Create a custom label.
- **Request:** `{ name, color_bg, color_text, color_border, icon? }`

### `PUT /api/labels/:id`
Update a label.

### `DELETE /api/labels/:id`
Delete a custom label (cannot delete defaults).

---

## Rules

### `GET /api/routines/:id/rules`
### `POST /api/routines/:id/rules`
- **Request:** `{ text, sort_order? }`
### `PUT /api/rules/:id`
### `DELETE /api/rules/:id`

---

## Chat (Phase 2)

### `POST /api/chat/message`
Send a message and receive streaming AI response.
- **Request:** `{ conversation_id?, message, routine_id? }`
- **Response:** SSE stream with events:
  - `event: token` — `{ data: "text chunk" }`
  - `event: tool_call` — `{ data: { tool: "create_block", args: {...} } }`
  - `event: routine_updated` — `{ data: { routine: {...} } }`
  - `event: provider` — `{ data: { name: "gemini" } }`
  - `event: done` — `{}`

### `GET /api/conversations`
List user's conversations.

### `GET /api/conversations/:id/messages`
Get messages for a conversation.

---

## Goals (Phase 3)

### `GET /api/goals`
- **Query:** `?scope=month&status=active`
### `POST /api/goals`
- **Request:** `{ title, description?, scope, target_date?, parent_id? }`
### `PUT /api/goals/:id`
### `DELETE /api/goals/:id`

---

## Sub-tasks (Phase 3)

### `GET /api/blocks/:id/subtasks`
### `POST /api/blocks/:id/subtasks`
- **Request:** `{ title, sort_order? }`
### `PUT /api/subtasks/:id`
- **Request:** `{ title?, is_completed?, sort_order? }`
### `DELETE /api/subtasks/:id`

---

## Events (Phase 3)

### `GET /api/events`
- **Query:** `?from=2026-04-01&to=2026-04-30`
### `POST /api/events`
- **Request:** `{ title, description?, starts_at, ends_at?, recurrence?, reminder_minutes? }`
### `PUT /api/events/:id`
### `DELETE /api/events/:id`

---

## Settings (Phase 2)

### `GET /api/settings/providers`
List available LLM providers and current selection.

### `PUT /api/settings/provider`
- **Request:** `{ provider: "gemini" | "claude" }`
