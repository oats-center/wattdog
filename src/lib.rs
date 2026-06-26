//! Library surface for state-machine unit tests and future daemon wiring.

#![expect(
    clippy::multiple_crate_versions,
    reason = "transitive dependencies currently pull duplicate versions; do not change dependency graph for lint policy"
)]

pub mod action;
pub mod config;
pub mod ffi;
pub mod http_server;
pub mod metrics;
pub mod parquet_writer;
pub mod sample;
pub mod scanner;
pub mod shutdown;
pub mod state;
