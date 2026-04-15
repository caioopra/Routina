# Database Schema Reference

## Tables

### `users`
User accounts and preferences.
| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | `gen_random_uuid()` |
| email | TEXT UNIQUE | Login identifier |
| name | TEXT | Display name |
| password_hash | TEXT | argon2 hash |
| preferences | JSONB | `{ timezone, language, theme, llm_provider }` |
| planner_context | TEXT | Nullable; free-form "about me" text injected into every LLM system prompt |
| role | TEXT NOT NULL DEFAULT 'user' | CHECK: `'user'` \| `'admin'`; see Admin model below |
| created_at | TIMESTAMPTZ | |
| updated_at | TIMESTAMPTZ | |

### `routines`
Weekly schedule templates.
| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| user_id | UUID FK→users | |
| name | TEXT | e.g., "Abril 2026" |
| period | TEXT | Free-form period label |
| is_active | BOOLEAN | Only one active per user |
| meta | JSONB | `{ titulo, subtitulo, metaDoMes }` |
| created_at | TIMESTAMPTZ | |
| updated_at | TIMESTAMPTZ | |

**Indexes:** `(user_id, is_active)`

### `blocks`
Time blocks within a routine's day.
| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| routine_id | UUID FK→routines CASCADE | |
| day_of_week | SMALLINT | 0=Mon … 6=Sun (ISO) |
| start_time | TIME | |
| end_time | TIME | Nullable for open-ended |
| title | TEXT | |
| type | TEXT | Label name reference |
| note | TEXT | Optional context |
| sort_order | INT | Display ordering |
| created_at | TIMESTAMPTZ | |
| updated_at | TIMESTAMPTZ | |

**Indexes:** `(routine_id, day_of_week, sort_order)`

### `labels`
Activity type labels with colors.
| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| user_id | UUID FK→users | |
| name | TEXT | e.g., "trabalho" |
| color_bg | TEXT | Background hex |
| color_text | TEXT | Text hex |
| color_border | TEXT | Border hex |
| icon | TEXT | Emoji or icon name |
| is_default | BOOLEAN | System-provided, cannot delete |

**Unique:** `(user_id, name)`

### `block_labels`
Many-to-many: blocks ↔ labels.
| Column | Type |
|--------|------|
| block_id | UUID FK→blocks CASCADE |
| label_id | UUID FK→labels CASCADE |

**PK:** `(block_id, label_id)`

### `subtasks`
Checklist items within a block.
| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| block_id | UUID FK→blocks CASCADE | |
| title | TEXT | |
| is_completed | BOOLEAN | Default false |
| sort_order | INT | |

**Indexes:** `(block_id, sort_order)`

### `goals`
Hierarchical goals (month → quarter → semester → year).
| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| user_id | UUID FK→users | |
| title | TEXT | |
| description | TEXT | |
| scope | TEXT | 'month', 'quarter', 'semester', 'year' |
| target_date | DATE | |
| parent_id | UUID FK→goals(self) | Hierarchy |
| status | TEXT | 'active', 'completed', 'abandoned' |
| created_at | TIMESTAMPTZ | |
| updated_at | TIMESTAMPTZ | |

**Indexes:** `(user_id, scope, status)`

### `events`
Calendar events with reminders.
| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| user_id | UUID FK→users | |
| routine_id | UUID FK→routines | Nullable |
| title | TEXT | |
| description | TEXT | |
| starts_at | TIMESTAMPTZ | |
| ends_at | TIMESTAMPTZ | |
| recurrence | JSONB | Null for one-off |
| reminder_minutes | INT[] | e.g., `{15, 60}` |
| created_at | TIMESTAMPTZ | |

**Indexes:** `(user_id, starts_at)`

### `conversations`
AI chat sessions between a user and the LLM. One row per session.
`routine_id` is optional — when set the conversation is scoped to a specific routine (e.g. editing "Abril 2026"). `title` starts NULL and can be back-filled by the LLM after a few turns.

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | `gen_random_uuid()` |
| user_id | UUID FK→users CASCADE | Owner |
| title | TEXT | Nullable; LLM-generated summary |
| routine_id | UUID FK→routines SET NULL | Optional scope |
| created_at | TIMESTAMPTZ | |
| updated_at | TIMESTAMPTZ | |

**Indexes:** `(user_id, created_at DESC)` — list endpoint, `(routine_id)` — filter by routine

### `messages`
Every turn in a conversation, including tool-call turns.

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| conversation_id | UUID FK→conversations CASCADE | |
| role | TEXT | CHECK: `'user'`, `'assistant'`, `'system'`, `'tool'` |
| content | TEXT | Nullable — omitted when assistant turn is tool-calls only |
| tool_calls | JSONB | Assistant rows only; list of `{ id, name, args }` objects |
| tool_call_id | TEXT | `role='tool'` rows; references the assistant's `tool_calls[].id` |
| provider | TEXT | Nullable; `'gemini'` or `'claude'` on assistant rows |
| input_tokens | INT | Nullable; prompt tokens billed on this LLM turn |
| output_tokens | INT | Nullable; completion tokens billed on this LLM turn |
| model | TEXT | Nullable; exact model string returned by provider (e.g. `'gemini-2.0-flash'`) |
| created_at | TIMESTAMPTZ | |

Only assistant rows produced by an LLM call carry non-NULL token values. User, tool, and system rows — and all rows created before migration 005 — remain NULL (no backfill). Token data feeds the Slice C rollup queries and the Phase 3 admin metrics endpoint.

**Indexes:** `(conversation_id, created_at ASC)` — history fetch in chronological order

### Chat model — tool-call turns

When the LLM invokes a tool the conversation log stores two rows:

1. An `assistant` row whose `tool_calls` JSONB contains one entry per tool invoked:
   ```json
   [{ "id": "call_abc", "name": "create_block", "args": { "title": "Gym", ... } }]
   ```
   `content` may be NULL if the assistant produced no prose in that turn.

2. One `tool` row per result, with `tool_call_id` matching the assistant's `tool_calls[].id`:
   ```json
   role = "tool", tool_call_id = "call_abc", content = "Block created with id=..."
   ```

This mirrors the OpenAI / Gemini function-calling wire format and lets the backend replay the full conversation history when making follow-up LLM calls.

### Chat model — system prompt composition

On every conversation turn the backend builds the system prompt from three sources, in order:

1. **Static instructions** — tool schemas, formatting rules, persona.
2. **`users.planner_context`** — injected verbatim when non-NULL. This is the user's self-authored "about me" text (job, weekly intent, long-term goals). It gives the LLM standing personal context without requiring the user to restate it each session. Users can update this field at any time via the profile/settings UI; the next turn immediately reflects the change.
3. **Active routine state** — the current routine's blocks, labels, and rules serialised to a compact text representation, when a `routine_id` is set on the conversation.

`planner_context` is intentionally separate from `users.preferences` (which stores provider toggle and UI preferences) because it is narrative text consumed by the LLM, not a machine-readable config value.

### Admin model

Phase 3 introduces a single `role` column on `users` (CHECK-constrained to `'user'` or `'admin'`) instead of a separate roles table. This is intentional: the application has only two privilege levels, and a JOIN-free CHECK constraint is simpler to query and enforce.

- **Default:** every new user gets `role = 'user'` automatically; no application code needs to set it.
- **Promotion:** an existing admin promotes another user by issuing `UPDATE users SET role = 'admin' WHERE id = $1`. A dedicated API endpoint (Phase 3 Slice B) wraps this with authorization checks.
- **Partial index:** `users_role_idx` (added in migration 005) covers only the `role = 'admin'` rows. Because virtually all users are regular users the index stays tiny while making the admin-list query (`SELECT * FROM users WHERE role = 'admin'`) an index-only scan.
- **Audit log:** a full action audit log (migration 006, Phase 3 Slice B) will record admin operations separately from the per-conversation `routine_actions` log.

### `rules`
Monthly rules/guidelines for a routine.
| Column | Type |
|--------|------|
| id | UUID PK |
| routine_id | UUID FK→routines CASCADE |
| text | TEXT |
| sort_order | INT |

### `summary_entries`
Weekly hour distribution summary.
| Column | Type |
|--------|------|
| id | UUID PK |
| routine_id | UUID FK→routines CASCADE |
| label_id | UUID FK→labels |
| hours | TEXT |

**Indexes:** `(routine_id)`

### `routine_actions`
Audit log for every LLM tool-driven routine mutation. Written by the ToolExecutor on each `create_block`, `update_block`, `delete_block`, `create_rule`, `update_rule`, or `delete_rule` call. Read-only tool calls (e.g. `get_routine`) are not logged here.

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | `gen_random_uuid()` |
| user_id | UUID FK→users CASCADE | Owner; used for authorization checks |
| routine_id | UUID FK→routines CASCADE | The routine being mutated |
| conversation_id | UUID FK→conversations SET NULL | Nullable — rows survive conversation deletion |
| action_type | TEXT | CHECK: `'create_block'`, `'update_block'`, `'delete_block'`, `'create_rule'`, `'update_rule'`, `'delete_rule'` |
| target_id | UUID | The id of the block or rule that was mutated |
| payload_before | JSONB | Full row snapshot before mutation; NULL for create operations |
| payload_after | JSONB | Full row snapshot after mutation; NULL for delete operations |
| created_at | TIMESTAMPTZ | When the action was executed |
| undone_at | TIMESTAMPTZ | NULL until reversed by `undo_last_action`; never hard-deleted |

**Indexes:**
- `(conversation_id, created_at DESC) WHERE undone_at IS NULL` — primary undo query (partial index stays small)
- `(routine_id, created_at DESC)` — per-routine history / admin queries
- `(user_id)` — ownership checks

#### Undo model

Every tool-driven mutation writes one `routine_actions` row with `payload_before` and `payload_after` holding full JSONB snapshots of the affected row. When the user calls `undo_last_action` the backend:

1. Finds the newest row where `conversation_id = $1 AND undone_at IS NULL` (hits the partial index).
2. Reverses the mutation symmetrically:
   - `create_*` — deletes the row identified by `payload_after.id`.
   - `update_*` — writes `payload_before` back to the row identified by `target_id`.
   - `delete_*` — re-inserts `payload_before` (restores the deleted row).
3. Stamps `undone_at = now()` on the audit row rather than deleting it, preserving the full history.

Scoping undo to `conversation_id` prevents an `undo` in a new chat session from accidentally reversing an action taken in an older session.

## Relationships Diagram

```
users ──┬── routines ──┬── blocks ──┬── subtasks
        │              │            └── block_labels ── labels
        │              ├── rules
        │              └── summary_entries ── labels
        ├── labels
        ├── goals (self-referencing via parent_id)
        ├── events
        ├── conversations ── messages
        └── routine_actions ──┬── routines
                              └── conversations (nullable)
```
