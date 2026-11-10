use std::collections::HashMap;
use std::sync::Mutex;

use conta_verde_domain::ApiKeyId;

/// How many buckets are kept before the ones that are full again are dropped.
const SWEEP_ABOVE: usize = 1024;

/// A token bucket per key, **in the memory of this process** ([ADR 0087]): with several instances behind a balancer a key's
/// real limit is that many times the one set, and a restart forgets the buckets. The time is the caller's, in Unix
/// milliseconds, so a test moves it.
pub(crate) struct RateLimiter {
    per_minute: u32,
    buckets: Mutex<HashMap<ApiKeyId, Bucket>>,
}

struct Bucket {
    tokens: f64,
    at: i64,
}

/// What a request that was let through leaves of its key's allowance, for the `RateLimit-*` headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Allowance {
    pub limit: u32,
    pub remaining: u32,
    /// Seconds until the bucket is full again.
    pub reset_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Allowed(Allowance),
    Refused {
        limit: u32,
        /// Seconds until one more request would be let through.
        retry_after_seconds: u64,
    },
}

impl RateLimiter {
    pub(crate) fn new(per_minute: u32) -> Self {
        Self {
            per_minute: per_minute.max(1),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn check(&self, key: ApiKeyId, now: i64) -> Verdict {
        let capacity = f64::from(self.per_minute);
        let per_millisecond = capacity / 60_000.0;
        let refilled = |bucket: &Bucket| {
            let elapsed = (now - bucket.at).max(0) as f64;
            (bucket.tokens + elapsed * per_millisecond).min(capacity)
        };
        let mut buckets = self.buckets.lock().expect("rate limiter lock poisoned");
        if buckets.len() > SWEEP_ABOVE {
            buckets.retain(|_, bucket| refilled(bucket) < capacity);
        }
        let bucket = buckets.entry(key).or_insert(Bucket {
            tokens: capacity,
            at: now,
        });
        let tokens = refilled(bucket);
        if tokens < 1.0 {
            bucket.tokens = tokens;
            bucket.at = now;
            return Verdict::Refused {
                limit: self.per_minute,
                retry_after_seconds: seconds_for(1.0 - tokens, per_millisecond),
            };
        }
        let tokens = tokens - 1.0;
        bucket.tokens = tokens;
        bucket.at = now;
        Verdict::Allowed(Allowance {
            limit: self.per_minute,
            remaining: tokens.floor() as u32,
            reset_seconds: seconds_for(capacity - tokens, per_millisecond),
        })
    }
}

/// Whole seconds, rounded up, for `missing` tokens to come back.
fn seconds_for(missing: f64, per_millisecond: f64) -> u64 {
    (missing / per_millisecond / 1000.0).ceil().max(1.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_spends_its_bucket_and_gets_a_token_back_as_time_passes() {
        let limiter = RateLimiter::new(60);
        let key = ApiKeyId::generate();
        for spent in 1..=60 {
            let Verdict::Allowed(allowance) = limiter.check(key, 0) else {
                panic!("refused too early");
            };
            assert_eq!(allowance.remaining, 60 - spent);
            assert_eq!(allowance.limit, 60);
        }
        // One a second is the refill of 60 a minute.
        assert_eq!(
            limiter.check(key, 0),
            Verdict::Refused {
                limit: 60,
                retry_after_seconds: 1
            }
        );
        assert_eq!(
            limiter.check(key, 999),
            Verdict::Refused {
                limit: 60,
                retry_after_seconds: 1
            }
        );
        assert!(matches!(limiter.check(key, 1_000), Verdict::Allowed(_)));
        assert!(matches!(limiter.check(key, 1_000), Verdict::Refused { .. }));
    }

    #[test]
    fn each_key_has_a_bucket_of_its_own_and_an_idle_one_is_full_again() {
        let limiter = RateLimiter::new(2);
        let (a, b) = (ApiKeyId::generate(), ApiKeyId::generate());
        assert!(matches!(limiter.check(a, 0), Verdict::Allowed(_)));
        assert!(matches!(limiter.check(a, 0), Verdict::Allowed(_)));
        assert!(matches!(limiter.check(a, 0), Verdict::Refused { .. }));
        assert!(matches!(limiter.check(b, 0), Verdict::Allowed(_)));
        let Verdict::Allowed(after_a_minute) = limiter.check(a, 60_000) else {
            panic!("an idle key was still refused");
        };
        assert_eq!(after_a_minute.remaining, 1);
    }

    #[test]
    fn the_reset_says_when_the_bucket_is_full_again() {
        let limiter = RateLimiter::new(60);
        let key = ApiKeyId::generate();
        let Verdict::Allowed(first) = limiter.check(key, 0) else {
            panic!()
        };
        assert_eq!((first.remaining, first.reset_seconds), (59, 1));
    }

    #[test]
    fn a_clock_that_goes_back_never_gives_tokens() {
        let limiter = RateLimiter::new(1);
        let key = ApiKeyId::generate();
        assert!(matches!(limiter.check(key, 10_000), Verdict::Allowed(_)));
        assert!(matches!(limiter.check(key, 5_000), Verdict::Refused { .. }));
    }
}
