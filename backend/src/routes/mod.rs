use std::sync::Arc;

use axum::Router;
use sqlx::PgPool;
use uuid::Uuid;

use crate::ai::provider::LlmProvider;
use crate::config::Config;
use crate::middleware::error::AppError;

pub mod auth;
pub mod blocks;
pub mod chat;
pub mod conversations;
pub mod health;
pub mod labels;
pub mod me;
pub mod routines;
pub mod rules;

pub fn create_router(pool: PgPool, config: Config) -> Router {
    create_router_with_provider(pool, config, None)
}

pub fn create_router_with_provider(
    pool: PgPool,
    config: Config,
    llm_provider: Option<Arc<dyn LlmProvider>>,
) -> Router {
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
        .nest("/labels", labels::router())
        // Conversations
        .nest("/conversations", conversations::router())
        // Chat SSE
        .nest("/chat", chat::router())
        // Me (planner context)
        .nest("/me", me::router());

    Router::new().nest("/api", api).with_state(AppState {
        pool,
        config,
        llm_provider,
    })
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    /// Shared LLM provider instance.  `None` when `GEMINI_API_KEY` is absent at
    /// startup — the chat handler returns 503 in that case rather than panicking.
    pub llm_provider: Option<Arc<dyn LlmProvider>>,
}

// Manual Debug impl because `dyn LlmProvider` doesn't implement Debug.
impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("pool", &self.pool)
            .field("config", &self.config)
            .field(
                "llm_provider",
                &self.llm_provider.as_ref().map(|p| p.name()),
            )
            .finish()
    }
}

/// Verifies that the routine exists and belongs to the given user.
/// Returns `AppError::NotFound` if not owned or not found, to avoid leaking
/// information about the existence of other users' routines.
pub(super) async fn verify_routine_owned(
    pool: &PgPool,
    user_id: Uuid,
    routine_id: Uuid,
) -> Result<(), AppError> {
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM routines WHERE id = $1 AND user_id = $2")
            .bind(routine_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;

    if exists.is_none() {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Returns `AppError::Validation` if `value` exceeds `max_len` bytes.
/// Use for free-text fields to prevent excessively large payloads.
pub(super) fn validate_length(
    field_name: &str,
    value: &str,
    max_len: usize,
) -> Result<(), AppError> {
    if value.len() > max_len {
        return Err(AppError::Validation(format!(
            "{field_name} must be at most {max_len} characters"
        )));
    }
    Ok(())
}
