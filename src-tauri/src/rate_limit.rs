//! Rate limiting for API endpoints
//!
//! Provides sliding window rate limiting to prevent API flooding.
//! Each user has their own request history tracked independently.
//! Uses DashMap for lock-free concurrent access.

use dashmap::DashMap;
use std::time::{Duration, Instant};
use tracing::warn;

/// Sliding window rate limiter using lock-free DashMap
/// Tracks requests per user within a time window
pub struct RateLimiter {
    /// Map of user_id -> list of request timestamps (lock-free)
    requests: DashMap<String, Vec<Instant>>,
    /// Maximum requests allowed in the window
    max_requests: usize,
    /// Time window for rate limiting
    window: Duration,
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    /// * `max_requests` - Maximum number of requests allowed in the window
    /// * `window_secs` - Duration of the sliding window in seconds
    pub fn new(max_requests: usize, window_secs: u64) -> Self {
        Self {
            requests: DashMap::new(),
            max_requests,
            window: Duration::from_secs(window_secs),
        }
    }

    /// Check if a request is allowed and record it if so
    ///
    /// Returns `true` if the request is allowed, `false` if rate limited
    pub fn check_and_record(&self, user_id: &str) -> bool {
        let now = Instant::now();
        let window = self.window;
        let max_requests = self.max_requests;

        let mut entry = self.requests.entry(user_id.to_string()).or_default();
        let timestamps = entry.value_mut();

        // Remove requests outside the window
        timestamps.retain(|t| now.duration_since(*t) < window);

        if timestamps.len() >= max_requests {
            warn!(
                user = user_id,
                requests = timestamps.len(),
                max = max_requests,
                "Rate limit exceeded"
            );
            return false;
        }

        timestamps.push(now);
        true
    }

    /// Get remaining requests for a user
    #[allow(dead_code)]
    pub fn remaining(&self, user_id: &str) -> usize {
        let now = Instant::now();
        let count = self
            .requests
            .get(user_id)
            .map(|entry| {
                entry
                    .value()
                    .iter()
                    .filter(|t| now.duration_since(**t) < self.window)
                    .count()
            })
            .unwrap_or(0);

        self.max_requests.saturating_sub(count)
    }

    /// Clean up old entries to prevent memory growth
    /// Should be called periodically
    #[allow(dead_code)]
    pub fn cleanup(&self) {
        let now = Instant::now();
        let window = self.window;

        // Remove entries with no recent requests
        self.requests.retain(|_, times| {
            times.retain(|t| now.duration_since(*t) < window);
            !times.is_empty()
        });
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        // Default: 20 requests per 60 seconds
        Self::new(20, 60)
    }
}

/// State wrapper for Tauri managed state
pub type RateLimitState = RateLimiter;

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_rate_limit_allows_under_limit() {
        let limiter = RateLimiter::new(3, 60);
        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user1"));
    }

    #[test]
    fn test_rate_limit_blocks_over_limit() {
        let limiter = RateLimiter::new(2, 60);
        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user1"));
        assert!(!limiter.check_and_record("user1")); // Should be blocked
    }

    #[test]
    fn test_rate_limit_per_user() {
        let limiter = RateLimiter::new(1, 60);
        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user2")); // Different user, allowed
        assert!(!limiter.check_and_record("user1")); // Same user, blocked
    }

    #[test]
    fn test_rate_limit_window_expiry() {
        let limiter = RateLimiter::new(1, 1); // 1 second window
        assert!(limiter.check_and_record("user1"));
        assert!(!limiter.check_and_record("user1"));

        // Wait for window to expire
        sleep(Duration::from_millis(1100));

        assert!(limiter.check_and_record("user1")); // Should be allowed again
    }

    #[test]
    fn test_remaining_count() {
        let limiter = RateLimiter::new(5, 60);
        assert_eq!(limiter.remaining("user1"), 5);

        limiter.check_and_record("user1");
        assert_eq!(limiter.remaining("user1"), 4);

        limiter.check_and_record("user1");
        limiter.check_and_record("user1");
        assert_eq!(limiter.remaining("user1"), 2);
    }
}
