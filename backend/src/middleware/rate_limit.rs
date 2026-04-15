//! Per-user sliding-window rate limiter for the chat endpoint.
//!
//! `RateLimitState` holds a `DashMap<Uuid, VecDeque<Instant>>` and a
//! `max_per_minute` cap.  It is embedded in `AppState` so that the middleware
//! function only needs a single `State<AppState>` extractor.
//!
//! The middleware:
//!  - Extracts the JWT bearer token from the `Authorization` header.
//!  - Decodes it (no DB round-trip) to obtain the `user_id`.
//!  - Trims entries older than 60 s, then rejects if `queue.len() >= max`.
//!  - If no valid JWT is present, passes the request through (the auth layer
//!    downstream rejects it with 401).
//!
//! On rejection it returns HTTP 429 with:
//!   `{"error":"rate_limited","retry_after_seconds":N}`
//! and a `Retry-After: N` header.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use serde_json::json;
use tokio::time::Instant;
use uuid::Uuid;

use crate::auth::{TokenKind, decode_token};
use crate::routes::AppState;

const WINDOW_SECS: u64 = 60;
/// Sliding-window duration for the per-email login rate limiter (15 minutes).
pub const LOGIN_RATE_WINDOW_SECS: u64 = 900;
/// Maximum failed/unvalidated login attempts per email per window.
pub const LOGIN_RATE_MAX: usize = 10;
/// Sweep empty buckets from the map roughly every N successful `check_and_record` calls.
const SWEEP_INTERVAL: u64 = 100;

/// Shared, cheaply-cloneable rate-limit store.
#[derive(Clone)]
pub struct RateLimitState {
    pub buckets: Arc<DashMap<Uuid, VecDeque<Instant>>>,
    pub max_per_minute: usize,
    /// Monotonically increasing counter used to trigger periodic sweeps.
    call_count: Arc<AtomicU64>,
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self::new(20)
    }
}

impl RateLimitState {
    pub fn new(max_per_minute: usize) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            max_per_minute,
            call_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Remove all map entries whose timestamps have all expired.
    ///
    /// Called opportunistically every `SWEEP_INTERVAL` allowed requests so that
    /// users who go inactive do not leave dead entries in the map forever.
    /// Each bucket's stale timestamps are trimmed first; if the bucket is then
    /// empty it is removed from the map.
    pub fn sweep_empty(&self) {
        let window = tokio::time::Duration::from_secs(WINDOW_SECS);
        let now = Instant::now();
        let cutoff = now.checked_sub(window).unwrap_or(now);
        self.buckets.retain(|_, deque| {
            while let Some(&front) = deque.front() {
                if front <= cutoff {
                    deque.pop_front();
                } else {
                    break;
                }
            }
            !deque.is_empty()
        });
    }

    /// Check and record a new request for `user_id`.
    ///
    /// Returns `Ok(())` if the request is within the limit, or
    /// `Err(retry_after_secs)` if the cap has been reached.
    pub fn check_and_record(&self, user_id: Uuid) -> Result<(), u64> {
        let window = tokio::time::Duration::from_secs(WINDOW_SECS);
        let now = Instant::now();

        let mut entry = self.buckets.entry(user_id).or_default();
        let deque = entry.value_mut();

        // Evict timestamps older than the sliding window.
        let cutoff = now.checked_sub(window).unwrap_or(now);
        while let Some(&front) = deque.front() {
            if front <= cutoff {
                deque.pop_front();
            } else {
                break;
            }
        }

        if deque.len() >= self.max_per_minute {
            let oldest = *deque.front().unwrap(); // safe: len >= 1
            let elapsed = now.duration_since(oldest);
            let retry_after = WINDOW_SECS.saturating_sub(elapsed.as_secs());
            return Err(retry_after.max(1));
        }

        deque.push_back(now);
        // Drop the entry guard before potentially calling sweep_empty, which
        // needs to acquire shard locks itself.
        drop(entry);

        // Opportunistic sweep: every SWEEP_INTERVAL allowed requests, trim
        // stale buckets to prevent unbounded map growth from inactive users.
        // We use the post-increment value so the first sweep fires after
        // exactly SWEEP_INTERVAL successful requests, not on the very first one.
        let count = self.call_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count.is_multiple_of(SWEEP_INTERVAL) {
            self.sweep_empty();
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Per-email login rate limiter
// ---------------------------------------------------------------------------

/// Sliding-window rate limiter keyed on a `String` (normalized email address).
///
/// Used by the login handler to limit brute-force/credential-stuffing attempts
/// before the password hash comparison is even attempted.
///
/// Design choice: every login attempt (successful or not) increments the
/// bucket; a successful login resets (clears) the bucket for that email.
/// This means an attacker who finds the correct password on attempt N ≤ limit
/// still succeeds, but subsequent attempts restart from zero — preventing
/// indefinite harassment while letting the real user log in.  The alternative
/// (only counting failures) would allow an attacker to interleave one correct
/// attempt per window to keep resetting, which is worse.
#[derive(Clone)]
pub struct EmailRateLimitState {
    pub buckets: Arc<DashMap<String, VecDeque<Instant>>>,
    pub max_attempts: usize,
    pub window_secs: u64,
}

impl EmailRateLimitState {
    pub fn new(max_attempts: usize, window_secs: u64) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            max_attempts,
            window_secs,
        }
    }

    /// Check and record a login attempt for `email`.
    ///
    /// Returns `Ok(())` if the attempt is within the limit, or
    /// `Err(retry_after_secs)` if the cap has been reached.
    ///
    /// The check happens BEFORE the DB lookup or password comparison, so the
    /// response does not reveal whether the email exists in the database.
    pub fn check_and_record(&self, email: &str) -> Result<(), u64> {
        let window = tokio::time::Duration::from_secs(self.window_secs);
        let now = Instant::now();

        let mut entry = self.buckets.entry(email.to_owned()).or_default();
        let deque = entry.value_mut();

        // Evict timestamps older than the sliding window.
        let cutoff = now.checked_sub(window).unwrap_or(now);
        while let Some(&front) = deque.front() {
            if front <= cutoff {
                deque.pop_front();
            } else {
                break;
            }
        }

        if deque.len() >= self.max_attempts {
            let oldest = *deque.front().unwrap(); // safe: len >= 1
            let elapsed = now.duration_since(oldest);
            let retry_after = self.window_secs.saturating_sub(elapsed.as_secs());
            return Err(retry_after.max(1));
        }

        deque.push_back(now);
        Ok(())
    }

    /// Clear the rate-limit bucket for `email` on a successful login.
    ///
    /// Removing the entry entirely means the next attempt starts with a clean
    /// slate rather than carrying forward stale timestamps.
    pub fn clear(&self, email: &str) {
        self.buckets.remove(email);
    }
}

/// Axum `from_fn_with_state` middleware that enforces per-user rate limiting.
///
/// `AppState` must carry a `rate_limit` field of type `RateLimitState`.
/// The middleware reads the JWT from the `Authorization` header (no DB hit),
/// calls `check_and_record`, and returns 429 on violation.
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    // Decode user_id from the JWT — no DB hit required.
    let user_id = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .and_then(|token| decode_token(token, &state.config.jwt_secret).ok())
        .filter(|claims| claims.typ == TokenKind::Access)
        .map(|claims| claims.sub);

    let Some(uid) = user_id else {
        // No valid JWT — pass through; auth middleware handles the 401.
        return next.run(request).await;
    };

    match state.rate_limit.check_and_record(uid) {
        Ok(()) => next.run(request).await,
        Err(retry_after) => {
            let body = json!({
                "error": "rate_limited",
                "retry_after_seconds": retry_after,
            });
            (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, retry_after.to_string())],
                Json(body),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(max: usize) -> RateLimitState {
        RateLimitState::new(max)
    }

    #[tokio::test]
    async fn allows_requests_under_limit() {
        let state = make_state(3);
        let uid = Uuid::now_v7();
        assert!(state.check_and_record(uid).is_ok());
        assert!(state.check_and_record(uid).is_ok());
        assert!(state.check_and_record(uid).is_ok());
    }

    #[tokio::test]
    async fn rejects_at_limit() {
        let state = make_state(2);
        let uid = Uuid::now_v7();
        assert!(state.check_and_record(uid).is_ok());
        assert!(state.check_and_record(uid).is_ok());
        let result = state.check_and_record(uid);
        assert!(result.is_err(), "should be rejected at limit");
        let retry = result.unwrap_err();
        assert!(
            (1..=60).contains(&retry),
            "retry_after out of range: {retry}"
        );
    }

    #[tokio::test]
    async fn different_users_are_independent() {
        let state = make_state(1);
        let uid_a = Uuid::now_v7();
        let uid_b = Uuid::now_v7();
        // uid_a fills up
        assert!(state.check_and_record(uid_a).is_ok());
        assert!(state.check_and_record(uid_a).is_err());
        // uid_b is unaffected
        assert!(state.check_and_record(uid_b).is_ok());
    }

    #[tokio::test]
    async fn expired_entries_are_evicted() {
        tokio::time::pause();

        let state = make_state(1);
        let uid = Uuid::now_v7();

        assert!(state.check_and_record(uid).is_ok());
        // Advance time past the window.
        tokio::time::advance(tokio::time::Duration::from_secs(61)).await;
        // After window expiry the slot should be available again.
        assert!(
            state.check_and_record(uid).is_ok(),
            "entry should have expired"
        );
    }

    #[tokio::test]
    async fn retry_after_is_nonzero() {
        let state = make_state(1);
        let uid = Uuid::now_v7();
        let _ = state.check_and_record(uid); // fill
        let err = state.check_and_record(uid).unwrap_err();
        assert!(err >= 1);
    }

    #[tokio::test]
    async fn sweep_empty_removes_inactive_entries() {
        tokio::time::pause();

        // Use a low max so we can fill the bucket quickly.
        let state = make_state(5);

        // Register three distinct users.
        let uid_a = Uuid::now_v7();
        let uid_b = Uuid::now_v7();
        let uid_c = Uuid::now_v7();

        assert!(state.check_and_record(uid_a).is_ok());
        assert!(state.check_and_record(uid_b).is_ok());
        assert!(state.check_and_record(uid_c).is_ok());

        // All three buckets are present.
        assert_eq!(state.buckets.len(), 3);

        // Advance time past the sliding window so all timestamps become stale.
        tokio::time::advance(tokio::time::Duration::from_secs(WINDOW_SECS + 1)).await;

        // A new request for uid_a evicts its own stale timestamps during
        // check_and_record (the bucket becomes empty before the new timestamp is
        // pushed); the explicit sweep then trims uid_b and uid_c which were never
        // touched again.
        assert!(state.check_and_record(uid_a).is_ok());

        // After the explicit sweep, uid_b and uid_c should be gone.
        state.sweep_empty();
        assert_eq!(
            state.buckets.len(),
            1,
            "only uid_a's active bucket should remain after sweep"
        );
        assert!(state.buckets.contains_key(&uid_a));
    }

    #[tokio::test]
    async fn opportunistic_sweep_triggers_at_interval() {
        tokio::time::pause();

        // Use a cap high enough that no request is rejected.
        let state = make_state(200);

        // Seed N_STALE distinct user entries (counts toward the shared call counter).
        const N_STALE: u64 = 10;
        let uids: Vec<Uuid> = (0..N_STALE).map(|_| Uuid::now_v7()).collect();
        for &uid in &uids {
            assert!(state.check_and_record(uid).is_ok());
        }
        assert_eq!(state.buckets.len() as u64, N_STALE);

        // Advance past the window so all seeded timestamps become stale.
        tokio::time::advance(tokio::time::Duration::from_secs(WINDOW_SECS + 1)).await;

        // We need exactly SWEEP_INTERVAL total calls to trigger the sweep.
        // N_STALE calls already happened, so drive (SWEEP_INTERVAL - N_STALE)
        // additional requests for a fresh active user.
        // Stop one short of the boundary — no sweep yet.
        let active_uid = Uuid::now_v7();
        let pre_sweep_calls = SWEEP_INTERVAL - N_STALE - 1;
        for _ in 0..pre_sweep_calls {
            assert!(state.check_and_record(active_uid).is_ok());
        }
        // N_STALE stale buckets + active_uid = N_STALE + 1 total entries.
        assert_eq!(
            state.buckets.len() as u64,
            N_STALE + 1,
            "no sweep should have fired yet"
        );

        // The next call reaches the SWEEP_INTERVAL boundary and fires the sweep.
        assert!(state.check_and_record(active_uid).is_ok());

        // After the sweep, the N_STALE stale buckets must be gone;
        // only the active user's bucket remains.
        assert_eq!(
            state.buckets.len(),
            1,
            "stale buckets should be evicted after opportunistic sweep"
        );
        assert!(state.buckets.contains_key(&active_uid));
    }
}
