use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("no seats available")]
    SoldOut,
    #[error("event not found")]
    EventNotFound,
    #[error("{0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::SoldOut => (StatusCode::CONFLICT, self.to_string()),
            AppError::EventNotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Invalid(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Db(err) => {
                tracing::error!(error = %err, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
