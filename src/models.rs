use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ReserveRequest {
    pub user_ref: String,
}

#[derive(Debug, Serialize)]
pub struct BookingResponse {
    pub booking_id: Uuid,
    pub event_id: Uuid,
    pub seat_id: Uuid,
    pub seat_label: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEventRequest {
    pub name: String,
    pub seat_count: i32,
}

#[derive(Debug, Serialize)]
pub struct CreateEventResponse {
    pub event_id: Uuid,
    pub name: String,
    pub seat_count: i64,
}

#[derive(Debug, Serialize)]
pub struct EventAvailabilityResponse {
    pub event_id: Uuid,
    pub total_seats: i64,
    pub available: i64,
    pub held: i64,
    pub booked: i64,
}
