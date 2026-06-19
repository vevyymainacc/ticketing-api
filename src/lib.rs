pub mod config;
pub mod error;
pub mod models;
pub mod repo;
pub mod routes;
pub mod state;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(routes::health::health))
        .route("/ready", get(routes::health::readiness))
        .route("/events", post(routes::events::create_event))
        .route(
            "/events/:event_id/reservations",
            post(routes::bookings::reserve_seat),
        )
        .with_state(state)
}
