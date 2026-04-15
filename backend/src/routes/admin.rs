//! Admin routes — `/api/admin/*`.
//!
//! Every handler in this module requires the `AdminUser` extractor, which
//! enforces `role == "admin"` via a DB lookup on every request.  No
//! router-level middleware is needed because the extractor itself returns
//! HTTP 403 for non-admins and HTTP 401 (via `CurrentUser`) for missing/invalid
//! tokens.
//!
//! Slice A ships a single proof-of-gating endpoint:
//!   `GET /api/admin/dashboard` → `{"ok": true, "admin_email": "..."}`
//!
//! Slices B–E will extend this module with audit, settings, metrics, and user
//! management endpoints.

use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

use crate::middleware::AdminUser;

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/dashboard", get(dashboard))
}

#[derive(Debug, Serialize)]
struct DashboardResponse {
    ok: bool,
    admin_email: String,
}

/// Stub dashboard endpoint — proof-of-gating for Slice A.
///
/// Returns `{"ok": true, "admin_email": "..."}` so integration tests can verify
/// that only users with `role = 'admin'` can reach this endpoint.  The response
/// body will be replaced with real metrics in Slice C.
async fn dashboard(State(_state): State<AppState>, admin: AdminUser) -> Json<DashboardResponse> {
    Json(DashboardResponse {
        ok: true,
        admin_email: admin.email,
    })
}
