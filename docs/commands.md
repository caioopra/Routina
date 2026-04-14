# Commands Reference

Quick lookup for the `Makefile` targets. Run everything from the repo root.

## First-time setup

| Command | What it does | When to run |
|---|---|---|
| `make install` | Installs frontend npm deps and compiles backend | Once after cloning, or after pulling changes that touch `Cargo.toml` / `package.json` |
| `make db-up` | Starts the Postgres container via `docker-compose` | Once per dev session (container persists until `db-down`) |
| `make migrate` | Applies pending `sqlx` migrations | After `db-up`, and any time new files appear in `backend/migrations/` |

Typical first run:
```bash
make install
make db-up
make migrate
```

## Daily development

| Command | What it does | When to run |
|---|---|---|
| `make dev` | Starts db + backend (`:3000`) + frontend (`:5173`) together; `Ctrl+C` stops both | Default way to work on the app |
| `make backend` | Runs only the Rust/Axum server | When you only need the API (e.g. hitting it with curl/Postman) |
| `make frontend` | Runs only the Vite dev server | When backend is already running elsewhere, or doing pure UI work against mocks |

Frontend proxies `/api` → backend, so always have the backend running when using the UI.

## Database

| Command | What it does | When to run |
|---|---|---|
| `make db-up` | Start Postgres | Start of session |
| `make db-down` | Stop Postgres (data preserved) | End of session / free resources |
| `make db-reset` | **Destroys** volume, recreates db, re-applies migrations | After breaking schema changes, or to reseed from scratch. Irreversible — you lose local data |
| `make migrate` | Apply new migrations without touching existing data | After adding a migration file |

## Testing & quality

| Command | What it does | When to run |
|---|---|---|
| `make test` | Runs backend + frontend test suites | Before committing / pushing |
| `make test-backend` | `cargo test` only | While iterating on Rust code |
| `make test-frontend` | `npm test` (Vitest) only | While iterating on React code |
| `make lint` | `cargo clippy -D warnings` + `cargo fmt --check` | Before pushing — CI enforces this |

## Build & deploy

| Command | What it does | When to run |
|---|---|---|
| `make build` | Release build of backend (`cargo build --release`) + frontend (`npm run build`) | Smoke-test a production build locally |
| `make deploy` | `fly deploy` to fly.io | Shipping to prod. Make sure `make test` and `make lint` pass first |

## Cleanup

| Command | What it does | When to run |
|---|---|---|
| `make clean` | Removes `backend/target`, `frontend/dist`, Vite cache | Reclaim disk space, or when builds act up |

## Discovery

```bash
make help    # prints every target with its description
```

## Cheat sheet

```bash
# fresh clone
make install && make db-up && make migrate && make dev

# normal morning
make dev

# before pushing
make lint && make test

# broke the schema
make db-reset

# ship it
make deploy
```
