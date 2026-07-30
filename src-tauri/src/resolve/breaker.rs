//! Per-sweep circuit breaker for metadata providers.
//!
//! Both fill services — text and artwork — independently contained the same
//! decision, down to the same threshold:
//!
//! ```text
//! let broken = matches!(reason, TemporaryReason::RateLimited) || {
//!     let count = temporary_misses.entry(code).or_insert(0);
//!     *count += 1;
//!     *count >= 5
//! };
//! ```
//!
//! Two copies of a policy is two places for it to drift: raising the threshold in
//! one service, or deciding a new `TemporaryReason` should trip immediately, would
//! silently apply to only half the pipeline. The behaviour is unchanged by this
//! extraction — it is the same rule with one definition.
//!
//! State is deliberately per-sweep, not stored on the service or in the database.
//! A provider that was rate-limited during one fill gets a clean slate on the
//! next: the breaker exists to stop hammering a provider that is failing *now*,
//! not to remember a grudge. Longer-lived backoff is the artwork ledger's job
//! (`next_retry_at`), which is per slot and survives restarts.

use std::collections::{HashMap, HashSet};

use super::TemporaryReason;

/// Consecutive transient misses a provider is allowed before it is dropped for
/// the rest of the sweep.
///
/// A single timeout is usually one bad request rather than a broken provider, so
/// tripping on the first would give up on a working provider for a whole library.
/// A rate limit is different and is not counted — see [`CircuitBreaker::record`].
const MISSES_BEFORE_BREAKING: u32 = 5;

/// Tracks which providers have been abandoned for the current sweep.
#[derive(Debug, Default)]
pub struct CircuitBreaker {
    broken: HashSet<&'static str>,
    misses: HashMap<&'static str, u32>,
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this provider has been given up on for the rest of the sweep.
    pub fn is_broken(&self, provider: &str) -> bool {
        self.broken.contains(provider)
    }

    /// Record a transient failure, returning whether the provider is now broken.
    ///
    /// A rate limit trips immediately: it is an unambiguous "stop asking"
    /// instruction from the provider, and continuing to ask is both futile and
    /// rude. Every other transient reason has to happen repeatedly first.
    pub fn record(&mut self, provider: &'static str, reason: &TemporaryReason) -> bool {
        let broken = if matches!(reason, TemporaryReason::RateLimited) {
            true
        } else {
            let count = self.misses.entry(provider).or_insert(0);
            *count += 1;
            *count >= MISSES_BEFORE_BREAKING
        };
        if broken {
            self.broken.insert(provider);
        }
        broken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_limit_breaks_a_provider_immediately() {
        let mut breaker = CircuitBreaker::new();
        assert!(breaker.record("steam_cdn", &TemporaryReason::RateLimited));
        assert!(breaker.is_broken("steam_cdn"));
    }

    #[test]
    fn a_single_timeout_does_not_break_a_provider() {
        let mut breaker = CircuitBreaker::new();
        assert!(!breaker.record("steam_cdn", &TemporaryReason::Timeout));
        assert!(
            !breaker.is_broken("steam_cdn"),
            "one bad request is not a broken provider"
        );
    }

    #[test]
    fn repeated_transient_misses_break_a_provider_at_the_threshold() {
        let mut breaker = CircuitBreaker::new();
        for attempt in 1..MISSES_BEFORE_BREAKING {
            assert!(
                !breaker.record("steam_cdn", &TemporaryReason::Timeout),
                "miss {attempt} is below the threshold"
            );
        }
        assert!(breaker.record("steam_cdn", &TemporaryReason::Timeout));
        assert!(breaker.is_broken("steam_cdn"));
    }

    #[test]
    fn misses_are_counted_per_provider() {
        let mut breaker = CircuitBreaker::new();
        for _ in 0..MISSES_BEFORE_BREAKING {
            breaker.record("steam_cdn", &TemporaryReason::Timeout);
        }
        assert!(breaker.is_broken("steam_cdn"));
        assert!(
            !breaker.is_broken("epic_catalog"),
            "one failing provider must not disable the others"
        );
    }

    #[test]
    fn an_untouched_provider_is_never_broken() {
        let breaker = CircuitBreaker::new();
        assert!(!breaker.is_broken("steam_local"));
    }
}
