use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use axum::Router;
use ticketing_api::middleware::{enforce_limits, Limiter};
use tower::ServiceExt;

async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_millis(300)).await;
    "ok"
}

#[tokio::test]
async fn sheds_excess_requests_when_over_capacity() {
    let capacity = 2;
    let attempts = 10;

    let limiter = Limiter::new(capacity, Duration::from_secs(5));
    let app = Router::new()
        .route("/", get(slow))
        .layer(from_fn_with_state(limiter, enforce_limits));

    let mut handles = Vec::new();
    for _ in 0..attempts {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let request = Request::builder().uri("/").body(Body::empty()).unwrap();
            app.oneshot(request).await.unwrap().status()
        }));
    }

    let mut served = 0;
    let mut shed = 0;
    for handle in handles {
        match handle.await.unwrap() {
            StatusCode::OK => served += 1,
            StatusCode::SERVICE_UNAVAILABLE => shed += 1,
            other => panic!("unexpected status: {other}"),
        }
    }

    assert_eq!(
        served, capacity,
        "only `capacity` requests should be served"
    );
    assert_eq!(shed, attempts - capacity, "the rest must be shed with 503");
}
