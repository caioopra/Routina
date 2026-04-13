use axum::Router;
use sqlx::PgPool;

use crate::config::Config;

pub mod health;

pub fn create_router(pool: PgPool, config: Config) -> Router {
    Router::new()
        .nest("/health", health::router())
        .with_state(AppState { pool, config })
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
}
