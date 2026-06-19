use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{BookingResponse, ReserveRequest};
use crate::repo;
use crate::state::AppState;

fn to_response(record: repo::BookingRecord) -> BookingResponse {
    BookingResponse {
        booking_id: record.booking_id,
        event_id: record.event_id,
        seat_id: record.seat_id,
        seat_label: record.seat_label,
        status: record.status,
        expires_at: record.expires_at,
    }
}

pub async fn reserve_seat(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ReserveRequest>,
) -> Result<Json<BookingResponse>, AppError> {
    // i used the idempotency key header so clients can safely retry failed requests
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or(AppError::Invalid("Idempotency-Key header is required"))?;

    let record = repo::reserve_any_seat(
        &state.pool,
        event_id,
        &body.user_ref,
        idempotency_key,
        state.hold_ttl_secs,
    )
    .await?;

    Ok(Json(to_response(record)))
}

pub async fn confirm_reservation(
    State(state): State<AppState>,
    Path(idempotency_key): Path<String>,
) -> Result<Json<BookingResponse>, AppError> {
    let record = repo::confirm_reservation(&state.pool, &idempotency_key).await?;
    Ok(Json(to_response(record)))
}

pub async fn get_reservation(
    State(state): State<AppState>,
    Path(idempotency_key): Path<String>,
) -> Result<Json<BookingResponse>, AppError> {
    let record = repo::find_booking(&state.pool, &idempotency_key)
        .await?
        .ok_or(AppError::NotFound("booking not found"))?;
    Ok(Json(to_response(record)))
}
