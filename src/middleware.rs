use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct Limiter {
    permits: Arc<Semaphore>,
    request_timeout: Duration,
}

impl Limiter {
    pub fn new(max_in_flight: usize, request_timeout: Duration) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(max_in_flight)),
            request_timeout,
        }
    }
}

pub async fn enforce_limits(
    State(limiter): State<Limiter>,
    request: Request,
    next: Next,
) -> Response {
    let _permit = match limiter.permits.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, "1")],
                Json(json!({ "error": "overloaded, retry shortly" })),
            )
                .into_response();
        }
    };

    match tokio::time::timeout(limiter.request_timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => (
            StatusCode::REQUEST_TIMEOUT,
            Json(json!({ "error": "request timed out" })),
        )
            .into_response(),
    }
}
