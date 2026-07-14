//! Orchestrates all configured runtime states.

use std::time::Instant;

use anyhow::{Context, Result};

use crate::{
    config::{ActionStepConfig, BinaryState, Config, HttpConfig},
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
    /// Zero-based sequence step.
    pub step_index: usize,
    /// HTTP sequence step to execute.
    pub action: ActionStepConfig,
    /// Polling deadline, when this is a JSON wait step.
    pub deadline: Option<Instant>,
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
            let previous_applied = runtime.applied_state;
            let decision = runtime.tick(now, self.http.retry_initial, self.http.retry_max);

            if let Some(sample_at) = runtime.latest_sample_at {
                metrics.set_state_input_age(
                    &runtime.config.name,
                    now.saturating_duration_since(sample_at).as_secs_f64(),
                );
            }
            let due = match decision {
                TickDecision::None => None,
                TickDecision::WaitTimedOut { target, step_index } => {
                    metrics.action_wait_timeout(&runtime.config.name, target);
                    runtime.record_wait_timeout(
                        target,
                        step_index,
                        now,
                        self.http.retry_initial,
                        self.http.retry_max,
                    );
                    None
                }
                TickDecision::Attempt { target, step_index } => Some((target, step_index)),
            };

            if previous_applied != runtime.applied_state {
                if let Some(applied) = runtime.applied_state {
                    metrics.state_transition(&runtime.config.name, applied);
                }
                metrics.set_action_sequence_step(&runtime.config.name, None);
            }
            metrics.set_action_sequence_step(
                &runtime.config.name,
                runtime
                    .pending_target_state
                    .map(|_| runtime.action_step_index),
            );
            metrics.set_http_retry_delay(
                &runtime.config.name,
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

            let Some((target, step_index)) = due else {
                continue;
            };
            let Some(action) = runtime.transition(target).action(step_index) else {
                continue;
            };
            let deadline = match &action {
                ActionStepConfig::WaitForJson { timeout, .. } => {
                    runtime.step_started_at.map(|started| started + *timeout)
                }
                ActionStepConfig::Request { .. } | ActionStepConfig::Delay { .. } => None,
            };
            actions.push(DueAction {
                state_index,
                state_name: runtime.config.name.clone(),
                target,
                step_index,
                action,
                deadline,
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
            ActionOutcome::Success { status } | ActionOutcome::Pending { status } => {
                (true, Some(*status))
            }
            ActionOutcome::Failure { status, .. } => (false, *status),
        };
        metrics.http_attempt(&action.state_name, action.target, success, status);
        if runtime.wait_timed_out(action.target, action.step_index, now) {
            metrics.action_wait_timeout(&action.state_name, action.target);
            runtime.record_wait_timeout(
                action.target,
                action.step_index,
                now,
                self.http.retry_initial,
                self.http.retry_max,
            );
        } else {
            runtime.record_action_result(
                action.target,
                action.step_index,
                outcome,
                now,
                self.http.retry_initial,
                self.http.retry_max,
            );
        }
        if previous_applied != runtime.applied_state {
            metrics.state_transition(&action.state_name, action.target);
        }
        metrics.set_action_sequence_step(
            &action.state_name,
            runtime
                .pending_target_state
                .map(|_| runtime.action_step_index),
        );
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
            ActionStepConfig, BinaryState, ComparisonOp, Config, DataConfig, HttpConfig,
            HttpMethod, MeasurementField, MetricsConfig, RollPeriod, StateConfig, TransitionConfig,
            WaitTimeoutPolicy,
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
            .execute(
                &action.state_name,
                action.target,
                action.step_index,
                &action.action,
                action.deadline,
            )
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

    #[test]
    fn polling_response_after_deadline_uses_timeout_policy() {
        let now = std::time::Instant::now();
        let mut config = config("12345");
        config.states[0].on.url = None;
        config.states[0].on.actions = vec![ActionStepConfig::WaitForJson {
            url: Url::parse("http://127.0.0.1/atx").expect("url"),
            pointer: "/result/leds/power".to_string(),
            expected: serde_json::Value::Bool(false),
            poll_every: Duration::from_secs(1),
            timeout: Duration::from_secs(1),
            on_timeout: WaitTimeoutPolicy::Continue,
        }];
        let metrics = Metrics::new("test");
        let mut engine = StateEngine::new(&config).expect("engine");
        engine.observe_sample(&sample(SERIAL, 11.0), &metrics, now);
        let action = engine
            .collect_due_actions(&metrics, now)
            .pop()
            .expect("due action");

        engine.record_action_result(
            &action,
            ActionOutcome::Failure {
                status: None,
                error: Some("timeout".to_string()),
            },
            &metrics,
            now + Duration::from_secs(1),
        );

        assert_eq!(engine.states[0].applied_state, Some(BinaryState::On));
        let encoded = metrics.encode().expect("metrics");
        assert!(
            encoded.contains("wattdog_action_wait_timeouts_total{name=\"relay\",target=\"on\"} 1")
        );
        assert!(
            encoded.contains("wattdog_state_transitions_total{name=\"relay\",target=\"on\"} 1")
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
            hold_for: Duration::ZERO,
            url: Some(Url::parse(url).expect("valid url")),
            actions: Vec::new(),
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
