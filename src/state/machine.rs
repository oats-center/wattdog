//! Runtime state machine for one configured binary output.

use std::time::{Duration, Instant};

use crate::{
    config::{BinaryState, StateConfig},
    sample::Sample,
};

use super::condition;

/// In-memory state for one configured threshold output.
#[derive(Debug)]
pub struct RuntimeState {
    /// Static state configuration.
    pub config: StateConfig,
    /// Parsed Thornwave device serial number.
    pub serial: u64,
    /// Latest matching measurement value.
    pub latest_value: Option<f64>,
    /// Time when the latest matching sample was observed.
    pub latest_sample_at: Option<Instant>,
    /// Current desired output state.
    pub desired_state: Option<BinaryState>,
    /// Last successfully applied output state.
    pub applied_state: Option<BinaryState>,
    /// Target currently awaiting action success.
    pub pending_target_state: Option<BinaryState>,
    /// Start time for the current continuous ON condition.
    pub on_condition_started_at: Option<Instant>,
    /// Start time for the current continuous OFF condition.
    pub off_condition_started_at: Option<Instant>,
    /// Number of consecutive failed attempts for the pending target.
    pub retry_attempt: u32,
    /// Next time a retry may be attempted.
    pub next_retry_at: Option<Instant>,
    /// Last HTTP status returned by an action attempt.
    pub last_http_status: Option<u16>,
    /// Last HTTP error returned by an action attempt.
    pub last_http_error: Option<String>,
    /// Last successful action time.
    pub last_success_at: Option<Instant>,
    /// Whether the latest sample is stale.
    pub stale: bool,
    /// Whether ON and OFF conditions are both satisfied.
    pub ambiguous: bool,
}

/// Result of one state-machine tick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TickDecision {
    /// No action should be attempted now.
    None,
    /// Attempt to apply the target state.
    Attempt(BinaryState),
}

/// Outcome of an attempted action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionOutcome {
    /// Action succeeded with an HTTP status.
    Success { status: u16 },
    /// Action failed with optional HTTP status and transport error.
    Failure {
        /// HTTP status, when a response was received.
        status: Option<u16>,
        /// Transport or request error, when available.
        error: Option<String>,
    },
}

impl RuntimeState {
    /// Creates empty runtime state for one configured output.
    #[must_use]
    pub fn new(config: StateConfig, serial: u64) -> Self {
        Self {
            config,
            serial,
            latest_value: None,
            latest_sample_at: None,
            desired_state: None,
            applied_state: None,
            pending_target_state: None,
            on_condition_started_at: None,
            off_condition_started_at: None,
            retry_attempt: 0,
            next_retry_at: None,
            last_http_status: None,
            last_http_error: None,
            last_success_at: None,
            stale: false,
            ambiguous: false,
        }
    }

    /// Observes a sample if its serial matches this state.
    pub fn observe_sample(&mut self, sample: &Sample, now: Instant) {
        if sample.serial == self.serial {
            self.latest_value = Some(self.config.field.value(sample));
            self.latest_sample_at = Some(now);
        }
    }

    /// Advances threshold timers and returns any due action attempt.
    pub fn tick(
        &mut self,
        now: Instant,
        retry_initial: Duration,
        retry_max: Duration,
    ) -> TickDecision {
        let _ = (retry_initial, retry_max);

        let (Some(value), Some(sample_at)) = (self.latest_value, self.latest_sample_at) else {
            return self.due_attempt(now);
        };

        self.stale = now.saturating_duration_since(sample_at) > self.config.stale_after;
        if self.stale {
            return self.due_attempt(now);
        }

        let on_matches = condition::matches(self.config.on.op, value, self.config.on.value);
        let off_matches = condition::matches(self.config.off.op, value, self.config.off.value);

        update_timer(&mut self.on_condition_started_at, on_matches, now);
        update_timer(&mut self.off_condition_started_at, off_matches, now);

        let on_satisfied = timer_satisfied(
            self.on_condition_started_at,
            on_matches,
            now,
            self.config.on.duration,
        );
        let off_satisfied = timer_satisfied(
            self.off_condition_started_at,
            off_matches,
            now,
            self.config.off.duration,
        );

        self.ambiguous = on_satisfied && off_satisfied;
        if self.ambiguous {
            return TickDecision::None;
        }

        if on_satisfied {
            self.desired_state = Some(BinaryState::On);
        } else if off_satisfied {
            self.desired_state = Some(BinaryState::Off);
        } else if !on_matches
            && !off_matches
            && self.applied_state.is_none()
            && self.desired_state.is_none()
        {
            self.desired_state = Some(self.config.default_state);
        }

        self.due_attempt(now)
    }

    /// Records the result of an attempted action.
    pub fn record_action_result(
        &mut self,
        target: BinaryState,
        outcome: ActionOutcome,
        now: Instant,
        retry_initial: Duration,
        retry_max: Duration,
    ) {
        match outcome {
            ActionOutcome::Success { status } => {
                self.applied_state = Some(target);
                self.pending_target_state = None;
                self.retry_attempt = 0;
                self.next_retry_at = None;
                self.last_http_status = Some(status);
                self.last_http_error = None;
                self.last_success_at = Some(now);
            }
            ActionOutcome::Failure { status, error } => {
                self.pending_target_state = Some(target);
                self.last_http_status = status;
                self.last_http_error = error;
                self.next_retry_at =
                    Some(now + backoff(self.retry_attempt, retry_initial, retry_max));
                self.retry_attempt = self.retry_attempt.saturating_add(1);
            }
        }
    }

    fn due_attempt(&mut self, now: Instant) -> TickDecision {
        let Some(desired) = self.desired_state else {
            return TickDecision::None;
        };
        if Some(desired) == self.applied_state {
            self.pending_target_state = None;
            return TickDecision::None;
        }
        if self.pending_target_state != Some(desired) {
            self.pending_target_state = Some(desired);
            self.retry_attempt = 0;
            self.next_retry_at = None;
        }
        if self.next_retry_at.is_none_or(|retry_at| now >= retry_at) {
            TickDecision::Attempt(desired)
        } else {
            TickDecision::None
        }
    }
}

fn update_timer(timer: &mut Option<Instant>, matched: bool, now: Instant) {
    if matched {
        timer.get_or_insert(now);
    } else {
        *timer = None;
    }
}

fn timer_satisfied(
    started_at: Option<Instant>,
    matched: bool,
    now: Instant,
    duration: Duration,
) -> bool {
    matched
        && started_at
            .is_some_and(|started_at| now.saturating_duration_since(started_at) >= duration)
}

fn backoff(attempt: u32, initial: Duration, max: Duration) -> Duration {
    initial.saturating_mul(1 << attempt.min(10)).min(max)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use chrono::Utc;
    use url::Url;

    use crate::{
        config::{BinaryState, ComparisonOp, MeasurementField, StateConfig, TransitionConfig},
        sample::Sample,
    };

    use super::{ActionOutcome, RuntimeState, TickDecision, backoff};

    const SERIAL: u64 = 12_345;

    #[test]
    fn neutral_default_sets_desired_state() {
        let now = Instant::now();
        let mut state = runtime(
            config(Duration::from_secs(5), Duration::from_secs(5)),
            now,
            12.3,
        );

        assert_eq!(
            state.tick(now, Duration::from_secs(1), Duration::from_secs(5)),
            TickDecision::Attempt(BinaryState::Off)
        );
        assert_eq!(state.desired_state, Some(BinaryState::Off));
    }

    #[test]
    fn on_duration_does_not_apply_default_before_duration() {
        let now = Instant::now();
        let mut state = runtime(config(Duration::from_secs(5), Duration::ZERO), now, 11.0);

        assert_eq!(
            state.tick(now, Duration::from_secs(1), Duration::from_secs(5)),
            TickDecision::None
        );
        assert_eq!(state.desired_state, None);
    }

    #[test]
    fn zero_duration_immediately_sets_desired_state() {
        let now = Instant::now();
        let mut state = runtime(config(Duration::ZERO, Duration::ZERO), now, 11.0);

        assert_eq!(
            state.tick(now, Duration::from_secs(1), Duration::from_secs(5)),
            TickDecision::Attempt(BinaryState::On)
        );
        assert_eq!(state.desired_state, Some(BinaryState::On));
    }

    #[test]
    fn timer_resets_when_condition_becomes_false() {
        let now = Instant::now();
        let mut state = runtime(config(Duration::from_secs(5), Duration::ZERO), now, 11.0);
        assert_eq!(
            state.tick(now, Duration::from_secs(1), Duration::from_secs(5)),
            TickDecision::None
        );

        state.observe_sample(&sample(12.3), now + Duration::from_secs(1));
        state.tick(
            now + Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_secs(5),
        );
        assert_eq!(state.on_condition_started_at, None);
        state.applied_state = Some(BinaryState::Off);

        state.observe_sample(&sample(11.0), now + Duration::from_secs(2));
        state.tick(
            now + Duration::from_secs(2),
            Duration::from_secs(1),
            Duration::from_secs(5),
        );
        assert_eq!(
            state.tick(
                now + Duration::from_secs(6),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::None
        );
        assert_eq!(state.desired_state, Some(BinaryState::Off));
    }

    #[test]
    fn stale_blocks_new_decisions() {
        let now = Instant::now();
        let mut state = runtime(config(Duration::ZERO, Duration::ZERO), now, 11.0);

        assert_eq!(
            state.tick(
                now + Duration::from_secs(31),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::None
        );
        assert!(state.stale);
        assert_eq!(state.desired_state, None);
    }

    #[test]
    fn stale_keeps_pending_retry_due() {
        let now = Instant::now();
        let mut state = runtime(config(Duration::ZERO, Duration::ZERO), now, 11.0);
        assert_eq!(
            state.tick(now, Duration::from_secs(1), Duration::from_secs(5)),
            TickDecision::Attempt(BinaryState::On)
        );

        assert_eq!(
            state.tick(
                now + Duration::from_secs(31),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::Attempt(BinaryState::On)
        );
        assert!(state.stale);
    }

    #[test]
    fn failed_http_keeps_applied_unchanged() {
        let now = Instant::now();
        let mut state = RuntimeState::new(config(Duration::ZERO, Duration::ZERO), SERIAL);
        state.applied_state = Some(BinaryState::Off);

        state.record_action_result(
            BinaryState::On,
            ActionOutcome::Failure {
                status: Some(500),
                error: Some("nope".to_string()),
            },
            now,
            Duration::from_secs(1),
            Duration::from_secs(5),
        );

        assert_eq!(state.applied_state, Some(BinaryState::Off));
        assert_eq!(state.last_http_status, Some(500));
        assert_eq!(state.last_http_error.as_deref(), Some("nope"));
    }

    #[test]
    fn success_updates_applied() {
        let now = Instant::now();
        let mut state = RuntimeState::new(config(Duration::ZERO, Duration::ZERO), SERIAL);

        state.record_action_result(
            BinaryState::On,
            ActionOutcome::Success { status: 204 },
            now,
            Duration::from_secs(1),
            Duration::from_secs(5),
        );

        assert_eq!(state.applied_state, Some(BinaryState::On));
        assert_eq!(state.pending_target_state, None);
        assert_eq!(state.last_success_at, Some(now));
    }

    #[test]
    fn opposite_desired_replaces_pending_target() {
        let now = Instant::now();
        let mut state = runtime(config(Duration::ZERO, Duration::ZERO), now, 11.0);
        state.tick(now, Duration::from_secs(1), Duration::from_secs(5));
        state.record_action_result(
            BinaryState::On,
            ActionOutcome::Failure {
                status: None,
                error: None,
            },
            now,
            Duration::from_secs(1),
            Duration::from_secs(5),
        );

        state.observe_sample(&sample(13.0), now + Duration::from_millis(1));
        assert_eq!(
            state.tick(
                now + Duration::from_millis(1),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::Attempt(BinaryState::Off)
        );
        assert_eq!(state.pending_target_state, Some(BinaryState::Off));
        assert_eq!(state.retry_attempt, 0);
        assert_eq!(state.next_retry_at, None);
    }

    #[test]
    fn backoff_caps_at_retry_max() {
        assert_eq!(
            backoff(0, Duration::from_secs(1), Duration::from_secs(5)),
            Duration::from_secs(1)
        );
        assert_eq!(
            backoff(2, Duration::from_secs(1), Duration::from_secs(5)),
            Duration::from_secs(4)
        );
        assert_eq!(
            backoff(10, Duration::from_secs(1), Duration::from_secs(5)),
            Duration::from_secs(5)
        );
    }

    fn runtime(config: StateConfig, now: Instant, value: f32) -> RuntimeState {
        let mut state = RuntimeState::new(config, SERIAL);
        state.observe_sample(&sample(value), now);
        state
    }

    fn config(on_duration: Duration, off_duration: Duration) -> StateConfig {
        StateConfig {
            name: "relay".to_string(),
            serial: SERIAL.to_string(),
            field: MeasurementField::Voltage1Volts,
            default_state: BinaryState::Off,
            stale_after: Duration::from_secs(30),
            on: transition(ComparisonOp::Le, 12.0, on_duration, "http://127.0.0.1/on"),
            off: transition(ComparisonOp::Ge, 12.6, off_duration, "http://127.0.0.1/off"),
        }
    }

    fn transition(op: ComparisonOp, value: f64, duration: Duration, url: &str) -> TransitionConfig {
        TransitionConfig {
            op,
            value,
            duration,
            url: Url::parse(url).expect("valid url"),
        }
    }

    fn sample(voltage1_volts: f32) -> Sample {
        Sample {
            schema_version: 1,
            observed_at: Utc::now(),
            serial: SERIAL,
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
