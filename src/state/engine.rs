//! Orchestrates all configured runtime states.

use std::time::Instant;

use anyhow::{Context, Result};

use crate::{
    config::{BinaryState, Config, HttpConfig},
    metrics::Metrics,
    sample::Sample,
    state::machine::{ActionOutcome, RuntimeState, TickDecision},
};

/// Action attempt selected by a synchronous engine tick.
#[derive(Debug)]
pub struct DueAction {
    /// Index of the runtime state that owns this action.
    pub state_index: usize,
    /// Configured state name.
    pub state_name: String,
    /// Target output state.
    pub target: BinaryState,
    /// HTTP endpoint to call.
    pub url: url::Url,
}

/// Runtime state engine for every configured output.
#[derive(Debug)]
pub struct StateEngine {
    states: Vec<RuntimeState>,
    http: HttpConfig,
}

impl StateEngine {
    /// Builds runtime state from static configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any configured state serial is not decimal `u64`.
    pub fn new(config: &Config) -> Result<Self> {
        let states = config
            .states
            .iter()
            .map(|state| {
                let serial = state.serial.parse::<u64>().with_context(|| {
                    format!(
                        "state {} serial must be an unsigned decimal integer",
                        state.name
                    )
                })?;
                Ok(RuntimeState::new(state.clone(), serial))
            })
            .collect::<Result<_>>()?;

        Ok(Self {
            states,
            http: HttpConfig {
                method: config.http.method,
                timeout: config.http.timeout,
                retry_initial: config.http.retry_initial,
                retry_max: config.http.retry_max,
                require_https: config.http.require_https,
                allow_invalid_certs: config.http.allow_invalid_certs,
            },
        })
    }

    /// Observes one sample in every configured state.
    pub fn observe_sample(&mut self, sample: &Sample, metrics: &Metrics, now: Instant) {
        for state in &mut self.states {
            state.observe_sample(sample, now);
            if sample.serial == state.serial {
                metrics.observe_state_input(
                    &state.config.name,
                    state.config.field.as_str(),
                    state.config.field.value(sample),
                    0.0,
                );
            }
        }
    }

    /// Advances all states and returns due actions without performing I/O.
    pub fn collect_due_actions(&mut self, metrics: &Metrics, now: Instant) -> Vec<DueAction> {
        let mut actions = Vec::new();

        for (state_index, runtime) in self.states.iter_mut().enumerate() {
            let decision = runtime.tick(now, self.http.retry_initial, self.http.retry_max);

            if let Some(sample_at) = runtime.latest_sample_at {
                metrics.set_state_input_age(
                    &runtime.config.name,
                    now.saturating_duration_since(sample_at).as_secs_f64(),
                );
            }
            metrics.set_state_status(
                &runtime.config.name,
                runtime.stale,
                runtime.ambiguous,
                runtime.desired_state,
                runtime.applied_state,
                runtime.pending_target_state,
                runtime.config.default_state,
            );

            let TickDecision::Attempt(target) = decision else {
                continue;
            };

            let url = match target {
                BinaryState::On => runtime.config.on.url.clone(),
                BinaryState::Off => runtime.config.off.url.clone(),
            };

            actions.push(DueAction {
                state_index,
                state_name: runtime.config.name.clone(),
                target,
                url,
            });
        }

        actions
    }

    /// Records an action attempt result.
    pub fn record_action_result(
        &mut self,
        action: &DueAction,
        outcome: ActionOutcome,
        metrics: &Metrics,
        now: Instant,
    ) {
        let Some(runtime) = self.states.get_mut(action.state_index) else {
            return;
        };

        let previous_applied = runtime.applied_state;
        let (success, status) = match &outcome {
            ActionOutcome::Success { status } => (true, Some(*status)),
            ActionOutcome::Failure { status, .. } => (false, *status),
        };
        metrics.http_attempt(&action.state_name, action.target, success, status);
        runtime.record_action_result(
            action.target,
            outcome,
            now,
            self.http.retry_initial,
            self.http.retry_max,
        );
        if success && previous_applied != runtime.applied_state {
            metrics.state_transition(&action.state_name, action.target);
        }
        metrics.set_http_retry_delay(
            &action.state_name,
            runtime.next_retry_at.map_or(0.0, |retry_at| {
                retry_at.saturating_duration_since(now).as_secs_f64()
            }),
        );
        metrics.set_state_status(
            &runtime.config.name,
            runtime.stale,
            runtime.ambiguous,
            runtime.desired_state,
            runtime.applied_state,
            runtime.pending_target_state,
            runtime.config.default_state,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use chrono::Utc;
    use url::Url;

    use crate::{
        action::http::ActionClient,
        config::{
            BinaryState, ComparisonOp, Config, DataConfig, HttpConfig, HttpMethod,
            MeasurementField, MetricsConfig, RollPeriod, StateConfig, TransitionConfig,
        },
        metrics::Metrics,
        sample::Sample,
    };

    use super::{ActionOutcome, StateEngine};

    const SERIAL: u64 = 12_345;

    #[test]
    fn new_parses_serial_and_rejects_invalid_serial() {
        assert_eq!(
            StateEngine::new(&config("12345")).expect("engine").states[0].serial,
            SERIAL
        );
        assert!(StateEngine::new(&config("nope")).is_err());
    }

    #[test]
    fn observe_sample_routes_by_serial() {
        let now = std::time::Instant::now();
        let mut engine = StateEngine::new(&config("12345")).expect("engine");

        let metrics = Metrics::new("test");

        engine.observe_sample(&sample(999, 11.0), &metrics, now);
        assert_eq!(engine.states[0].latest_value, None);

        engine.observe_sample(&sample(SERIAL, 11.0), &metrics, now);
        assert_eq!(engine.states[0].latest_value, Some(11.0));
    }

    #[tokio::test]
    async fn dry_run_tick_records_applied_state() {
        let now = std::time::Instant::now();
        let config = config("12345");
        let client = ActionClient::new(&config.http, true).expect("client");
        let metrics = Metrics::new("test");
        let mut engine = StateEngine::new(&config).expect("engine");

        engine.observe_sample(&sample(SERIAL, 11.0), &metrics, now);
        let action = engine
            .collect_due_actions(&metrics, now)
            .pop()
            .expect("due action");
        let outcome = client
            .send(&action.state_name, action.target, &action.url)
            .await;
        engine.record_action_result(&action, outcome, &metrics, now);

        assert_eq!(engine.states[0].applied_state, Some(BinaryState::On));
    }

    #[test]
    fn transition_metric_counts_only_successful_apply() {
        let now = std::time::Instant::now();
        let config = config("12345");
        let metrics = Metrics::new("test");
        let mut engine = StateEngine::new(&config).expect("engine");

        engine.observe_sample(&sample(SERIAL, 11.0), &metrics, now);
        let mut actions = engine.collect_due_actions(&metrics, now);
        assert!(
            !metrics
                .encode()
                .expect("metrics")
                .contains("state_transitions")
        );

        let action = actions.pop().expect("due action");
        engine.record_action_result(
            &action,
            ActionOutcome::Success { status: 204 },
            &metrics,
            now,
        );

        assert!(
            metrics
                .encode()
                .expect("metrics")
                .contains("wattdog_state_transitions_total{name=\"relay\",target=\"on\"} 1")
        );
    }

    fn config(serial: &str) -> Config {
        Config {
            data: DataConfig {
                dir: PathBuf::new(),
                roll: RollPeriod::Hourly,
            },
            metrics: MetricsConfig::default(),
            http: HttpConfig {
                method: HttpMethod::Post,
                timeout: Duration::from_secs(1),
                retry_initial: Duration::from_millis(1),
                retry_max: Duration::from_millis(1),
                require_https: false,
                allow_invalid_certs: false,
            },
            states: vec![StateConfig {
                name: "relay".to_string(),
                serial: serial.to_string(),
                field: MeasurementField::Voltage1Volts,
                default_state: BinaryState::Off,
                stale_after: Duration::from_secs(30),
                on: transition(ComparisonOp::Le, 12.0, "http://127.0.0.1/on"),
                off: transition(ComparisonOp::Ge, 12.6, "http://127.0.0.1/off"),
            }],
        }
    }

    fn transition(op: ComparisonOp, value: f64, url: &str) -> TransitionConfig {
        TransitionConfig {
            op,
            value,
            duration: Duration::ZERO,
            url: Url::parse(url).expect("valid url"),
        }
    }

    fn sample(serial: u64, voltage1_volts: f32) -> Sample {
        Sample {
            schema_version: 1,
            observed_at: Utc::now(),
            serial,
            address_raw: 0,
            address_display: None,
            address_kind: None,
            name: None,
            model: None,
            firmware_version_bcd: 0,
            firmware_version: None,
            hardware_revision_bcd: 0,
            hardware_revision: None,
            device_time_raw: 0,
            flags_raw: 0,
            voltage1_volts,
            voltage2_volts: 0.0,
            current_amps: 0.0,
            power_watts: 0.0,
            coulomb_meter_raw: 0.0,
            power_meter_raw: 0.0,
            temperature_celsius: 0.0,
            temperature_is_external: false,
            power_status_code: 0,
            power_status: None,
            soc_raw: 0,
            soc_percent: None,
            runtime_raw: 0,
            runtime_minutes: None,
            rssi_dbm: 0,
        }
    }
}
