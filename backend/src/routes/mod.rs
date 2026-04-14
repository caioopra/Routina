use axum::Router;
use sqlx::PgPool;

use crate::config::Config;

pub mod auth;
pub mod health;
pub mod routines;

pub fn create_router(pool: PgPool, config: Config) -> Router {
    let api = Router::new()
        .nest("/health", health::router())
        .nest("/auth", auth::router())
        .nest("/routines", routines::router());

    Router::new()
        .nest("/api", api)
        .with_state(AppState { pool, config })
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
}
