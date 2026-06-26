//! HTTP server for health checks and Prometheus/OpenMetrics scraping.
//!
//! The server exposes only read-only endpoints. It is intentionally not a
//! control API and contains no routes that can mutate Thornwave devices or
//! daemon configuration.

use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::metrics::Metrics;

/// Runs the HTTP server until the provided cancellation token is cancelled.
///
/// The server binds `addr`, serves `/healthz` and `/metrics`, and performs axum
/// graceful shutdown when `shutdown` is cancelled by the main task.
///
/// # Errors
///
/// Returns an error if binding the TCP listener or serving HTTP fails.
pub async fn serve(
    addr: SocketAddr,
    metrics: Arc<Metrics>,
    shutdown: CancellationToken,
) -> Result<()> {
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics_handler))
        .with_state(metrics);

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "metrics HTTP server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await?;

    Ok(())
}

async fn healthz() -> &'static str {
    "ok\n"
}

async fn metrics_handler(State(metrics): State<Arc<Metrics>>) -> impl IntoResponse {
    match metrics.encode() {
        Ok(body) => (
            StatusCode::OK,
            [(
                "content-type",
                "application/openmetrics-text; version=1.0.0; charset=utf-8",
            )],
            body,
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("content-type", "text/plain; charset=utf-8")],
            format!("failed to encode metrics: {error}\n"),
        )
            .into_response(),
    }
}
