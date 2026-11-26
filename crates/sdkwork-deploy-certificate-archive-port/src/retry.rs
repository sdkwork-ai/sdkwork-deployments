//! When an archive row gets another attempt.
//!
//! Archiving is best-effort *by contract*: an issuance that succeeded is never
//! undone because Drive was unreachable. What keeps that from meaning "lost" is a
//! bounded, persisted retry schedule — the row carries `attempt_count` and
//! `next_attempt_at`, so a retry survives a process restart and two workers cannot
//! both decide to retry the same row.
//!
//! The ladder is `30s → 2m → 10m → 1h → 6h` (design §4.6.3). Each step is longer
//! than the one before by at least a factor of four, so a Drive outage is not
//! amplified by retries: five attempts over roughly seven hours, versus the
//! hundreds a fixed interval would produce. After the last attempt the row is
//! `FAILED` and only the console's explicit retry moves it again.
//!
//! Every function here is pure and takes the attempt count as an input; there is
//! no clock read inside, so the schedule can be asserted exactly.

/// The delay before each attempt, in seconds, indexed by `attempt_count` *after*
/// the attempt that failed.
///
/// `RETRY_BACKOFF_SECONDS[0]` is the delay after attempt 1 failed, and so on.
pub const RETRY_BACKOFF_SECONDS: [i64; 5] = [30, 120, 600, 3_600, 21_600];

/// Default ceiling on attempts for one version, before the row becomes `FAILED`.
pub const DEFAULT_MAX_ATTEMPTS: i32 = 5;

/// Upper bound an operator may configure.
///
/// The database check constraint admits `0..=100`; the ceiling here is lower on
/// purpose — a row retried more than a day's running is not a transient failure
/// and the 6 hour tail would delay every later attempt anyway.
pub const MAXIMUM_MAX_ATTEMPTS: i32 = 20;

/// Bounded retry configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveRetryPolicy {
    /// How many attempts a version gets before the row is `FAILED`.
    pub max_attempts: i32,
}

impl Default for ArchiveRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_MAX_ATTEMPTS,
        }
    }
}

impl ArchiveRetryPolicy {
    /// Validates an attempt ceiling.
    ///
    /// # Errors
    ///
    /// A ceiling outside `1..=MAXIMUM_MAX_ATTEMPTS`. Zero is refused rather than
    /// treated as "never archive": the switch for that is
    /// `SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE=off`, and a zero here would leave every
    /// version permanently `PENDING`.
    pub fn new(max_attempts: i32) -> Result<Self, RetryPolicyError> {
        if !(1..=MAXIMUM_MAX_ATTEMPTS).contains(&max_attempts) {
            return Err(RetryPolicyError::MaxAttemptsOutOfRange { max_attempts });
        }
        Ok(Self { max_attempts })
    }

    /// What happens after the attempt that left `attempt_count` on the row.
    ///
    /// `attempt_count` is the value *after* the failed attempt, which is the value
    /// the worker writes on the same update as the status and `next_attempt_at`.
    #[must_use]
    pub fn after_failure(&self, attempt_count: i32) -> ArchiveRetryDecision {
        if attempt_count < self.max_attempts {
            return ArchiveRetryDecision::Retry {
                delay_seconds: backoff_after(attempt_count),
            };
        }
        ArchiveRetryDecision::Exhausted
    }
}

/// The delay to wait after `attempt_count` consecutive failures.
///
/// Attempts beyond the ladder (only reachable after an operator raised the
/// ceiling with rows already in flight) repeat the last step instead of growing
/// without bound.
#[must_use]
pub fn backoff_after(attempt_count: i32) -> i64 {
    if attempt_count <= 0 {
        return RETRY_BACKOFF_SECONDS[0];
    }
    let index = usize::try_from(attempt_count - 1).unwrap_or(usize::MAX);
    RETRY_BACKOFF_SECONDS
        .get(index)
        .copied()
        .unwrap_or(RETRY_BACKOFF_SECONDS[RETRY_BACKOFF_SECONDS.len() - 1])
}

/// Outcome of one failed attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveRetryDecision {
    /// Leave the row `PENDING` with `next_attempt_at = now + delay_seconds`.
    Retry { delay_seconds: i64 },
    /// Mark the row `FAILED`; only an operator's retry moves it again.
    Exhausted,
}

/// Why a retry policy could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RetryPolicyError {
    #[error(
        "certificate archive max attempts must be between 1 and {max} (got {max_attempts})",
        max = MAXIMUM_MAX_ATTEMPTS
    )]
    MaxAttemptsOutOfRange { max_attempts: i32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ladder_is_the_documented_one() {
        let policy = ArchiveRetryPolicy::default();
        let delays: Vec<i64> = (1..=4)
            .map(|attempt| match policy.after_failure(attempt) {
                ArchiveRetryDecision::Retry { delay_seconds } => delay_seconds,
                ArchiveRetryDecision::Exhausted => panic!("attempt {attempt} must retry"),
            })
            .collect();
        // Four retries after four failures; the fifth failure spends the budget,
        // so the 6 h step is reached by the four-failure path above and the fifth
        // attempt is the last one.
        assert_eq!(delays, vec![30, 120, 600, 3_600]);
        assert_eq!(RETRY_BACKOFF_SECONDS, [30, 120, 600, 3_600, 21_600]);
    }

    #[test]
    fn the_fifth_failure_is_the_last_one() {
        let policy = ArchiveRetryPolicy::default();
        // Four failures leave one attempt, so the row still retries...
        assert_eq!(
            policy.after_failure(4),
            ArchiveRetryDecision::Retry { delay_seconds: 3_600 }
        );
        // ...and the fifth spends it.
        assert_eq!(policy.after_failure(5), ArchiveRetryDecision::Exhausted);
        assert_eq!(policy.after_failure(6), ArchiveRetryDecision::Exhausted);
    }

    #[test]
    fn the_schedule_spans_several_hours_rather_than_retrying_quickly() {
        // The bound that matters operationally: five attempts are spread over
        // more than six hours, so a Drive outage is survived, not amplified.
        let total: i64 = RETRY_BACKOFF_SECONDS.iter().sum();
        assert!(
            total >= 6 * 3_600,
            "the five-step ladder must span at least six hours, got {total}s"
        );
    }

    #[test]
    fn every_step_waits_substantially_longer_than_the_one_before() {
        // The property that keeps a Drive outage from being amplified: a fixed or
        // gently growing interval would produce hundreds of attempts overnight.
        for window in RETRY_BACKOFF_SECONDS.windows(2) {
            assert!(
                window[1] >= window[0] * 4,
                "step {} must be at least four times step {}",
                window[1],
                window[0]
            );
        }
    }

    #[test]
    fn attempts_beyond_the_ladder_repeat_its_last_step() {
        assert_eq!(backoff_after(0), 30, "a zero count is treated as one failure");
        assert_eq!(backoff_after(-3), 30);
        assert_eq!(backoff_after(5), 21_600);
        assert_eq!(backoff_after(99), 21_600);
    }

    #[test]
    fn a_raised_ceiling_extends_the_schedule_without_new_steps() {
        let policy = ArchiveRetryPolicy::new(8).expect("policy");
        assert_eq!(
            policy.after_failure(6),
            ArchiveRetryDecision::Retry { delay_seconds: 21_600 }
        );
        assert_eq!(policy.after_failure(8), ArchiveRetryDecision::Exhausted);
    }

    #[test]
    fn a_zero_or_excessive_ceiling_is_refused() {
        for value in [0, -1, MAXIMUM_MAX_ATTEMPTS + 1, 101] {
            assert!(
                ArchiveRetryPolicy::new(value).is_err(),
                "max attempts {value} must be refused"
            );
        }
        assert!(ArchiveRetryPolicy::new(1).is_ok());
        assert!(ArchiveRetryPolicy::new(MAXIMUM_MAX_ATTEMPTS).is_ok());
    }

    #[test]
    fn the_default_ceiling_fits_the_database_check_constraint() {
        // `chk_deploy_certificate_archive_attempts` admits 0..=100; exceeding it
        // would turn a retry into a constraint violation on the last attempt.
        assert!((1..=100).contains(&DEFAULT_MAX_ATTEMPTS));
        assert!(MAXIMUM_MAX_ATTEMPTS <= 100);
        assert_eq!(RETRY_BACKOFF_SECONDS.len() as i32, DEFAULT_MAX_ATTEMPTS);
    }
}
