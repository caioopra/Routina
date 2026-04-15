use axum::Router;
use sqlx::PgPool;

use crate::config::Config;

pub mod auth;
pub mod blocks;
pub mod health;
pub mod labels;
pub mod routines;
pub mod rules;

pub fn create_router(pool: PgPool, config: Config) -> Router {
    let api = Router::new()
        .nest("/health", health::router())
        .nest("/auth", auth::router())
        .nest("/routines", routines::router())
        // Blocks nested under routines for list/create, flat for update/delete.
        .nest("/routines/{routine_id}/blocks", blocks::nested_router())
        .nest("/blocks", blocks::flat_router())
        // Rules nested under routines for list/create, flat for update/delete.
        .nest("/routines/{routine_id}/rules", rules::nested_router())
        .nest("/rules", rules::flat_router())
        // Labels are top-level, user-scoped.
        .nest("/labels", labels::router());

    Router::new()
        .nest("/api", api)
        .with_state(AppState { pool, config })
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
}
