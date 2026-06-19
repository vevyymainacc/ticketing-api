use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;
use uuid::Uuid;

pub async fn request_context(req: Request, next: Next) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let span =
        tracing::info_span!("request", request_id = %request_id, method = %method, path = %path);

    let mut response = async move {
        let response = next.run(req).await;
        tracing::info!(status = response.status().as_u16(), "handled");
        response
    }
    .instrument(span)
    .await;

    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}
