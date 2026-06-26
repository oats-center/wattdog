//! Prometheus/OpenMetrics state for daemon health and latest values.
//!
//! Metrics are the live operational view of the daemon. They are intentionally
//! separate from the durable Parquet log and hold only latest per-device values,
//! process health, scanner counters, and writer counters.

use std::{
    collections::HashSet,
    sync::{RwLock, atomic::AtomicU64},
};

use anyhow::Result;
use prometheus_client::{
    encoding::{EncodeLabelSet, text::encode},
    metrics::{counter::Counter, family::Family, gauge::Gauge},
    registry::Registry,
};

use crate::{config::BinaryState, sample::Sample};

type F64Gauge = Gauge<f64, AtomicU64>;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct DeviceLabels {
    serial: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct DeviceInfoLabels {
    serial: String,
    name: String,
    model: String,
    address: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct StateLabels {
    name: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct StateFieldLabels {
    name: String,
    field: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct StateTargetLabels {
    name: String,
    target: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct HttpAttemptLabels {
    name: String,
    target: String,
    result: String,
}

/// Shared metrics registry and metric handles used by all runtime tasks.
///
/// The type is cheap to share behind an [`std::sync::Arc`]. Individual metric
/// values are internally synchronized by `prometheus_client`; only the serial
/// set uses an explicit [`RwLock`] to maintain the device count gauge.
#[derive(Debug)]
pub struct Metrics {
    registry: Registry,
    scanner_running: Gauge,
    scanner_ble_running: Gauge,
    observations_received: Counter,
    observations_dropped: Counter,
    writer_dropped_observations: Counter,
    devices_seen: Gauge,
    device_info: Family<DeviceInfoLabels, Gauge>,
    device_observations_received: Family<DeviceLabels, Counter>,
    last_seen_timestamp_seconds: Family<DeviceLabels, F64Gauge>,
    voltage1_volts: Family<DeviceLabels, F64Gauge>,
    voltage2_volts: Family<DeviceLabels, F64Gauge>,
    current_amperes: Family<DeviceLabels, F64Gauge>,
    power_watts: Family<DeviceLabels, F64Gauge>,
    temperature_celsius: Family<DeviceLabels, F64Gauge>,
    soc_percent: Family<DeviceLabels, F64Gauge>,
    runtime_minutes: Family<DeviceLabels, F64Gauge>,
    rssi_dbm: Family<DeviceLabels, F64Gauge>,
    power_status_code: Family<DeviceLabels, F64Gauge>,
    parquet_rows_written: Counter,
    parquet_batches_written: Counter,
    parquet_write_errors: Counter,
    parquet_active_file_open: Gauge,
    parquet_active_file_rows: Gauge,
    parquet_last_successful_write_timestamp_seconds: F64Gauge,
    parquet_rolls: Counter,
    state_input_value: Family<StateFieldLabels, F64Gauge>,
    state_input_age_seconds: Family<StateLabels, F64Gauge>,
    state_stale: Family<StateLabels, Gauge>,
    state_ambiguous: Family<StateLabels, Gauge>,
    state_desired: Family<StateLabels, Gauge>,
    state_applied: Family<StateLabels, Gauge>,
    state_pending: Family<StateLabels, Gauge>,
    state_default: Family<StateLabels, Gauge>,
    state_transitions: Family<StateTargetLabels, Counter>,
    http_attempts: Family<HttpAttemptLabels, Counter>,
    http_last_status_code: Family<StateLabels, F64Gauge>,
    http_retry_delay_seconds: Family<StateLabels, F64Gauge>,
    http_pending: Family<StateLabels, Gauge>,
    seen_serials: RwLock<HashSet<u64>>,
}

impl Metrics {
    /// Creates and registers every metric emitted by the daemon.
    ///
    /// `version` is embedded in `wattdog_build_info`. The registry is
    /// owned by this object and encoded on demand by the HTTP `/metrics` route.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "metric registration is intentionally explicit to keep names and help text in one place"
    )]
    pub fn new(version: &'static str) -> Self {
        let mut registry = Registry::default();

        let scanner_running = Gauge::default();
        let scanner_ble_running = Gauge::default();
        let observations_received = Counter::default();
        let observations_dropped = Counter::default();
        let writer_dropped_observations = Counter::default();
        let devices_seen = Gauge::default();
        let build_info = Family::<Vec<(String, String)>, Gauge>::default();
        let device_info = Family::<DeviceInfoLabels, Gauge>::default();
        let device_observations_received = Family::<DeviceLabels, Counter>::default();
        let last_seen_timestamp_seconds = Family::<DeviceLabels, F64Gauge>::default();
        let voltage1_volts = Family::<DeviceLabels, F64Gauge>::default();
        let voltage2_volts = Family::<DeviceLabels, F64Gauge>::default();
        let current_amperes = Family::<DeviceLabels, F64Gauge>::default();
        let power_watts = Family::<DeviceLabels, F64Gauge>::default();
        let temperature_celsius = Family::<DeviceLabels, F64Gauge>::default();
        let soc_percent = Family::<DeviceLabels, F64Gauge>::default();
        let runtime_minutes = Family::<DeviceLabels, F64Gauge>::default();
        let rssi_dbm = Family::<DeviceLabels, F64Gauge>::default();
        let power_status_code = Family::<DeviceLabels, F64Gauge>::default();
        let parquet_rows_written = Counter::default();
        let parquet_batches_written = Counter::default();
        let parquet_write_errors = Counter::default();
        let parquet_active_file_open = Gauge::default();
        let parquet_active_file_rows = Gauge::default();
        let parquet_last_successful_write_timestamp_seconds = F64Gauge::default();
        let parquet_rolls = Counter::default();
        let state_input_value = Family::<StateFieldLabels, F64Gauge>::default();
        let state_input_age_seconds = Family::<StateLabels, F64Gauge>::default();
        let state_stale = Family::<StateLabels, Gauge>::default();
        let state_ambiguous = Family::<StateLabels, Gauge>::default();
        let state_desired = Family::<StateLabels, Gauge>::default();
        let state_applied = Family::<StateLabels, Gauge>::default();
        let state_pending = Family::<StateLabels, Gauge>::default();
        let state_default = Family::<StateLabels, Gauge>::default();
        let state_transitions = Family::<StateTargetLabels, Counter>::default();
        let http_attempts = Family::<HttpAttemptLabels, Counter>::default();
        let http_last_status_code = Family::<StateLabels, F64Gauge>::default();
        let http_retry_delay_seconds = Family::<StateLabels, F64Gauge>::default();
        let http_pending = Family::<StateLabels, Gauge>::default();

        registry.register(
            "wattdog_build_info",
            "Daemon build information",
            build_info.clone(),
        );
        registry.register(
            "wattdog_up",
            "Whether any scanner transport is running",
            scanner_running.clone(),
        );
        registry.register(
            "wattdog_ble_scanner_running",
            "Whether BLE scanner transport is running",
            scanner_ble_running.clone(),
        );
        registry.register(
            "wattdog_observations_received",
            "Observed Thornwave advertisements",
            observations_received.clone(),
        );
        registry.register(
            "wattdog_observations_dropped",
            "Observations dropped before processing",
            observations_dropped.clone(),
        );
        registry.register(
            "wattdog_writer_dropped_observations",
            "Observations dropped before Parquet writer enqueue",
            writer_dropped_observations.clone(),
        );
        registry.register(
            "wattdog_devices_seen",
            "Number of Thornwave device serials seen",
            devices_seen.clone(),
        );
        registry.register(
            "wattdog_device_info",
            "Latest Thornwave device identity information",
            device_info.clone(),
        );
        registry.register(
            "wattdog_device_observations_received",
            "Observed advertisements by device",
            device_observations_received.clone(),
        );
        registry.register(
            "wattdog_last_observation_time_seconds",
            "Last observation timestamp by device",
            last_seen_timestamp_seconds.clone(),
        );
        registry.register(
            "wattdog_voltage1_volts",
            "Latest advertised voltage 1",
            voltage1_volts.clone(),
        );
        registry.register(
            "wattdog_voltage2_volts",
            "Latest advertised voltage 2",
            voltage2_volts.clone(),
        );
        registry.register(
            "wattdog_current_amperes",
            "Latest advertised current",
            current_amperes.clone(),
        );
        registry.register(
            "wattdog_power_watts",
            "Latest advertised power",
            power_watts.clone(),
        );
        registry.register(
            "wattdog_temperature_celsius",
            "Latest advertised temperature",
            temperature_celsius.clone(),
        );
        registry.register(
            "wattdog_soc_percent",
            "Latest advertised state of charge",
            soc_percent.clone(),
        );
        registry.register(
            "wattdog_runtime_minutes",
            "Latest advertised runtime",
            runtime_minutes.clone(),
        );
        registry.register(
            "wattdog_rssi_dbm",
            "Latest advertised RSSI",
            rssi_dbm.clone(),
        );
        registry.register(
            "wattdog_power_status_code",
            "Latest advertised power status code",
            power_status_code.clone(),
        );
        registry.register(
            "wattdog_parquet_rows_written",
            "Rows written to Parquet",
            parquet_rows_written.clone(),
        );
        registry.register(
            "wattdog_parquet_batches_written",
            "Record batches written to Parquet",
            parquet_batches_written.clone(),
        );
        registry.register(
            "wattdog_parquet_write_errors",
            "Parquet write errors",
            parquet_write_errors.clone(),
        );
        registry.register(
            "wattdog_parquet_active_file_open",
            "Whether a Parquet active file is open",
            parquet_active_file_open.clone(),
        );
        registry.register(
            "wattdog_parquet_active_file_rows",
            "Rows written to current active Parquet file",
            parquet_active_file_rows.clone(),
        );
        registry.register(
            "wattdog_parquet_last_successful_write_timestamp_seconds",
            "Unix timestamp of last successful Parquet write",
            parquet_last_successful_write_timestamp_seconds.clone(),
        );
        registry.register(
            "wattdog_parquet_rolls",
            "Completed Parquet file rolls",
            parquet_rolls.clone(),
        );
        registry.register(
            "wattdog_state_input_value",
            "Latest state input value",
            state_input_value.clone(),
        );
        registry.register(
            "wattdog_state_input_age_seconds",
            "Age of latest state input sample",
            state_input_age_seconds.clone(),
        );
        registry.register(
            "wattdog_state_stale",
            "Whether latest state input is stale",
            state_stale.clone(),
        );
        registry.register(
            "wattdog_state_ambiguous",
            "Whether state conditions are ambiguous",
            state_ambiguous.clone(),
        );
        registry.register(
            "wattdog_state_desired",
            "Desired state: on=1, off=0, unknown=-1",
            state_desired.clone(),
        );
        registry.register(
            "wattdog_state_applied",
            "Applied state: on=1, off=0, unknown=-1",
            state_applied.clone(),
        );
        registry.register(
            "wattdog_state_pending",
            "Whether an action is pending",
            state_pending.clone(),
        );
        registry.register(
            "wattdog_state_default",
            "Configured default state: on=1, off=0",
            state_default.clone(),
        );
        registry.register(
            "wattdog_state_transitions",
            "State desired transitions",
            state_transitions.clone(),
        );
        registry.register(
            "wattdog_http_attempts",
            "HTTP action attempts",
            http_attempts.clone(),
        );
        registry.register(
            "wattdog_http_last_status_code",
            "Last HTTP action status code",
            http_last_status_code.clone(),
        );
        registry.register(
            "wattdog_http_retry_delay_seconds",
            "Current HTTP action retry delay",
            http_retry_delay_seconds.clone(),
        );
        registry.register(
            "wattdog_http_pending",
            "Whether an HTTP action is pending",
            http_pending.clone(),
        );

        build_info
            .get_or_create(&vec![("version".to_string(), version.to_string())])
            .set(1);

        Self {
            registry,
            scanner_running,
            scanner_ble_running,
            observations_received,
            observations_dropped,
            writer_dropped_observations,
            devices_seen,
            device_info,
            device_observations_received,
            last_seen_timestamp_seconds,
            voltage1_volts,
            voltage2_volts,
            current_amperes,
            power_watts,
            temperature_celsius,
            soc_percent,
            runtime_minutes,
            rssi_dbm,
            power_status_code,
            parquet_rows_written,
            parquet_batches_written,
            parquet_write_errors,
            parquet_active_file_open,
            parquet_active_file_rows,
            parquet_last_successful_write_timestamp_seconds,
            parquet_rolls,
            state_input_value,
            state_input_age_seconds,
            state_stale,
            state_ambiguous,
            state_desired,
            state_applied,
            state_pending,
            state_default,
            state_transitions,
            http_attempts,
            http_last_status_code,
            http_retry_delay_seconds,
            http_pending,
            seen_serials: RwLock::new(HashSet::new()),
        }
    }

    /// Encodes the registry in Prometheus/OpenMetrics text format.
    ///
    /// # Errors
    ///
    /// Returns an error if metric text encoding fails.
    pub fn encode(&self) -> Result<String> {
        let mut output = String::new();
        encode(&mut output, &self.registry)?;
        Ok(output)
    }

    /// Sets the aggregate scanner-running gauge.
    pub fn set_scanner_running(&self, running: bool) {
        self.scanner_running.set(i64::from(running));
    }

    /// Sets the BLE scanner-running gauge.
    pub fn set_ble_running(&self, running: bool) {
        self.scanner_ble_running.set(i64::from(running));
    }

    /// Returns whether BLE scanning has been marked running.
    pub fn any_scanner_transport_running(&self) -> bool {
        self.scanner_ble_running.get() == 1
    }

    /// Records an observation dropped at the scanner callback queue boundary.
    pub fn observe_scanner_dropped(&self) {
        self.observations_dropped.inc();
    }

    /// Records an observation dropped before enqueueing to the Parquet writer.
    pub fn observe_writer_dropped(&self) {
        self.writer_dropped_observations.inc();
    }

    /// Updates global and per-device latest metrics for a received sample.
    ///
    /// This increments observation counters, tracks new serials, updates latest
    /// electrical values, and refreshes the low-cardinality device info metric.
    pub fn observe_sample(&self, sample: &Sample) {
        self.observations_received.inc();
        self.track_device(sample.serial);

        let labels = DeviceLabels {
            serial: sample.serial.to_string(),
        };
        self.device_observations_received
            .get_or_create(&labels)
            .inc();
        self.last_seen_timestamp_seconds
            .get_or_create(&labels)
            .set(timestamp_seconds(sample.observed_at.timestamp()));
        self.voltage1_volts
            .get_or_create(&labels)
            .set(f64::from(sample.voltage1_volts));
        self.voltage2_volts
            .get_or_create(&labels)
            .set(f64::from(sample.voltage2_volts));
        self.current_amperes
            .get_or_create(&labels)
            .set(f64::from(sample.current_amps));
        self.power_watts
            .get_or_create(&labels)
            .set(f64::from(sample.power_watts));
        self.temperature_celsius
            .get_or_create(&labels)
            .set(f64::from(sample.temperature_celsius));
        self.soc_percent
            .get_or_create(&labels)
            .set(sample.soc_percent.map_or(f64::NAN, f64::from));
        self.runtime_minutes
            .get_or_create(&labels)
            .set(sample.runtime_minutes.map_or(f64::NAN, f64::from));
        self.rssi_dbm
            .get_or_create(&labels)
            .set(f64::from(sample.rssi_dbm));
        self.power_status_code
            .get_or_create(&labels)
            .set(f64::from(sample.power_status_code));

        let info_labels = DeviceInfoLabels {
            serial: sample.serial.to_string(),
            name: sample.name.clone().unwrap_or_default(),
            model: sample.model.clone().unwrap_or_default(),
            address: sample.address_display.clone().unwrap_or_default(),
        };
        self.device_info.get_or_create(&info_labels).set(1);
    }

    /// Records a successful Parquet record-batch write.
    ///
    /// `rows` is the number of rows in the just-written batch and
    /// `active_file_rows` is the total row count in the current active file.
    pub fn parquet_rows_written(&self, rows: u64, active_file_rows: u64) {
        self.parquet_rows_written.inc_by(rows);
        self.parquet_batches_written.inc();
        self.parquet_active_file_rows
            .set(active_file_rows.try_into().unwrap_or(i64::MAX));
        self.parquet_last_successful_write_timestamp_seconds
            .set(timestamp_seconds(chrono::Utc::now().timestamp()));
    }

    /// Records a Parquet write or conversion error.
    pub fn parquet_write_error(&self) {
        self.parquet_write_errors.inc();
    }

    /// Sets whether the writer currently has an active `.parquet.inprogress` file.
    pub fn set_parquet_active_file_open(&self, open: bool) {
        self.parquet_active_file_open.set(i64::from(open));
    }

    /// Sets the number of rows written to the active Parquet file.
    pub fn set_parquet_active_file_rows(&self, rows: u64) {
        self.parquet_active_file_rows
            .set(rows.try_into().unwrap_or(i64::MAX));
    }

    /// Records a completed Parquet file roll/finalization.
    pub fn parquet_roll(&self) {
        self.parquet_rolls.inc();
    }

    /// Updates latest input metrics for one configured state.
    pub fn observe_state_input(&self, name: &str, field: &str, value: f64, age_seconds: f64) {
        self.state_input_value
            .get_or_create(&StateFieldLabels {
                name: name.to_string(),
                field: field.to_string(),
            })
            .set(value);
        self.set_state_input_age(name, age_seconds);
    }

    /// Updates the latest input age for one configured state.
    pub fn set_state_input_age(&self, name: &str, age_seconds: f64) {
        self.state_input_age_seconds
            .get_or_create(&state_labels(name))
            .set(age_seconds);
    }

    /// Updates current gauges for one configured state.
    #[expect(
        clippy::too_many_arguments,
        reason = "state status metrics are one small call site; a struct would only move these fields"
    )]
    pub fn set_state_status(
        &self,
        name: &str,
        stale: bool,
        ambiguous: bool,
        desired: Option<BinaryState>,
        applied: Option<BinaryState>,
        pending: Option<BinaryState>,
        default_state: BinaryState,
    ) {
        let labels = state_labels(name);
        self.state_stale
            .get_or_create(&labels)
            .set(bool_metric(stale));
        self.state_ambiguous
            .get_or_create(&labels)
            .set(bool_metric(ambiguous));
        self.state_desired
            .get_or_create(&labels)
            .set(state_metric(desired));
        self.state_applied
            .get_or_create(&labels)
            .set(state_metric(applied));
        self.state_pending
            .get_or_create(&labels)
            .set(bool_metric(pending.is_some()));
        self.http_pending
            .get_or_create(&labels)
            .set(bool_metric(pending.is_some()));
        self.state_default
            .get_or_create(&labels)
            .set(default_state.metric_value());
    }

    /// Records a desired-state transition.
    pub fn state_transition(&self, name: &str, target: BinaryState) {
        self.state_transitions
            .get_or_create(&StateTargetLabels {
                name: name.to_string(),
                target: target.as_str().to_string(),
            })
            .inc();
    }

    /// Records one HTTP action attempt outcome.
    pub fn http_attempt(
        &self,
        name: &str,
        target: BinaryState,
        success: bool,
        status: Option<u16>,
    ) {
        self.http_attempts
            .get_or_create(&HttpAttemptLabels {
                name: name.to_string(),
                target: target.as_str().to_string(),
                result: if success { "success" } else { "failure" }.to_string(),
            })
            .inc();

        if let Some(status) = status {
            self.http_last_status_code
                .get_or_create(&state_labels(name))
                .set(f64::from(status));
        }
    }

    /// Updates the current retry delay for one configured state.
    pub fn set_http_retry_delay(&self, name: &str, seconds: f64) {
        self.http_retry_delay_seconds
            .get_or_create(&state_labels(name))
            .set(seconds);
    }

    fn track_device(&self, serial: u64) {
        let mut seen = self
            .seen_serials
            .write()
            .expect("seen serial lock poisoned");
        if seen.insert(serial) {
            self.devices_seen
                .set(seen.len().try_into().unwrap_or(i64::MAX));
        }
    }
}

fn state_labels(name: &str) -> StateLabels {
    StateLabels {
        name: name.to_string(),
    }
}

fn bool_metric(value: bool) -> i64 {
    i64::from(value)
}

fn state_metric(state: Option<BinaryState>) -> i64 {
    state.map_or(-1, BinaryState::metric_value)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "current Unix timestamps fit exactly enough for Prometheus seconds gauges"
)]
fn timestamp_seconds(timestamp: i64) -> f64 {
    timestamp as f64
}
