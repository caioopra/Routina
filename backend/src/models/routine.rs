use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Routine {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub period: Option<String>,
    pub is_active: bool,
    pub meta: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request body for `POST /api/routines`.
#[derive(Debug, Deserialize)]
pub struct CreateRoutineRequest {
    pub name: String,
    pub period: Option<String>,
    pub meta: Option<serde_json::Value>,
}

/// Request body for `PUT /api/routines/:id`.
/// Note: `is_active` is intentionally omitted — changes to activation go through
/// the dedicated `/activate` endpoint.
#[derive(Debug, Deserialize)]
pub struct UpdateRoutineRequest {
    pub name: Option<String>,
    pub period: Option<String>,
    pub meta: Option<serde_json::Value>,
}

/// Summary response shape returned for list/create/update/activate.
#[derive(Debug, Serialize)]
pub struct RoutineSummary {
    pub id: Uuid,
    pub name: String,
    pub period: Option<String>,
    pub is_active: bool,
    pub meta: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Routine> for RoutineSummary {
    fn from(r: Routine) -> Self {
        Self {
            id: r.id,
            name: r.name,
            period: r.period,
            is_active: r.is_active,
            meta: r.meta,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// Full response for `GET /api/routines/:id`.
/// `blocks`, `rules`, and `summary` are placeholders (empty arrays) in this
/// slice; later features will populate them.
#[derive(Debug, Serialize)]
pub struct RoutineResponse {
    pub id: Uuid,
    pub name: String,
    pub period: Option<String>,
    pub is_active: bool,
    pub meta: serde_json::Value,
    pub blocks: Vec<serde_json::Value>,
    pub rules: Vec<serde_json::Value>,
    pub summary: Vec<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Routine> for RoutineResponse {
    fn from(r: Routine) -> Self {
        Self {
            id: r.id,
            name: r.name,
            period: r.period,
            is_active: r.is_active,
            meta: r.meta,
            blocks: Vec::new(),
            rules: Vec::new(),
            summary: Vec::new(),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}
