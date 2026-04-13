# CLAUDE.md

## Overview

AI-Guided Planner — a full-stack application where users create and manage weekly routines through conversation with an LLM agent. Built with a Rust (Axum) backend and React frontend.

## Architecture

- **Backend:** Rust + Axum + PostgreSQL (sqlx) in `/backend/`
- **Frontend:** React 18 + Vite + Tailwind CSS v4 in `/frontend/`
- **LLM:** Multi-provider (Gemini primary, Claude secondary) via trait abstraction in `/backend/src/ai/`
- **Deployment:** fly.io with managed Postgres

## Commands

### Frontend
```bash
cd frontend
npm install          # install deps
npm run dev          # dev server at localhost:5173 (proxies /api to backend)
npm run build        # production build to dist/
npm test             # run Vitest tests
```

### Backend
```bash
cd backend
cargo build          # compile
cargo run            # start server at localhost:3000
cargo test           # run all tests
cargo clippy -- -D warnings  # lint (must pass with zero warnings)
cargo sqlx migrate run       # apply database migrations
```

### Infrastructure
```bash
docker-compose up -d           # start PostgreSQL (dev + test)
docker-compose down            # stop services
fly deploy                     # deploy to fly.io
```

## Project Structure

```
backend/
  src/
    main.rs              — Axum server, router
    config.rs            — env-based configuration
    db/                  — connection pool
    models/              — data structures (User, Routine, Block, etc.)
    routes/              — HTTP handlers (auth, routines, blocks, chat, ...)
    ai/                  — LLM provider trait, Gemini/Claude impls, tools, prompts
    middleware/           — auth (JWT), error handling
  migrations/            — SQL migrations (sqlx)
  tests/                 — integration tests
frontend/
  src/
    App.jsx               — Router setup
    stores/               — Zustand stores (auth, routine, chat)
    api/                   — API client + endpoint modules
    hooks/                 — Custom hooks (useMediaQuery, useSSE, useRoutine)
    components/            — UI components organized by domain
    pages/                 — Page-level components
docs/
  api.md                — REST API contract
  schema.md             — Database schema reference
```

## Agent Team

Development is organized via specialized agents in `.claude/agents/`:

| Agent | Scope | Role |
|-------|-------|------|
| `manager` | `/docs/**` | Orchestrates all agents, breaks features into tasks |
| `backend` | `/backend/**` | Rust/Axum routes, DB queries, auth |
| `frontend` | `/frontend/**` | React components, stores, Tailwind |
| `ai-prompt` | `/backend/src/ai/**` | LLM providers, prompts, tool schemas |
| `database` | `/backend/migrations/**` | SQL migrations, seeds, indexes |
| `infra` | `Dockerfile, CI, fly.toml` | Docker, CI/CD, deployment |
| `testing` | `**/tests/**, *.test.*` | Test suites, fixtures, coverage |

**Rule:** Each agent owns its scope. No agent writes outside its scope. The manager coordinates.

## Design System

Dark purple palette:
- Surface tokens: base (#08060f), surface (#0f0c1a), raised (#161227), overlay (#1e1836)
- Accent: #8b5cf6 (purple)
- Fonts: Outfit (display), DM Sans (body), JetBrains Mono (mono)

Block type colors defined in COLORS object — 7 types: trabalho, mestrado, aula, exercicio, slides, viagem, livre.

## Testing

**Every feature must have tests. No exceptions.**

- Backend: unit tests in-file (`#[cfg(test)]`), integration tests in `/backend/tests/`
- Frontend: Vitest + React Testing Library, MSW for API mocking
- CI: `cargo test` + `cargo clippy` + `npm test` + `npm run build` on every push

## Data

- `frontend/src/rotina.json` — Original static data (kept as reference/seed data)
- Database is the source of truth once backend is running
