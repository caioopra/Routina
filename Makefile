.PHONY: help install db-up db-down db-reset migrate prepare backend frontend dev test test-backend test-frontend lint build clean deploy check-backend check-frontend fullcheck

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

install: ## Install backend + frontend dependencies
	cd frontend && npm install
	cd backend && cargo build

DOCKER_COMPOSE ?= docker compose

db-up: ## Start PostgreSQL via docker compose
	$(DOCKER_COMPOSE) up -d

db-down: ## Stop PostgreSQL
	$(DOCKER_COMPOSE) down

db-reset: ## Drop, recreate, and migrate the database
	$(DOCKER_COMPOSE) down -v
	$(DOCKER_COMPOSE) up -d
	sleep 2
	cd backend && cargo sqlx migrate run

migrate: ## Apply pending migrations
	cd backend && cargo sqlx migrate run

prepare: ## Regenerate sqlx offline query metadata (.sqlx/); run after changing any sqlx::query! macro
	cd backend && cargo sqlx prepare --workspace -- --all-targets

backend: ## Run the Rust/Axum backend (localhost:3000)
	cd backend && cargo run

frontend: ## Run the Vite dev server (localhost:5173)
	cd frontend && npm run dev

dev: ## Start db + backend + frontend together (Ctrl+C to stop)
	$(MAKE) db-up
	@echo "Starting backend and frontend..."
	@trap 'kill 0' INT; \
		(cd backend && cargo run) & \
		(cd frontend && npm run dev) & \
		wait

test: test-backend test-frontend ## Run all tests

test-backend: ## Run backend tests
	cd backend && cargo test

test-frontend: ## Run frontend tests
	cd frontend && npm test

lint: ## Run clippy (zero warnings) + prettier check
	cd backend && cargo clippy -- -D warnings
	cd backend && cargo fmt --check

check-backend: ## Mirror backend CI: fmt + clippy + sqlx-prepare + tests (requires `make db-up`)
	cd backend && cargo fmt -- --check
	cd backend && SQLX_OFFLINE=true cargo clippy -- -D warnings
	cd backend && SQLX_OFFLINE=false cargo sqlx prepare --workspace --check -- --all-targets
	cd backend && SQLX_OFFLINE=true cargo test

check-frontend: ## Mirror frontend CI: prettier + tests + build
	cd frontend && npm run check

fullcheck: check-backend check-frontend ## Run backend + frontend CI checks locally (DB must be running)

build: ## Production build for both sides
	cd backend && cargo build --release
	cd frontend && npm run build

clean: ## Remove build artifacts
	cd backend && cargo clean
	cd frontend && rm -rf dist node_modules/.vite

deploy: ## Deploy to fly.io
	fly deploy

preview: ## Regenerate the standalone rotina.json preview at temp/index.html
	./scripts/build-preview.sh
