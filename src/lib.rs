pub mod config;
pub mod error;
pub mod middleware;
pub mod models;
pub mod repo;
pub mod routes;
pub mod state;
pub mod telemetry;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(routes::health::health))
        .route("/ready", get(routes::health::readiness))
        .route("/events", post(routes::events::create_event))
        .route("/events/:event_id", get(routes::events::get_event))
        .route(
            "/events/:event_id/reservations",
            post(routes::bookings::reserve_seat),
        )
        .route(
            "/reservations/:idempotency_key",
            get(routes::bookings::get_reservation),
        )
        .route(
            "/reservations/:idempotency_key/confirm",
            post(routes::bookings::confirm_reservation),
        )
        .with_state(state)
}
