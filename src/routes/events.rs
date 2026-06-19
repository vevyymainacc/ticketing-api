use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{CreateEventRequest, CreateEventResponse, EventAvailabilityResponse};
use crate::repo;
use crate::state::AppState;

pub async fn create_event(
    State(state): State<AppState>,
    Json(body): Json<CreateEventRequest>,
) -> Result<(StatusCode, Json<CreateEventResponse>), AppError> {
    let created = repo::create_event(&state.pool, &body.name, body.seat_count).await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateEventResponse {
            event_id: created.event_id,
            name: created.name,
            seat_count: created.seat_count,
        }),
    ))
}

pub async fn get_event(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
) -> Result<Json<EventAvailabilityResponse>, AppError> {
    let availability = repo::event_availability(&state.pool, event_id)
        .await?
        .ok_or(AppError::NotFound("event not found"))?;

    Ok(Json(EventAvailabilityResponse {
        event_id: availability.event_id,
        total_seats: availability.total,
        available: availability.available,
        held: availability.held,
        booked: availability.booked,
    }))
}
