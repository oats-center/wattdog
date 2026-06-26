//! TOML configuration for `wattdog`.

use std::{
    cmp::Ordering, collections::HashSet, net::SocketAddr, path::Path, path::PathBuf, time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use url::Url;

use crate::sample::Sample;

/// Complete daemon configuration loaded from TOML.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Local sample storage configuration.
    pub data: DataConfig,

    /// Metrics server configuration.
    #[serde(default)]
    pub metrics: MetricsConfig,

    /// HTTP action configuration.
    pub http: HttpConfig,

    /// Threshold state definitions.
    pub states: Vec<StateConfig>,
}

/// Local Parquet storage configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataConfig {
    /// Directory where local Parquet data files are written.
    pub dir: PathBuf,

    /// How often to close the active Parquet file and start a new one.
    #[serde(default)]
    pub roll: RollPeriod,
}

/// File roll period for completed Parquet files.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RollPeriod {
    /// Close and finalize the active Parquet file at each UTC hour boundary.
    #[default]
    Hourly,

    /// Close and finalize the active Parquet file at each UTC day boundary.
    Daily,
}

/// Metrics endpoint configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    /// Metrics HTTP listen address.
    #[serde(default = "default_metrics_listen")]
    pub listen: SocketAddr,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            listen: default_metrics_listen(),
        }
    }
}

/// Shared HTTP action configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    /// HTTP method used for actions.
    pub method: HttpMethod,

    /// Per-request timeout.
    #[serde(default = "default_http_timeout", with = "humantime_serde")]
    pub timeout: Duration,

    /// First retry delay after an action failure.
    #[serde(default = "default_retry_initial", with = "humantime_serde")]
    pub retry_initial: Duration,

    /// Maximum retry delay after repeated failures.
    #[serde(default = "default_retry_max", with = "humantime_serde")]
    pub retry_max: Duration,

    /// Reject action URLs that are not HTTPS.
    #[serde(default)]
    pub require_https: bool,

    /// Allow invalid TLS certificates for action endpoints.
    #[serde(default)]
    pub allow_invalid_certs: bool,
}

/// Supported HTTP action methods.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// POST action request.
    Post,

    /// PUT action request.
    Put,
}

/// One configured binary output state driven by one Thornwave measurement.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateConfig {
    /// Unique state name used in metrics/logs.
    pub name: String,

    /// Thornwave device serial number, decimal only.
    pub serial: String,

    /// Measurement field used for threshold comparisons.
    pub field: MeasurementField,

    /// Initial convergence target when the input is fresh and neutral.
    pub default_state: BinaryState,

    /// Maximum age for latest input before decisions pause.
    #[serde(with = "humantime_serde")]
    pub stale_after: Duration,

    /// Transition that desires ON.
    pub on: TransitionConfig,

    /// Transition that desires OFF.
    pub off: TransitionConfig,
}

/// Binary state used for desired/applied action state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BinaryState {
    /// Enabled state.
    On,

    /// Disabled state.
    Off,
}

/// One threshold transition target.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionConfig {
    /// Comparison operator.
    pub op: ComparisonOp,

    /// Threshold value.
    pub value: f64,

    /// Required continuous match duration.
    #[serde(with = "humantime_serde")]
    pub duration: Duration,

    /// Absolute action URL. Never log this in full.
    pub url: Url,
}

/// Supported Thornwave measurement fields for action thresholds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementField {
    /// Voltage channel 1 in volts.
    Voltage1Volts,

    /// Voltage channel 2 in volts.
    Voltage2Volts,

    /// Current in amperes.
    CurrentAmps,

    /// Power in watts.
    PowerWatts,
}

/// Supported threshold comparison operators.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum ComparisonOp {
    /// Less than.
    #[serde(rename = "<")]
    Lt,

    /// Less than or equal.
    #[serde(rename = "<=")]
    Le,

    /// Greater than.
    #[serde(rename = ">")]
    Gt,

    /// Greater than or equal.
    #[serde(rename = ">=")]
    Ge,
}

impl Config {
    /// Loads and validates configuration from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, parsed, or validated.
    pub async fn load(path: &Path) -> Result<Self> {
        let contents = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    /// Validates cross-field config rules that serde cannot express.
    ///
    /// # Errors
    ///
    /// Returns an error if required values are empty, invalid, or internally inconsistent.
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.states.is_empty(), "at least one state is required");
        ensure!(
            self.http.retry_initial > Duration::ZERO,
            "http.retry_initial must be greater than zero"
        );
        ensure!(
            self.http.retry_max > Duration::ZERO,
            "http.retry_max must be greater than zero"
        );
        ensure!(
            self.http.retry_initial <= self.http.retry_max,
            "http.retry_initial must be <= http.retry_max"
        );
        ensure!(
            self.http.retry_max <= Duration::from_secs(5),
            "http.retry_max must be <= 5s"
        );

        let mut names = HashSet::new();
        for state in &self.states {
            ensure!(
                !state.name.trim().is_empty(),
                "state name must not be empty"
            );
            ensure!(
                names.insert(&state.name),
                "duplicate state name {}",
                state.name
            );
            ensure!(
                !state.serial.trim().is_empty(),
                "state {} serial must not be empty",
                state.name
            );
            state.serial.parse::<u64>().with_context(|| {
                format!(
                    "state {} serial must be an unsigned decimal integer",
                    state.name
                )
            })?;
            ensure!(
                state.stale_after > Duration::ZERO,
                "state {} stale_after must be greater than zero",
                state.name
            );
            validate_transition_url(&state.name, "on", &state.on.url, self.http.require_https)?;
            validate_transition_url(&state.name, "off", &state.off.url, self.http.require_https)?;
            validate_thresholds(state)?;
        }

        Ok(())
    }
}

impl MeasurementField {
    /// Returns this field's numeric value from a sample.
    #[must_use]
    pub fn value(self, sample: &Sample) -> f64 {
        match self {
            Self::Voltage1Volts => f64::from(sample.voltage1_volts),
            Self::Voltage2Volts => f64::from(sample.voltage2_volts),
            Self::CurrentAmps => f64::from(sample.current_amps),
            Self::PowerWatts => f64::from(sample.power_watts),
        }
    }

    /// Metric-safe field name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Voltage1Volts => "voltage1_volts",
            Self::Voltage2Volts => "voltage2_volts",
            Self::CurrentAmps => "current_amps",
            Self::PowerWatts => "power_watts",
        }
    }
}

impl BinaryState {
    /// Numeric value used for metrics.
    #[must_use]
    pub fn metric_value(self) -> i64 {
        match self {
            Self::On => 1,
            Self::Off => 0,
        }
    }

    /// Metric-safe state name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }
}

fn validate_transition_url(
    state_name: &str,
    transition: &str,
    url: &Url,
    require_https: bool,
) -> Result<()> {
    match url.scheme() {
        "https" => Ok(()),
        "http" if !require_https => Ok(()),
        "http" => bail!("state {state_name} {transition} URL must use https"),
        scheme => bail!("state {state_name} {transition} URL has unsupported scheme {scheme}"),
    }
}

fn validate_thresholds(state: &StateConfig) -> Result<()> {
    let on_upper = upper_bound(state.on.op);
    let off_upper = upper_bound(state.off.op);
    ensure!(
        on_upper != off_upper,
        "state {} on/off thresholds must use opposite directions",
        state.name
    );

    let (upper, lower) = if on_upper {
        (&state.on, &state.off)
    } else {
        (&state.off, &state.on)
    };

    let ordering = upper.value.total_cmp(&lower.value);
    let overlaps = ordering == Ordering::Greater
        || (ordering == Ordering::Equal && inclusive(upper.op) && inclusive(lower.op));
    ensure!(!overlaps, "state {} on/off thresholds overlap", state.name);
    Ok(())
}

fn upper_bound(op: ComparisonOp) -> bool {
    matches!(op, ComparisonOp::Lt | ComparisonOp::Le)
}

fn inclusive(op: ComparisonOp) -> bool {
    matches!(op, ComparisonOp::Le | ComparisonOp::Ge)
}

fn default_metrics_listen() -> SocketAddr {
    "127.0.0.1:9107"
        .parse()
        .expect("valid default listen address")
}

fn default_http_timeout() -> Duration {
    Duration::from_secs(5)
}

fn default_retry_initial() -> Duration {
    Duration::from_secs(1)
}

fn default_retry_max() -> Duration {
    Duration::from_secs(5)
}

#[cfg(test)]
mod tests {
    use super::Config;

    const VALID: &str = r#"
[data]
dir = "/var/lib/wattdog"

[http]
method = "POST"

[[states]]
name = "battery_relay"
serial = "12345678"
field = "voltage1_volts"
default_state = "off"
stale_after = "30s"

[states.on]
op = "<="
value = 12.0
duration = "30s"
url = "http://127.0.0.1/on"

[states.off]
op = ">="
value = 12.6
duration = "5m"
url = "http://127.0.0.1/off"
"#;

    #[test]
    fn valid_minimal_config_loads() {
        parse_ok(VALID);
    }

    #[test]
    fn duplicate_state_names_rejected() {
        parse_err(&format!(
            "{VALID}\n{}",
            r#"
[[states]]
name = "battery_relay"
serial = "87654321"
field = "voltage1_volts"
default_state = "off"
stale_after = "30s"

[states.on]
op = "<="
value = 12.0
duration = "30s"
url = "http://127.0.0.1/on2"

[states.off]
op = ">="
value = 12.6
duration = "5m"
url = "http://127.0.0.1/off2"
"#
        ));
    }

    #[test]
    fn empty_states_rejected() {
        parse_err(
            r#"
states = []

[data]
dir = "/tmp"

[http]
method = "POST"
"#,
        );
    }

    #[test]
    fn unknown_measurement_field_rejected_by_serde() {
        parse_err(&VALID.replace("voltage1_volts", "temperature_celsius"));
    }

    #[test]
    fn invalid_url_rejected() {
        parse_err(&VALID.replace("http://127.0.0.1/on", "not a url"));
    }

    #[test]
    fn require_https_rejects_http_action_url() {
        parse_err(&VALID.replace(
            "method = \"POST\"",
            "method = \"POST\"\nrequire_https = true",
        ));
    }

    #[test]
    fn retry_max_over_five_seconds_rejected() {
        parse_err(&VALID.replace("method = \"POST\"", "method = \"POST\"\nretry_max = \"6s\""));
    }

    #[test]
    fn retry_initial_over_retry_max_rejected() {
        parse_err(&VALID.replace(
            "method = \"POST\"",
            "method = \"POST\"\nretry_initial = \"5s\"\nretry_max = \"1s\"",
        ));
    }

    #[test]
    fn overlapping_thresholds_rejected() {
        parse_err(&VALID.replace("value = 12.6", "value = 12.0"));
    }

    fn parse_ok(toml: &str) -> Config {
        let config: Config = toml::from_str(toml).expect("config should parse");
        config.validate().expect("config should validate");
        config
    }

    fn parse_err(toml: &str) {
        if let Ok(config) = toml::from_str::<Config>(toml) {
            assert!(config.validate().is_err(), "config should be invalid");
        }
    }
}
