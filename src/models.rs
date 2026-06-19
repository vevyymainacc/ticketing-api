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
