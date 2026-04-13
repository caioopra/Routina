use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(health_check))
}

async fn health_check(State(state): State<AppState>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").execute(&state.pool).await.is_ok();

    Json(json!({
        "status": if db_ok { "healthy" } else { "degraded" },
        "database": db_ok,
    }))
}
