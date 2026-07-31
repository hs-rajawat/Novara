//! Per-game scan backoff.
//!
//! Detection is cheap for one game and expensive for a library. A user with 400 games
//! and no saves detected for 380 of them must not pay a full enumeration for each of
//! those 380 on every library load. So a scan that found nothing records when it is
//! worth trying again.
//!
//! ## Negative results expire; positive results do not
//!
//! ADR-0007's rule, already proven for artwork in `0007_artwork_backoff`. A game with a
//! `bind_eligible` or `suggested` candidate is *done* — rescanning cannot improve on it,
//! and the stored candidate is append-only anyway. Only "I looked and found nothing" is
//! worth revisiting, because the world changes: the game gets played, a KB refresh
//! ships, metadata arrives that enables a `{DEVELOPER}` template.
//!
//! ## New information clears the backoff
//!
//! A backoff is a statement about what was knowable at the time, not a sentence. When
//! the inputs change, `Db::clear_scan_backoff` resets every waiting game — otherwise a
//! KB update would take a week to take effect on exactly the games it was shipped to
//! fix.
//!
//! Nothing here touches the clock or the database: [`next_retry_after`] is a pure
//! function of the attempt count, so the ladder is testable without waiting.

use std::time::Duration;

/// What a scan concluded, as stored in `save_scan_attempts.outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanOutcome {
    BindEligible,
    Suggested,
    Nothing,
    Error,
}

impl ScanOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScanOutcome::BindEligible => "bind_eligible",
            ScanOutcome::Suggested => "suggested",
            ScanOutcome::Nothing => "nothing",
            ScanOutcome::Error => "error",
        }
    }

    /// Whether this outcome is worth revisiting.
    pub fn is_negative(&self) -> bool {
        matches!(self, ScanOutcome::Nothing | ScanOutcome::Error)
    }
}

/// The ladder for a scan that found nothing, in seconds.
///
/// Deliberately coarse and short at the start: the most likely reason a first scan
/// finds nothing is that the game has never been run, and that changes within hours.
/// The tail is long because a game that has produced nothing after five attempts
/// probably stores saves somewhere this design cannot yet reach — Steam Cloud
/// `userdata`, say — and hammering the disk will not discover it.
const NOTHING_LADDER_SECS: &[u64] = &[
    60 * 60,          // 1 hour
    6 * 60 * 60,      // 6 hours
    24 * 60 * 60,     // 1 day
    3 * 24 * 60 * 60, // 3 days
    7 * 24 * 60 * 60, // 1 week
];

/// The ladder for a scan that failed outright.
///
/// Shorter than the `Nothing` ladder: an error is usually transient — a directory
/// briefly locked, a drive not yet mounted — and unlike "found nothing" it carries no
/// information about the game at all.
const ERROR_LADDER_SECS: &[u64] = &[
    5 * 60,       // 5 minutes
    15 * 60,      // 15 minutes
    60 * 60,      // 1 hour
    6 * 60 * 60,  // 6 hours
    24 * 60 * 60, // 1 day
];

/// How long to wait before scanning this game again.
///
/// `None` means "never again on a schedule" — either the scan succeeded, or the ladder
/// is exhausted. An exhausted ladder is not a permanent ban: `clear_scan_backoff` still
/// releases it when new information arrives.
///
/// `attempt_count` is the count *including* the attempt just recorded, so the first
/// failure passes 1.
pub fn next_retry_after(outcome: ScanOutcome, attempt_count: i64) -> Option<Duration> {
    if !outcome.is_negative() {
        return None;
    }
    let ladder = match outcome {
        ScanOutcome::Error => ERROR_LADDER_SECS,
        _ => NOTHING_LADDER_SECS,
    };
    // Attempt 1 takes the first rung.
    let index = usize::try_from(attempt_count.max(1) - 1).unwrap_or(usize::MAX);
    ladder.get(index).copied().map(Duration::from_secs)
}

/// Whether a game recorded as retrying at `next_retry_at` is due.
///
/// An absent or unparseable timestamp means **due**. Failing open matters here: a
/// corrupt row must not silently exclude a game from detection forever, and the cost
/// of an unnecessary scan is milliseconds.
pub fn is_due(next_retry_at: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> bool {
    match next_retry_at {
        None => true,
        Some(raw) => match crate::models::parse_rfc3339(raw) {
            Some(due) => now >= due,
            None => true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, Utc};

    #[test]
    fn a_successful_scan_never_schedules_a_retry() {
        for outcome in [ScanOutcome::BindEligible, ScanOutcome::Suggested] {
            for attempt in 1..8 {
                assert_eq!(
                    next_retry_after(outcome, attempt),
                    None,
                    "{outcome:?} attempt {attempt} should not schedule a retry"
                );
            }
        }
    }

    #[test]
    fn a_fruitless_scan_backs_off_monotonically() {
        let mut previous = Duration::ZERO;
        for attempt in 1..=NOTHING_LADDER_SECS.len() as i64 {
            let wait = next_retry_after(ScanOutcome::Nothing, attempt)
                .unwrap_or_else(|| panic!("attempt {attempt} should have a rung"));
            assert!(
                wait > previous,
                "attempt {attempt}: {wait:?} is not longer than {previous:?}"
            );
            previous = wait;
        }
    }

    #[test]
    fn the_first_fruitless_scan_retries_within_the_hour() {
        assert_eq!(
            next_retry_after(ScanOutcome::Nothing, 1),
            Some(Duration::from_secs(3_600))
        );
    }

    /// An error is more likely transient than "found nothing", so it retries sooner.
    #[test]
    fn errors_retry_sooner_than_empty_results() {
        for attempt in 1..=5 {
            let err = next_retry_after(ScanOutcome::Error, attempt).unwrap();
            let nothing = next_retry_after(ScanOutcome::Nothing, attempt).unwrap();
            assert!(
                err < nothing,
                "attempt {attempt}: error {err:?} should be shorter than {nothing:?}"
            );
        }
    }

    #[test]
    fn an_exhausted_ladder_stops_scheduling() {
        assert_eq!(next_retry_after(ScanOutcome::Nothing, 99), None);
        assert_eq!(next_retry_after(ScanOutcome::Error, 99), None);
    }

    /// A zero or negative attempt count must not panic or index out of bounds.
    #[test]
    fn a_nonsensical_attempt_count_is_handled() {
        assert_eq!(
            next_retry_after(ScanOutcome::Nothing, 0),
            Some(Duration::from_secs(3_600))
        );
        assert_eq!(
            next_retry_after(ScanOutcome::Nothing, -5),
            Some(Duration::from_secs(3_600))
        );
    }

    #[test]
    fn a_game_with_no_recorded_retry_is_due() {
        assert!(is_due(None, Utc::now()));
    }

    #[test]
    fn a_future_retry_time_is_not_due() {
        let later = (Utc::now() + ChronoDuration::hours(2)).to_rfc3339();
        assert!(!is_due(Some(&later), Utc::now()));
    }

    #[test]
    fn a_past_retry_time_is_due() {
        let earlier = (Utc::now() - ChronoDuration::hours(2)).to_rfc3339();
        assert!(is_due(Some(&earlier), Utc::now()));
    }

    /// Failing open: a corrupt timestamp must not exclude a game from detection
    /// forever. The cost of an unnecessary scan is milliseconds; the cost of a game
    /// that can never be scanned again is a feature that looks broken.
    #[test]
    fn an_unparseable_retry_time_is_due() {
        assert!(is_due(Some("tomorrow-ish"), Utc::now()));
        assert!(is_due(Some(""), Utc::now()));
    }
}
