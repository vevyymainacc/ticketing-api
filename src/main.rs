use std::time::Duration;

use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::get;
use axum_prometheus::PrometheusMetricLayer;
use sqlx::postgres::PgPoolOptions;
use sqlx::Executor;
use tracing_subscriber::EnvFilter;

use ticketing_api::build_router;
use ticketing_api::config::Config;
use ticketing_api::middleware::{enforce_limits, Limiter};
use ticketing_api::state::AppState;
use ticketing_api::telemetry::request_context;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();

    let statement_timeout_ms = config.statement_timeout_ms;
    let pool = PgPoolOptions::new()
        .max_connections(config.pool_max_connections)
        .acquire_timeout(Duration::from_secs(config.pool_acquire_timeout_secs))
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                let stmt = format!("SET statement_timeout = {statement_timeout_ms}");
                conn.execute(stmt.as_str()).await?;
                Ok(())
            })
        })
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let pool_for_shutdown = pool.clone();
    let sweep_pool = pool.clone();
    let sweep_interval = config.sweep_interval_secs.max(1);
    let state = AppState {
        pool,
        hold_ttl_secs: config.hold_ttl_secs,
    };

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(sweep_interval));
        loop {
            ticker.tick().await;
            match ticketing_api::repo::sweep_expired_holds(&sweep_pool).await {
                Ok(n) if n > 0 => tracing::info!(reclaimed = n, "swept expired holds"),
                Ok(_) => {}
                Err(err) => tracing::error!(error = %err, "hold sweep failed"),
            }
        }
    });

    let limiter = Limiter::new(
        config.max_in_flight,
        Duration::from_secs(config.request_timeout_secs),
    );

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let app = build_router(state)
        .route(
            "/metrics",
            get(move || std::future::ready(metric_handle.render())),
        )
        .layer(from_fn_with_state(limiter, enforce_limits))
        .layer(prometheus_layer)
        .layer(from_fn(request_context));

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("draining database connections");
    pool_for_shutdown.close().await;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, draining in-flight requests");
}
