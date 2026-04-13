# Routina

AI-guided weekly planner — build and manage your routine through conversation.

## What is this?

Routina helps you create and manage weekly routines through conversation with an AI agent. Tell it about your work, classes, exercise habits, and goals — it builds your schedule. Need to adjust? Just chat with it or edit manually.

## Tech Stack

- **Frontend:** React 18 + Vite + Tailwind CSS v4
- **Backend:** Rust (Axum) + PostgreSQL
- **AI:** Multi-provider (Gemini, Claude) with tool-use for real-time schedule modifications

## Local Development

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) (v20+)
- [Docker](https://www.docker.com/) (for PostgreSQL)

### Setup

```bash
# Start PostgreSQL
docker-compose up -d

# Copy env config
cp .env.example .env
# Edit .env with your settings (database URL, API keys, etc.)

# Backend
cd backend
cargo run

# Frontend (in another terminal)
npm install
npm run dev
```

Open `http://localhost:5173`.

## Project Structure

```
backend/       Rust/Axum API server
  src/
    routes/    HTTP handlers
    models/    Data structures
    ai/        LLM provider integrations
    middleware/ Auth, error handling
  migrations/  PostgreSQL schema
src/           React frontend
docs/          API contract & schema reference
```

## License

MIT
