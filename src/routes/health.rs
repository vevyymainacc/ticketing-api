use axum::extract::State;
use axum::http::StatusCode;

use crate::state::AppState;

pub async fn health() -> &'static str {
    "ok"
}

pub async fn readiness(State(state): State<AppState>) -> StatusCode {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}
