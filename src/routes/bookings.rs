use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{BookingResponse, ReserveRequest};
use crate::repo;
use crate::state::AppState;

pub async fn reserve_seat(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ReserveRequest>,
) -> Result<Json<BookingResponse>, AppError> {
    // i used the idempotency key header so clients can safely retry failed requests
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok());

    let reserved =
        repo::reserve_any_seat(&state.pool, event_id, &body.user_ref, idempotency_key).await?;

    Ok(Json(BookingResponse {
        booking_id: reserved.booking_id,
        event_id,
        seat_id: reserved.seat_id,
        seat_label: reserved.seat_label,
    }))
}
