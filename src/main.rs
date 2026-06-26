//! Binary entry point and task orchestration for `wattdog`.
//!
//! The executable wires together TOML configuration, Thornwave
//! scanner, in-memory metrics state, HTTP server, Parquet writer, and graceful
//! shutdown handling. Long-running work is delegated to focused tasks so the
//! scanner callback path remains non-blocking.

#![expect(
    clippy::multiple_crate_versions,
    reason = "transitive dependencies currently pull duplicate versions; do not change dependency graph for lint policy"
)]
#![warn(missing_docs)]

mod cli;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::Parser;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use wattdog::{
    action::http::ActionClient,
    config::Config,
    http_server,
    metrics::Metrics,
    parquet_writer::{ParquetWriterConfig, run_writer},
    sample,
    scanner::Scanner,
    shutdown,
    state::engine::StateEngine,
};

use crate::cli::Cli;

const SAMPLE_QUEUE_CAPACITY: usize = 16_384;
const WRITER_QUEUE_CAPACITY: usize = 16_384;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config = Config::load(&cli.config).await?;
    info!(config = %cli.config.display(), "loaded wattdog config");

    if cli.check_config {
        info!(config = %cli.config.display(), "config is valid");
        return Ok(());
    }

    info!(dry_run = cli.dry_run, "starting wattdog");

    tokio::fs::create_dir_all(&config.data.dir)
        .await
        .with_context(|| {
            format!(
                "failed to create data directory {}",
                config.data.dir.display()
            )
        })?;

    let metrics = Arc::new(Metrics::new(env!("CARGO_PKG_VERSION")));
    let action_client = ActionClient::new(&config.http, cli.dry_run)?;
    let engine = Arc::new(Mutex::new(StateEngine::new(&config)?));
    let shutdown = CancellationToken::new();

    let (sample_tx, sample_rx) = mpsc::channel(SAMPLE_QUEUE_CAPACITY);
    let (writer_tx, writer_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);

    let writer_metrics = Arc::clone(&metrics);
    let writer_config = ParquetWriterConfig::new(config.data.dir.clone(), config.data.roll);
    let writer_shutdown = shutdown.clone();
    let writer_handle = tokio::spawn(async move {
        if let Err(error) =
            run_writer(writer_config, writer_rx, writer_metrics, writer_shutdown).await
        {
            error!(?error, "parquet writer task failed");
        }
    });

    let processor_metrics = Arc::clone(&metrics);
    let processor_engine = Arc::clone(&engine);
    let processor_shutdown = shutdown.clone();
    let processor_handle = tokio::spawn(async move {
        process_samples(
            sample_rx,
            writer_tx,
            processor_engine,
            processor_metrics,
            processor_shutdown,
        )
        .await;
    });

    let tick_metrics = Arc::clone(&metrics);
    let tick_engine = Arc::clone(&engine);
    let tick_shutdown = shutdown.clone();
    let tick_handle = tokio::spawn(async move {
        run_state_tick(tick_engine, action_client, tick_metrics, tick_shutdown).await;
    });

    let http_metrics = Arc::clone(&metrics);
    let http_shutdown = shutdown.clone();
    let http_handle = tokio::spawn(async move {
        if let Err(error) =
            http_server::serve(config.metrics.listen, http_metrics, http_shutdown).await
        {
            error!(?error, "http server failed");
        }
    });

    let scanner_metrics = Arc::clone(&metrics);
    let scanner_shutdown = shutdown.clone();
    info!("spawning Thornwave scanner startup task");
    let scanner_handle = tokio::task::spawn_blocking(move || {
        run_scanner_blocking(sample_tx, &scanner_metrics, &scanner_shutdown);
    });

    shutdown::wait_for_shutdown().await;
    info!("shutdown requested");

    shutdown.cancel();

    processor_handle
        .await
        .context("sample processor task panicked")?;
    writer_handle
        .await
        .context("parquet writer task panicked")?;
    tick_handle.await.context("state tick task panicked")?;
    http_handle.await.context("http task panicked")?;
    scanner_handle.await.context("scanner task panicked")?;

    info!("wattdog stopped");
    Ok(())
}

/// Owns Thornwave scanner startup and shutdown on a blocking thread.
///
/// The Thornwave SDK is synchronous and may block while initializing Bluetooth,
/// or `DBus` state. Running it on Tokio's blocking pool keeps `/healthz`
/// and `/metrics` available even if scanner initialization is slow or degraded.
fn run_scanner_blocking(
    sample_tx: mpsc::Sender<sample::Sample>,
    metrics: &Arc<Metrics>,
    shutdown: &CancellationToken,
) {
    info!("scanner startup task entered");
    let scanner = match Scanner::new(sample_tx, Arc::clone(metrics)) {
        Ok(scanner) => scanner,
        Err(error) => {
            metrics.set_scanner_running(false);
            metrics.set_ble_running(false);
            error!(
                ?error,
                "failed to create Thornwave scanner; metrics endpoint remains available"
            );
            return;
        }
    };

    info!(
        library_version_bcd = format_args!("0x{:04X}", Scanner::library_version_bcd()),
        "loaded Thornwave SDK"
    );

    match scanner.start_ble() {
        Ok(()) => {
            metrics.set_ble_running(true);
            info!("started Thornwave BLE scan");
        }
        Err(error) => {
            metrics.set_ble_running(false);
            warn!(?error, "failed to start Thornwave BLE scan");
        }
    }

    if metrics.any_scanner_transport_running() {
        metrics.set_scanner_running(true);
        info!(
            "scanner is running; metrics will remain empty until Thornwave advertisements are received"
        );
    } else {
        metrics.set_scanner_running(false);
        warn!("BLE Thornwave scanning could not be started; metrics endpoint remains available");
        return;
    }

    while !shutdown.is_cancelled() {
        std::thread::sleep(Duration::from_millis(250));
    }

    scanner.stop();
    metrics.set_scanner_running(false);
    metrics.set_ble_running(false);
}

/// Processes scanner observations into metrics and the Parquet writer queue.
///
/// The scanner callback sends fully-owned [`sample::Sample`] values into
/// `sample_rx`. This task performs all metrics mutation and attempts a
/// non-blocking send to the writer channel. If the writer queue is full, the
/// observation is intentionally dropped for disk persistence and the dropped
/// writer metric is incremented.
async fn process_samples(
    mut sample_rx: mpsc::Receiver<sample::Sample>,
    writer_tx: mpsc::Sender<sample::Sample>,
    engine: Arc<Mutex<StateEngine>>,
    metrics: Arc<Metrics>,
    shutdown: CancellationToken,
) {
    let mut observations_processed = 0_u64;
    let mut observations_logged = 0_u64;
    let mut status_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(30),
        Duration::from_secs(30),
    );

    loop {
        tokio::select! {
            biased;
            Some(sample) = sample_rx.recv() => {
                if observations_processed == 0 {
                    info!(
                        serial = sample.serial,
                        name = sample.name.as_deref().unwrap_or(""),
                        rssi_dbm = sample.rssi_dbm,
                        voltage1_volts = sample.voltage1_volts,
                        "processed first Thornwave advertisement"
                    );
                }
                observations_processed = observations_processed.saturating_add(1);
                metrics.observe_sample(&sample);
                engine.lock().await.observe_sample(&sample, &metrics, Instant::now());
                if writer_tx.try_send(sample).is_err() {
                    metrics.observe_writer_dropped();
                }
            }
            _ = status_interval.tick() => {
                if observations_processed == 0 {
                    info!("scanner is running but no Thornwave advertisements have been processed yet");
                } else if observations_processed != observations_logged {
                    info!(observations_processed, "processed Thornwave advertisements");
                    observations_logged = observations_processed;
                } else {
                    info!(observations_processed, "no new Thornwave advertisements since last status log");
                }
            }
            () = shutdown.cancelled() => {
                break;
            }
            else => break,
        }
    }

    while let Ok(sample) = sample_rx.try_recv() {
        metrics.observe_sample(&sample);
        engine
            .lock()
            .await
            .observe_sample(&sample, &metrics, Instant::now());
        if writer_tx.try_send(sample).is_err() {
            metrics.observe_writer_dropped();
        }
    }
}

async fn run_state_tick(
    engine: Arc<Mutex<StateEngine>>,
    action_client: ActionClient,
    metrics: Arc<Metrics>,
    shutdown: CancellationToken,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let now = Instant::now();
                let actions = engine.lock().await.collect_due_actions(&metrics, now);
                for action in actions {
                    let outcome = action_client
                        .send(&action.state_name, action.target, &action.url)
                        .await;
                    engine
                        .lock()
                        .await
                        .record_action_result(&action, outcome, &metrics, now);
                }
            }
            () = shutdown.cancelled() => break,
        }
    }
}
