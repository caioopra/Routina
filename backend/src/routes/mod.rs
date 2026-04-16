use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::middleware::from_fn_with_state;
use dashmap::DashMap;
use sqlx::PgPool;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::ai::provider::LlmProvider;
use crate::config::Config;
use crate::middleware::auth::auth_middleware;
use crate::middleware::error::AppError;
use crate::middleware::rate_limit::{
    EmailRateLimitState, LOGIN_RATE_MAX, LOGIN_RATE_WINDOW_SECS, RateLimitState,
    rate_limit_middleware,
};

/// Maximum step-up confirm attempts per admin user per window.
pub const CONFIRM_RATE_MAX: usize = 5;
/// Sliding-window duration for the confirm rate limiter (5 minutes).
pub const CONFIRM_RATE_WINDOW_SECS: u64 = 300;

// ── SettingsCache ─────────────────────────────────────────────────────────────

/// How long the in-memory settings cache is considered fresh.
const SETTINGS_CACHE_TTL: Duration = Duration::from_secs(60);

/// Inner state of the settings cache, kept under a single lock so that the
/// map update and timestamp update are always atomic with respect to each other.
struct CacheInner {
    map: HashMap<String, String>,
    refreshed_at: Instant,
}

/// In-memory cache for `app_settings` rows with a 60-second TTL.
///
/// Uses a single `tokio::sync::RwLock<CacheInner>` so the timestamp and map
/// are always updated atomically, eliminating the split-lock race present when
/// two separate locks are used.
///
/// The `get()` method uses the double-checked locking pattern: it acquires a
/// read lock first (fast path), and only upgrades to a write lock when the
/// cache is actually stale (slow path), re-checking staleness after acquiring
/// the write lock to prevent thundering-herd refreshes.
#[derive(Clone)]
pub struct SettingsCache {
    inner: Arc<tokio::sync::RwLock<CacheInner>>,
}

impl Default for SettingsCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsCache {
    /// Create an empty cache that will refresh on first use.
    pub fn new() -> Self {
        // Set refreshed_at to a point far in the past so the first call always
        // triggers a DB load.
        let stale = Instant::now()
            .checked_sub(SETTINGS_CACHE_TTL + Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(CacheInner {
                map: HashMap::new(),
                refreshed_at: stale,
            })),
        }
    }

    /// Return the value for `key`, refreshing from the DB when the cache is stale.
    ///
    /// Uses double-checked locking:
    /// 1. Acquire read lock; if fresh, return immediately (common path).
    /// 2. Drop read lock, acquire write lock, re-check staleness.
    /// 3. Only the first writer refreshes; subsequent writers see a fresh cache
    ///    after the first writer is done and skip the DB call.
    pub async fn get(&self, pool: &PgPool, key: &str) -> Option<String> {
        // Fast path: read lock, check if still fresh.
        {
            let guard = self.inner.read().await;
            if guard.refreshed_at.elapsed() <= SETTINGS_CACHE_TTL {
                return guard.map.get(key).cloned();
            }
        }
        // Slow path: cache is stale.  Upgrade to write lock and re-check so
        // only one concurrent caller performs the DB refresh.
        {
            let mut guard = self.inner.write().await;
            // Re-check: another task may have refreshed while we waited.
            if guard.refreshed_at.elapsed() > SETTINGS_CACHE_TTL {
                Self::refresh_inner(&mut guard, pool).await;
            }
            guard.map.get(key).cloned()
        }
    }

    /// Force the cache to expire so the next `get` call reloads from the DB.
    pub async fn invalidate(&self) {
        let stale = Instant::now()
            .checked_sub(SETTINGS_CACHE_TTL + Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        let mut guard = self.inner.write().await;
        guard.refreshed_at = stale;
    }

    /// Reload all `app_settings` rows from the DB into the provided inner guard.
    ///
    /// Must be called while holding the write lock so the map and timestamp
    /// update atomically.
    async fn refresh_inner(inner: &mut CacheInner, pool: &PgPool) {
        let rows: Result<Vec<(String, String)>, _> =
            sqlx::query_as::<_, (String, String)>("SELECT key, value FROM app_settings")
                .fetch_all(pool)
                .await;

        match rows {
            Ok(pairs) => {
                inner.map.clear();
                for (k, v) in pairs {
                    inner.map.insert(k, v);
                }
                inner.refreshed_at = Instant::now();
            }
            Err(e) => {
                tracing::error!(error = ?e, "SettingsCache: failed to refresh from DB");
                // Leave the existing (stale) data in place so reads still work.
                // Advance the timestamp by a short interval so we don't hammer
                // the DB on every request when the DB is unavailable.
                inner.refreshed_at = Instant::now()
                    .checked_sub(
                        SETTINGS_CACHE_TTL
                            .checked_sub(Duration::from_secs(5))
                            .unwrap_or_default(),
                    )
                    .unwrap_or_else(Instant::now);
            }
        }
    }
}

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
        confirm_rate_limit: EmailRateLimitState::new(CONFIRM_RATE_MAX, CONFIRM_RATE_WINDOW_SECS),
        settings_cache: SettingsCache::new(),
        chat_semaphores: Arc::new(DashMap::new()),
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
        confirm_rate_limit: EmailRateLimitState::new(CONFIRM_RATE_MAX, CONFIRM_RATE_WINDOW_SECS),
        settings_cache: SettingsCache::new(),
        chat_semaphores: Arc::new(DashMap::new()),
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
    /// Per-user sliding-window rate limiter for the step-up confirm endpoint.
    /// Keyed on the admin's user_id (as a String).
    /// Configured for 5 attempts per 5-minute window.
    pub confirm_rate_limit: EmailRateLimitState,
    /// In-memory cache for `app_settings` rows with a 60-second TTL.
    /// Used by the chat handler for kill-switch and budget checks without
    /// hitting the DB on every request.
    pub settings_cache: SettingsCache,
    /// Per-user binary semaphore (1 permit) that serialises the budget-check +
    /// LLM-call + usage-upsert sequence.  This prevents two concurrent requests
    /// from the same user from both passing the budget check before either
    /// records usage (TOCTOU race).  The `DashMap` grows at most to the number
    /// of concurrently active users and is never explicitly pruned.
    pub chat_semaphores: Arc<DashMap<Uuid, Arc<Semaphore>>>,
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
