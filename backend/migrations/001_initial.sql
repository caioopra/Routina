-- Phase 1: Core tables for users, routines, blocks, labels, rules, and summary entries

CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Users
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    preferences JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Routines (weekly schedule templates)
CREATE TABLE routines (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    period TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true,
    meta JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_routines_user_active ON routines(user_id, is_active);

-- Labels (activity type categories with colors)
CREATE TABLE labels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    color_bg TEXT NOT NULL,
    color_text TEXT NOT NULL,
    color_border TEXT NOT NULL,
    icon TEXT,
    is_default BOOLEAN NOT NULL DEFAULT false,
    UNIQUE(user_id, name)
);

-- Blocks (time slots within a routine's day)
CREATE TABLE blocks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    day_of_week SMALLINT NOT NULL CHECK (day_of_week BETWEEN 0 AND 6),
    start_time TIME NOT NULL,
    end_time TIME,
    title TEXT NOT NULL,
    type TEXT NOT NULL,
    note TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_blocks_routine_day ON blocks(routine_id, day_of_week, sort_order);

-- Block-label associations (many-to-many)
CREATE TABLE block_labels (
    block_id UUID NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    PRIMARY KEY (block_id, label_id)
);

-- Subtasks (checklist items within a block)
CREATE TABLE subtasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    block_id UUID NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    is_completed BOOLEAN NOT NULL DEFAULT false,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_subtasks_block ON subtasks(block_id, sort_order);

-- Goals (hierarchical: month → quarter → semester → year)
CREATE TABLE goals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT,
    scope TEXT NOT NULL CHECK (scope IN ('month', 'quarter', 'semester', 'year')),
    target_date DATE,
    parent_id UUID REFERENCES goals(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed', 'abandoned')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_goals_user_scope ON goals(user_id, scope, status);

-- Events (calendar events with reminders)
CREATE TABLE events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    routine_id UUID REFERENCES routines(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    description TEXT,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ,
    recurrence JSONB,
    reminder_minutes INTEGER[],
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_events_user_start ON events(user_id, starts_at);

-- Rules (monthly guidelines for a routine)
CREATE TABLE rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);

-- Summary entries (weekly hour distribution)
CREATE TABLE summary_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    hours TEXT NOT NULL
);

CREATE INDEX idx_summary_routine ON summary_entries(routine_id);
