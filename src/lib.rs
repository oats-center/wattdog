//! Library surface for Wattdog and shared Thornwave SDK access.

#![expect(
    clippy::multiple_crate_versions,
    reason = "transitive dependencies currently pull duplicate versions; do not change dependency graph for lint policy"
)]

#[cfg(feature = "app")]
pub mod action;
#[cfg(feature = "app")]
pub mod config;
#[cfg(feature = "ffi")]
pub mod ffi;
#[cfg(feature = "app")]
pub mod http_server;
#[cfg(feature = "app")]
pub mod metrics;
#[cfg(feature = "app")]
pub mod parquet_writer;
#[cfg(feature = "app")]
pub mod sample;
#[cfg(feature = "app")]
pub mod scanner;
#[cfg(feature = "app")]
pub mod shutdown;
#[cfg(feature = "app")]
pub mod state;
