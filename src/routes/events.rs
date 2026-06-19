use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::error::AppError;
use crate::models::{CreateEventRequest, CreateEventResponse};
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
