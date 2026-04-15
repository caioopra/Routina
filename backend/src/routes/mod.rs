use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use sqlx::PgPool;
use uuid::Uuid;

use crate::ai::provider::LlmProvider;
use crate::config::Config;
use crate::middleware::auth::auth_middleware;
use crate::middleware::error::AppError;
use crate::middleware::rate_limit::{
    EmailRateLimitState, LOGIN_RATE_MAX, LOGIN_RATE_WINDOW_SECS, RateLimitState,
    rate_limit_middleware,
};

pub mod admin;
pub mod auth;
pub mod blocks;
pub mod chat;
pub mod conversations;
pub mod health;
pub mod labels;
pub mod me;
pub mod routines;
pub mod rules;
pub mod settings;

/// Default cap: 20 chat requests per user per minute.
pub const CHAT_RATE_LIMIT: usize = 20;

pub fn create_router(pool: PgPool, config: Config) -> Router {
    create_router_with_providers(pool, config, HashMap::new())
}

/// Build a router with a custom per-user chat rate limit — used in integration
/// tests so we can exercise the 429 path without sending 20+ requests.
pub fn create_router_with_rate_limit(
    pool: PgPool,
    config: Config,
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    chat_rate_limit: usize,
) -> Router {
    let state = AppState {
        pool,
        config,
        providers,
        rate_limit: RateLimitState::new(chat_rate_limit),
        login_rate_limit: EmailRateLimitState::new(LOGIN_RATE_MAX, LOGIN_RATE_WINDOW_SECS),
    };

    let chat_router =
        chat::router().layer(from_fn_with_state(state.clone(), rate_limit_middleware));

    let protected = Router::new()
        .nest("/routines", routines::router())
        .nest("/routines/{routine_id}/blocks", blocks::nested_router())
        .nest("/blocks", blocks::flat_router())
        .nest("/routines/{routine_id}/rules", rules::nested_router())
        .nest("/rules", rules::flat_router())
        .nest("/labels", labels::router())
        .nest("/conversations", conversations::router())
        .nest("/chat", chat_router)
        .nest("/me", me::router())
        .nest("/settings", settings::router())
        .layer(from_fn_with_state(state.clone(), auth_middleware));

    let admin = Router::new()
        .nest("/admin", admin::router())
        .layer(from_fn_with_state(state.clone(), auth_middleware));

    let public = Router::new()
        .nest("/health", health::router())
        .nest("/auth", auth::router());

    Router::new()
        .nest("/api", public.merge(protected).merge(admin))
        .with_state(state)
}

pub fn create_router_with_provider(
    pool: PgPool,
    config: Config,
    llm_provider: Option<Arc<dyn LlmProvider>>,
) -> Router {
    let mut providers = HashMap::new();
    if let Some(p) = llm_provider {
        providers.insert(p.name().to_string(), p);
    }
    create_router_with_providers(pool, config, providers)
}

pub fn create_router_with_providers(
    pool: PgPool,
    config: Config,
    providers: HashMap<String, Arc<dyn LlmProvider>>,
) -> Router {
    let state = AppState {
        pool,
        config,
        providers,
        rate_limit: RateLimitState::new(CHAT_RATE_LIMIT),
        login_rate_limit: EmailRateLimitState::new(LOGIN_RATE_MAX, LOGIN_RATE_WINDOW_SECS),
    };

    // Chat route with per-user rate limiting layered on top.
    // `rate_limit_middleware` reads `state.rate_limit` directly.
    let chat_router =
        chat::router().layer(from_fn_with_state(state.clone(), rate_limit_middleware));

    // Protected sub-router: all authenticated endpoints wrapped with the
    // router-level auth middleware.  Even a handler that accidentally omits the
    // `CurrentUser` extractor cannot be reached without a valid Access token.
    let protected = Router::new()
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
        // Chat SSE (rate-limited, then auth-gated)
        .nest("/chat", chat_router)
        // Me (planner context)
        .nest("/me", me::router())
        // Settings (provider toggle)
        .nest("/settings", settings::router())
        .layer(from_fn_with_state(state.clone(), auth_middleware));

    // Admin sub-router: auth_middleware blocks unauthenticated callers before
    // they reach the AdminUser extractor, which then checks the role.
    let admin = Router::new()
        .nest("/admin", admin::router())
        .layer(from_fn_with_state(state.clone(), auth_middleware));

    // Public sub-router: no auth required.
    let public = Router::new()
        .nest("/health", health::router())
        .nest("/auth", auth::router());

    Router::new()
        .nest("/api", public.merge(protected).merge(admin))
        .with_state(state)
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    /// Map of provider name → Arc<dyn LlmProvider>.
    /// Keys that are missing from the map indicate the provider key was not
    /// configured at startup.  The chat handler reads the user's selected
    /// provider from `users.preferences` and falls back to the first available
    /// if the selection is unavailable.
    pub providers: HashMap<String, Arc<dyn LlmProvider>>,
    /// Per-user sliding-window rate limiter for the chat endpoint.
    pub rate_limit: RateLimitState,
    /// Per-email sliding-window rate limiter for the login endpoint.
    /// Keyed on the normalized (lowercase, trimmed) email address.
    pub login_rate_limit: EmailRateLimitState,
}

impl AppState {
    /// Returns the provider to use for the given user preference, falling back
    /// to the first available provider if the preferred one is unavailable.
    pub fn resolve_provider(&self, preferred: Option<&str>) -> Option<Arc<dyn LlmProvider>> {
        // Try preferred provider first.
        if let Some(name) = preferred
            && let Some(p) = self.providers.get(name)
        {
            return Some(p.clone());
        }
        // Fall back to first available (deterministic: sorted by key name).
        let mut keys: Vec<&String> = self.providers.keys().collect();
        keys.sort();
        keys.first().and_then(|k| self.providers.get(*k)).cloned()
    }
}

// Manual Debug impl because `dyn LlmProvider` doesn't implement Debug.
impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.providers.keys().map(String::as_str).collect();
        f.debug_struct("AppState")
            .field("pool", &self.pool)
            .field("config", &self.config)
            .field("providers", &names)
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
