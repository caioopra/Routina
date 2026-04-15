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
| created_at | TIMESTAMPTZ | |

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

## Relationships Diagram

```
users ──┬── routines ──┬── blocks ──┬── subtasks
        │              │            └── block_labels ── labels
        │              ├── rules
        │              └── summary_entries ── labels
        ├── labels
        ├── goals (self-referencing via parent_id)
        ├── events
        └── conversations ── messages
```
