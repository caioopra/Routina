.PHONY: help install db-up db-down db-reset migrate backend frontend dev test test-backend test-frontend lint build clean deploy

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

install: ## Install backend + frontend dependencies
	cd frontend && npm install
	cd backend && cargo build

db-up: ## Start PostgreSQL via docker-compose
	docker-compose up -d

db-down: ## Stop PostgreSQL
	docker-compose down

db-reset: ## Drop, recreate, and migrate the database
	docker-compose down -v
	docker-compose up -d
	sleep 2
	cd backend && cargo sqlx migrate run

migrate: ## Apply pending migrations
	cd backend && cargo sqlx migrate run

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

build: ## Production build for both sides
	cd backend && cargo build --release
	cd frontend && npm run build

clean: ## Remove build artifacts
	cd backend && cargo clean
	cd frontend && rm -rf dist node_modules/.vite

deploy: ## Deploy to fly.io
	fly deploy
