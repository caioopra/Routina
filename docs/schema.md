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
AI chat conversations.
| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| user_id | UUID FK→users | |
| purpose | TEXT | 'onboarding', 'routine_edit', 'general' |
| routine_id | UUID FK→routines | Nullable |
| created_at | TIMESTAMPTZ | |
| updated_at | TIMESTAMPTZ | |

**Indexes:** `(user_id, created_at DESC)`

### `messages`
Individual messages in conversations.
| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| conversation_id | UUID FK→conversations CASCADE | |
| role | TEXT | 'user', 'assistant', 'system', 'tool_use', 'tool_result' |
| content | TEXT | |
| tool_calls | JSONB | If assistant used tools |
| tool_results | JSONB | If this is a tool result |
| created_at | TIMESTAMPTZ | |

**Indexes:** `(conversation_id, created_at)`

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
