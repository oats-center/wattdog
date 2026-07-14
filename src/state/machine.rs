//! Runtime state machine for one configured binary output.

use std::time::{Duration, Instant};

use crate::{
    config::{ActionStepConfig, BinaryState, StateConfig, WaitTimeoutPolicy},
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
    /// Current step within the pending target sequence.
    pub action_step_index: usize,
    /// Time when the current sequence step began.
    pub step_started_at: Option<Instant>,
    /// Next time a retry may be attempted.
    pub next_retry_at: Option<Instant>,
    /// Earliest time when the opposite transition may begin qualification.
    pub hold_until: Option<Instant>,
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
    Attempt {
        /// Sequence target.
        target: BinaryState,
        /// Zero-based sequence step.
        step_index: usize,
    },
    /// A JSON wait reached its configured deadline.
    WaitTimedOut {
        /// Sequence target.
        target: BinaryState,
        /// Zero-based sequence step.
        step_index: usize,
    },
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
    /// A polling request succeeded but its JSON condition was not yet satisfied.
    Pending { status: u16 },
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
            action_step_index: 0,
            step_started_at: None,
            next_retry_at: None,
            hold_until: None,
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

        if let Some(sample_at) = self.latest_sample_at {
            self.stale = now.saturating_duration_since(sample_at) > self.config.stale_after;
        }

        let (Some(value), Some(sample_at)) = (self.latest_value, self.latest_sample_at) else {
            return self.due_attempt(now);
        };

        if self.pending_target_state.is_some() {
            return self.due_attempt(now);
        }

        self.stale = now.saturating_duration_since(sample_at) > self.config.stale_after;
        if self.stale {
            return TickDecision::None;
        }

        if self.hold_until.is_some_and(|until| now < until) {
            self.on_condition_started_at = None;
            self.off_condition_started_at = None;
            return TickDecision::None;
        }
        self.hold_until = None;

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
        step_index: usize,
        outcome: ActionOutcome,
        now: Instant,
        retry_initial: Duration,
        retry_max: Duration,
    ) {
        if self.pending_target_state != Some(target) || self.action_step_index != step_index {
            return;
        }

        let is_poll = matches!(
            self.current_action(),
            Some(ActionStepConfig::WaitForJson { .. })
        );
        match outcome {
            ActionOutcome::Success { status } => {
                self.retry_attempt = 0;
                self.next_retry_at = None;
                self.last_http_status = Some(status);
                self.last_http_error = None;
                self.last_success_at = Some(now);
                self.advance_step(target, now);
            }
            ActionOutcome::Failure { status, error } => {
                self.last_http_status = status;
                self.last_http_error = error;
                if is_poll {
                    self.next_retry_at = Some(now + self.poll_every());
                } else {
                    self.next_retry_at =
                        Some(now + backoff(self.retry_attempt, retry_initial, retry_max));
                    self.retry_attempt = self.retry_attempt.saturating_add(1);
                }
            }
            ActionOutcome::Pending { status } => {
                self.last_http_status = Some(status);
                self.last_http_error = None;
                self.next_retry_at = Some(now + self.poll_every());
            }
        }
    }

    /// Applies the configured timeout policy for the current JSON wait step.
    pub fn record_wait_timeout(
        &mut self,
        target: BinaryState,
        step_index: usize,
        now: Instant,
        retry_initial: Duration,
        retry_max: Duration,
    ) {
        if self.pending_target_state != Some(target) || self.action_step_index != step_index {
            return;
        }
        let Some(ActionStepConfig::WaitForJson { on_timeout, .. }) = self.current_action() else {
            return;
        };
        match on_timeout {
            WaitTimeoutPolicy::Continue => self.advance_step(target, now),
            WaitTimeoutPolicy::Retry => {
                let delay = backoff(self.retry_attempt, retry_initial, retry_max);
                self.retry_attempt = self.retry_attempt.saturating_add(1);
                self.step_started_at = Some(now + delay);
                self.next_retry_at = Some(now + delay);
            }
        }
    }

    /// Returns whether a matching JSON wait step has reached its deadline.
    #[must_use]
    pub fn wait_timed_out(&self, target: BinaryState, step_index: usize, now: Instant) -> bool {
        if self.pending_target_state != Some(target) || self.action_step_index != step_index {
            return false;
        }
        matches!(
            self.current_action(),
            Some(ActionStepConfig::WaitForJson { timeout, .. })
                if now.saturating_duration_since(self.step_started_at.unwrap_or(now)) >= timeout
        )
    }

    fn due_attempt(&mut self, now: Instant) -> TickDecision {
        if let Some(target) = self.pending_target_state {
            return self.due_sequence_step(target, now);
        }

        let Some(desired) = self.desired_state else {
            return TickDecision::None;
        };
        if Some(desired) == self.applied_state {
            self.pending_target_state = None;
            return TickDecision::None;
        }
        self.pending_target_state = Some(desired);
        self.action_step_index = 0;
        self.step_started_at = Some(now);
        self.retry_attempt = 0;
        self.next_retry_at = None;
        self.due_sequence_step(desired, now)
    }

    fn due_sequence_step(&mut self, target: BinaryState, now: Instant) -> TickDecision {
        let Some(action) = self.current_action() else {
            return TickDecision::None;
        };
        match action {
            ActionStepConfig::Delay { duration } => {
                let started = self.step_started_at.get_or_insert(now);
                if now.saturating_duration_since(*started) >= duration {
                    self.advance_step(target, now);
                    self.due_sequence_step(target, now)
                } else {
                    TickDecision::None
                }
            }
            ActionStepConfig::WaitForJson { timeout, .. }
                if now.saturating_duration_since(self.step_started_at.unwrap_or(now))
                    >= timeout =>
            {
                TickDecision::WaitTimedOut {
                    target,
                    step_index: self.action_step_index,
                }
            }
            ActionStepConfig::Request { .. } | ActionStepConfig::WaitForJson { .. } => {
                if self.next_retry_at.is_none_or(|retry_at| now >= retry_at) {
                    TickDecision::Attempt {
                        target,
                        step_index: self.action_step_index,
                    }
                } else {
                    TickDecision::None
                }
            }
        }
    }

    fn advance_step(&mut self, target: BinaryState, now: Instant) {
        self.action_step_index += 1;
        self.retry_attempt = 0;
        self.next_retry_at = None;
        self.step_started_at = Some(now);

        if self.action_step_index >= self.transition(target).action_count() {
            let hold_for = self.transition(target).hold_for;
            self.applied_state = Some(target);
            self.desired_state = Some(target);
            self.pending_target_state = None;
            self.action_step_index = 0;
            self.step_started_at = None;
            self.hold_until = Some(now + hold_for);
            self.on_condition_started_at = None;
            self.off_condition_started_at = None;
        }
    }

    /// Returns the configured transition for a target state.
    #[must_use]
    pub fn transition(&self, target: BinaryState) -> &crate::config::TransitionConfig {
        match target {
            BinaryState::On => &self.config.on,
            BinaryState::Off => &self.config.off,
        }
    }

    fn current_action(&self) -> Option<ActionStepConfig> {
        self.pending_target_state
            .and_then(|target| self.transition(target).action(self.action_step_index))
    }

    fn poll_every(&self) -> Duration {
        match self.current_action() {
            Some(ActionStepConfig::WaitForJson { poll_every, .. }) => poll_every,
            _ => Duration::ZERO,
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
        config::{
            ActionStepConfig, BinaryState, ComparisonOp, MeasurementField, StateConfig,
            TransitionConfig, WaitTimeoutPolicy,
        },
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
            attempt(BinaryState::Off)
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
            attempt(BinaryState::On)
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
        state.pending_target_state = None;

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
            attempt(BinaryState::On)
        );

        assert_eq!(
            state.tick(
                now + Duration::from_secs(31),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            attempt(BinaryState::On)
        );
        assert!(state.stale);
    }

    #[test]
    fn failed_http_keeps_applied_unchanged() {
        let now = Instant::now();
        let mut state = RuntimeState::new(config(Duration::ZERO, Duration::ZERO), SERIAL);
        state.applied_state = Some(BinaryState::Off);
        state.desired_state = Some(BinaryState::On);
        assert_eq!(
            state.tick(now, Duration::from_secs(1), Duration::from_secs(5)),
            attempt(BinaryState::On)
        );

        state.record_action_result(
            BinaryState::On,
            0,
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
        state.desired_state = Some(BinaryState::On);
        assert_eq!(
            state.tick(now, Duration::from_secs(1), Duration::from_secs(5)),
            attempt(BinaryState::On)
        );

        state.record_action_result(
            BinaryState::On,
            0,
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
    fn opposite_condition_does_not_replace_atomic_pending_target() {
        let now = Instant::now();
        let mut state = runtime(config(Duration::ZERO, Duration::ZERO), now, 11.0);
        state.tick(now, Duration::from_secs(1), Duration::from_secs(5));
        state.record_action_result(
            BinaryState::On,
            0,
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
            TickDecision::None
        );
        assert_eq!(state.pending_target_state, Some(BinaryState::On));
        assert_eq!(state.retry_attempt, 1);
        assert!(state.next_retry_at.is_some());
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one chronological sequence keeps the timing assertions readable"
    )]
    fn sequence_advances_in_order_and_dwell_precedes_opposite_duration() {
        let now = Instant::now();
        let mut config = config(Duration::ZERO, Duration::from_secs(5));
        config.on.url = None;
        config.on.hold_for = Duration::from_secs(10);
        config.on.actions = vec![
            request("http://127.0.0.1/soft-off"),
            ActionStepConfig::WaitForJson {
                url: Url::parse("http://127.0.0.1/atx").expect("url"),
                pointer: "/result/leds/power".to_string(),
                expected: serde_json::Value::Bool(false),
                poll_every: Duration::from_secs(2),
                timeout: Duration::from_secs(20),
                on_timeout: WaitTimeoutPolicy::Continue,
            },
            ActionStepConfig::Delay {
                duration: Duration::from_secs(1),
            },
            request("http://127.0.0.1/relay-off"),
        ];
        let mut state = runtime(config, now, 11.0);

        assert_eq!(
            state.tick(now, Duration::from_secs(1), Duration::from_secs(5)),
            attempt(BinaryState::On)
        );
        state.record_action_result(
            BinaryState::On,
            0,
            ActionOutcome::Success { status: 204 },
            now,
            Duration::from_secs(1),
            Duration::from_secs(5),
        );
        assert_eq!(state.action_step_index, 1);

        assert_eq!(
            state.tick(now, Duration::from_secs(1), Duration::from_secs(5)),
            TickDecision::Attempt {
                target: BinaryState::On,
                step_index: 1,
            }
        );
        state.record_action_result(
            BinaryState::On,
            1,
            ActionOutcome::Pending { status: 200 },
            now,
            Duration::from_secs(1),
            Duration::from_secs(5),
        );
        assert_eq!(
            state.tick(
                now + Duration::from_secs(1),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::None
        );
        assert_eq!(
            state.tick(
                now + Duration::from_secs(2),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::Attempt {
                target: BinaryState::On,
                step_index: 1,
            }
        );
        state.record_action_result(
            BinaryState::On,
            1,
            ActionOutcome::Success { status: 200 },
            now + Duration::from_secs(2),
            Duration::from_secs(1),
            Duration::from_secs(5),
        );
        assert_eq!(
            state.tick(
                now + Duration::from_secs(2),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::None
        );
        assert_eq!(
            state.tick(
                now + Duration::from_secs(3),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::Attempt {
                target: BinaryState::On,
                step_index: 3,
            }
        );
        state.record_action_result(
            BinaryState::On,
            3,
            ActionOutcome::Success { status: 204 },
            now + Duration::from_secs(3),
            Duration::from_secs(1),
            Duration::from_secs(5),
        );
        assert_eq!(state.applied_state, Some(BinaryState::On));

        state.observe_sample(&sample(13.0), now + Duration::from_secs(4));
        assert_eq!(
            state.tick(
                now + Duration::from_secs(13),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::None
        );
        assert_eq!(
            state.tick(
                now + Duration::from_secs(18),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            attempt(BinaryState::Off)
        );
    }

    #[test]
    fn wait_timeout_continue_advances_to_next_step() {
        let now = Instant::now();
        let mut config = config(Duration::ZERO, Duration::ZERO);
        config.on.url = None;
        config.on.actions = vec![
            ActionStepConfig::WaitForJson {
                url: Url::parse("http://127.0.0.1/atx").expect("url"),
                pointer: "/result/leds/power".to_string(),
                expected: serde_json::Value::Bool(false),
                poll_every: Duration::from_secs(1),
                timeout: Duration::from_secs(2),
                on_timeout: WaitTimeoutPolicy::Continue,
            },
            request("http://127.0.0.1/relay-off"),
        ];
        let mut state = runtime(config, now, 11.0);
        assert!(matches!(
            state.tick(now, Duration::from_secs(1), Duration::from_secs(5)),
            TickDecision::Attempt { step_index: 0, .. }
        ));
        assert_eq!(
            state.tick(
                now + Duration::from_secs(2),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::WaitTimedOut {
                target: BinaryState::On,
                step_index: 0,
            }
        );
        state.record_wait_timeout(
            BinaryState::On,
            0,
            now + Duration::from_secs(2),
            Duration::from_secs(1),
            Duration::from_secs(5),
        );
        assert_eq!(
            state.tick(
                now + Duration::from_secs(2),
                Duration::from_secs(1),
                Duration::from_secs(5)
            ),
            TickDecision::Attempt {
                target: BinaryState::On,
                step_index: 1
            }
        );
    }

    #[test]
    fn wait_timeout_retry_polls_after_backoff_longer_than_timeout() {
        let now = Instant::now();
        let mut config = config(Duration::ZERO, Duration::ZERO);
        config.on.url = None;
        config.on.actions = vec![ActionStepConfig::WaitForJson {
            url: Url::parse("http://127.0.0.1/atx").expect("url"),
            pointer: "/result/leds/power".to_string(),
            expected: serde_json::Value::Bool(false),
            poll_every: Duration::from_secs(1),
            timeout: Duration::from_secs(1),
            on_timeout: WaitTimeoutPolicy::Retry,
        }];
        let mut state = runtime(config, now, 11.0);
        assert!(matches!(
            state.tick(now, Duration::from_secs(5), Duration::from_secs(5)),
            TickDecision::Attempt { .. }
        ));
        assert!(matches!(
            state.tick(
                now + Duration::from_secs(1),
                Duration::from_secs(5),
                Duration::from_secs(5)
            ),
            TickDecision::WaitTimedOut { .. }
        ));
        state.record_wait_timeout(
            BinaryState::On,
            0,
            now + Duration::from_secs(1),
            Duration::from_secs(5),
            Duration::from_secs(5),
        );
        assert_eq!(
            state.tick(
                now + Duration::from_secs(5),
                Duration::from_secs(5),
                Duration::from_secs(5)
            ),
            TickDecision::None
        );
        assert!(matches!(
            state.tick(
                now + Duration::from_secs(6),
                Duration::from_secs(5),
                Duration::from_secs(5)
            ),
            TickDecision::Attempt { step_index: 0, .. }
        ));
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
            hold_for: Duration::ZERO,
            url: Some(Url::parse(url).expect("valid url")),
            actions: Vec::new(),
        }
    }

    fn request(url: &str) -> ActionStepConfig {
        ActionStepConfig::Request {
            method: None,
            url: Url::parse(url).expect("valid url"),
        }
    }

    fn attempt(target: BinaryState) -> TickDecision {
        TickDecision::Attempt {
            target,
            step_index: 0,
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
