//! Outbound request throttling.
//!
//! Providers previously issued network requests as fast as the fill loop could
//! drive them. A first scan of a 500-game library meant roughly 500
//! `appdetails` GETs plus 1,500 CDN HEADs back to back, against endpoints Valve
//! does not document a rate limit for but is well known to answer with HTTP 429
//! under sustained load. That failure mode only appears in the field, on
//! libraries larger than any developer's test data, and its consequence is an
//! IP-level throttle rather than a clean error.
//!
//! Two independent limits, because they solve different problems:
//!   * a semaphore caps how many requests may be **in flight** at once, which
//!     is what matters once the fill loop is parallelised (Batch 6);
//!   * a minimum interval caps the **rate**, which is what matters today with
//!     a sequential loop — a concurrency cap of 1 still permits an unbounded
//!     request rate.
//!
//! Deliberately not a full token bucket: bursts are exactly what we are trying
//! to avoid, so there is no allowance to accumulate.

use std::time::Duration;

use tokio::sync::{Mutex, Semaphore, SemaphorePermit};
use tokio::time::Instant;

/// Default ceiling on concurrent outbound requests.
pub const DEFAULT_MAX_CONCURRENT: usize = 4;

/// Default minimum spacing between outbound requests.
///
/// 250ms caps sustained traffic at four requests per second. For a 500-game
/// Steam library that stretches a first fill to roughly eight minutes, which is
/// acceptable precisely because the fill is a background task whose progress
/// arrives by event — and is far preferable to being throttled at the IP level
/// partway through.
pub const DEFAULT_MIN_INTERVAL: Duration = Duration::from_millis(250);

pub struct Throttle {
    semaphore: Semaphore,
    min_interval: Duration,
    /// When the next request may start. Held across the wait so concurrent
    /// callers queue rather than all sleeping until the same instant and then
    /// firing together.
    next_allowed: Mutex<Option<Instant>>,
}

/// Held for the duration of a request; releases the concurrency slot on drop.
pub struct ThrottleGuard<'a> {
    _permit: SemaphorePermit<'a>,
}

impl Throttle {
    pub fn new(max_concurrent: usize, min_interval: Duration) -> Self {
        Self {
            semaphore: Semaphore::new(max_concurrent.max(1)),
            min_interval,
            next_allowed: Mutex::new(None),
        }
    }

    /// Wait until a request may be issued, then hold a slot until the returned
    /// guard is dropped.
    pub async fn acquire(&self) -> ThrottleGuard<'_> {
        let permit = self
            .semaphore
            .acquire()
            .await
            .expect("throttle semaphore is never closed");

        // Reserve this caller's slot in the schedule *before* sleeping, so two
        // callers arriving together are spaced apart from each other rather
        // than both waking at the same deadline.
        let wait_until = {
            let mut next = self.next_allowed.lock().await;
            let now = Instant::now();
            let start = match *next {
                Some(t) if t > now => t,
                _ => now,
            };
            *next = Some(start + self.min_interval);
            start
        };

        let now = Instant::now();
        if wait_until > now {
            tokio::time::sleep(wait_until - now).await;
        }

        ThrottleGuard { _permit: permit }
    }
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CONCURRENT, DEFAULT_MIN_INTERVAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spaces_sequential_requests_by_the_minimum_interval() {
        let throttle = Throttle::new(4, Duration::from_millis(50));
        let start = Instant::now();
        for _ in 0..4 {
            let _g = throttle.acquire().await;
        }
        // Four requests at 50ms spacing: the first may start immediately, so at
        // least three intervals must have elapsed.
        assert!(
            start.elapsed() >= Duration::from_millis(150),
            "elapsed {:?} is too fast to be throttled",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn concurrent_callers_are_spaced_from_each_other_not_bunched() {
        let throttle = Arc::new(Throttle::new(8, Duration::from_millis(40)));
        let start = Instant::now();
        let mut handles = Vec::new();
        for _ in 0..5 {
            let t = throttle.clone();
            handles.push(tokio::spawn(async move {
                let _g = t.acquire().await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert!(
            start.elapsed() >= Duration::from_millis(160),
            "five concurrent callers fired as a burst: {:?}",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn caps_requests_in_flight() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let throttle = Arc::new(Throttle::new(2, Duration::from_millis(1)));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..10 {
            let t = throttle.clone();
            let cur = in_flight.clone();
            let peak = peak.clone();
            handles.push(tokio::spawn(async move {
                let _g = t.acquire().await;
                let now = cur.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                cur.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert!(
            peak.load(Ordering::SeqCst) <= 2,
            "peak in flight was {}",
            peak.load(Ordering::SeqCst)
        );
    }

    use std::sync::Arc;
}
