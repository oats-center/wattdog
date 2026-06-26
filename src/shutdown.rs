//! Process signal handling for graceful daemon shutdown.
//!
//! The daemon uses a single shutdown future to coordinate SIGINT/SIGTERM with
//! scanner stop, queue drain, and Parquet writer finalization.

use tracing::info;

/// Waits until the process receives Ctrl-C or, on Unix, SIGTERM.
///
/// The function logs which signal triggered shutdown and then returns to the
/// caller so the rest of the process can cancel tasks and flush in-flight data.
pub async fn wait_for_shutdown() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(?error, "failed to install Ctrl-C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::warn!(?error, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("received Ctrl-C"),
        () = terminate => info!("received SIGTERM"),
    }
}
