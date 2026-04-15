//! Settings endpoints — provider selection.
//!
//! `GET  /api/settings/providers`       — list available + selected provider.
//! `POST /api/settings/llm-provider`    — set the preferred LLM provider for
//!                                         the authenticated user.  Writes to
//!                                         `users.preferences` (JSONB merge;
//!                                         other keys are preserved).

use axum::{Json, Router, extract::State, routing::get, routing::post};
use serde::{Deserialize, Serialize};

use crate::middleware::CurrentUser;
use crate::middleware::error::AppError;

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/providers", get(get_providers))
        .route("/llm-provider", post(set_llm_provider))
}

// ---------------------------------------------------------------------------
// Response / request types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ProvidersResponse {
    pub available: Vec<String>,
    pub selected: String,
}

#[derive(Debug, Deserialize)]
pub struct SetProviderRequest {
    pub provider: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /api/settings/providers`
///
/// Returns the list of available providers (those configured at startup) and
/// the user's current selection from `users.preferences.llm_provider`.
/// If the stored selection names a provider that is no longer available, the
/// response returns the first available provider as `selected`.
async fn get_providers(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<ProvidersResponse>, AppError> {
    let mut available: Vec<String> = state.providers.keys().cloned().collect();
    available.sort(); // deterministic ordering

    // Read user's stored preference.
    let row = sqlx::query!("SELECT preferences FROM users WHERE id = $1", user.id,)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let stored = row
        .preferences
        .get("llm_provider")
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    // The selection is the stored value if it's in the available list;
    // otherwise fall back to the first available (or empty string if none).
    let selected = match stored {
        Some(name) if available.contains(&name) => name,
        _ => available.first().cloned().unwrap_or_default(),
    };

    Ok(Json(ProvidersResponse {
        available,
        selected,
    }))
}

/// `POST /api/settings/llm-provider`
///
/// Body: `{ "provider": "gemini" | "claude" | ... }`
///
/// Returns 400 if the requested provider is not in the `available` list.
/// Writes `llm_provider` into `users.preferences` via `jsonb_set` to avoid
/// clobbering other preference keys.
async fn set_llm_provider(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<SetProviderRequest>,
) -> Result<Json<ProvidersResponse>, AppError> {
    // Validate against the CURRENT available list (not a hard-coded one).
    let mut available: Vec<String> = state.providers.keys().cloned().collect();
    available.sort();

    if !available.contains(&body.provider) {
        return Err(AppError::BadRequest(format!(
            "provider '{}' is not available; available: [{}]",
            body.provider,
            available.join(", ")
        )));
    }

    // Merge into preferences JSONB without clobbering other keys.
    // COALESCE guards against preferences being NULL (jsonb_set returns NULL
    // when the first argument is NULL, even with create_missing = true).
    let provider_value = serde_json::json!(body.provider);
    sqlx::query(
        "UPDATE users \
         SET preferences = jsonb_set(COALESCE(preferences, '{}'::jsonb), '{llm_provider}', $1::jsonb, true), \
             updated_at  = now() \
         WHERE id = $2",
    )
    .bind(provider_value)
    .bind(user.id)
    .execute(&state.pool)
    .await?;

    Ok(Json(ProvidersResponse {
        available,
        selected: body.provider,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_response_serializes() {
        let r = ProvidersResponse {
            available: vec!["claude".to_string(), "gemini".to_string()],
            selected: "gemini".to_string(),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["selected"], "gemini");
        assert!(v["available"].as_array().unwrap().len() == 2);
    }

    #[test]
    fn set_provider_request_deserializes() {
        let json = serde_json::json!({ "provider": "claude" });
        let req: SetProviderRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.provider, "claude");
    }
}
