//! Header-Based Rate Limit Manager
//!
//! Dynamically adjusts request timing based on Anthropic API response headers:
//! - `anthropic-ratelimit-requests-remaining`
//! - `anthropic-ratelimit-requests-reset`
//! - `anthropic-ratelimit-tokens-remaining`
//! - `anthropic-ratelimit-tokens-reset`
//!
//! This replaces the fixed MIN_REQUEST_DELAY_MS approach with intelligent
//! backoff that maximizes throughput while avoiding rate limits.

use reqwest::Response;
use std::time::{Duration, Instant};

/// Rate limit state from API headers
#[derive(Debug, Clone)]
pub struct RateLimitState {
    /// Requests remaining in current window
    pub requests_remaining: Option<u32>,
    /// Timestamp when request limit resets (seconds since epoch)
    pub requests_reset: Option<u64>,
    /// Tokens remaining in current window
    pub tokens_remaining: Option<u32>,
    /// Timestamp when token limit resets
    pub tokens_reset: Option<u64>,
    /// When this state was recorded
    pub recorded_at: Instant,
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self {
            requests_remaining: None,
            requests_reset: None,
            tokens_remaining: None,
            tokens_reset: None,
            recorded_at: Instant::now(),
        }
    }
}

/// Manages rate limiting based on API response headers
///
/// # Usage
/// ```ignore
/// let mut rate_limiter = RateLimitManager::new();
///
/// // Before each request
/// let delay = rate_limiter.get_delay();
/// tokio::time::sleep(delay).await;
///
/// // After receiving response
/// rate_limiter.update_from_response(&response);
/// ```
pub struct RateLimitManager {
    /// Current rate limit state
    state: RateLimitState,
    /// Minimum delay between requests (floor)
    min_delay: Duration,
    /// Maximum delay between requests (ceiling)
    max_delay: Duration,
    /// Whether we've received any header info yet
    has_header_info: bool,
}

impl RateLimitManager {
    /// Create a new rate limit manager with default settings
    pub fn new() -> Self {
        Self {
            state: RateLimitState::default(),
            min_delay: Duration::from_millis(500), // Reduced from 2500ms
            max_delay: Duration::from_secs(60),
            has_header_info: false,
        }
    }

    /// Create with custom delay bounds
    pub fn with_bounds(min_delay: Duration, max_delay: Duration) -> Self {
        Self {
            state: RateLimitState::default(),
            min_delay,
            max_delay,
            has_header_info: false,
        }
    }

    /// Update state from API response headers
    ///
    /// Call this after every API response to keep the rate limiter informed
    pub fn update_from_response(&mut self, response: &Response) {
        let headers = response.headers();

        self.state = RateLimitState {
            requests_remaining: headers
                .get("anthropic-ratelimit-requests-remaining")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok()),
            requests_reset: headers
                .get("anthropic-ratelimit-requests-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_reset_timestamp),
            tokens_remaining: headers
                .get("anthropic-ratelimit-tokens-remaining")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok()),
            tokens_reset: headers
                .get("anthropic-ratelimit-tokens-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_reset_timestamp),
            recorded_at: Instant::now(),
        };

        // Mark that we now have header info
        if self.state.requests_remaining.is_some() || self.state.tokens_remaining.is_some() {
            self.has_header_info = true;
        }

        eprintln!(
            "[RateLimiter] Updated: requests_remaining={:?}, tokens_remaining={:?}",
            self.state.requests_remaining, self.state.tokens_remaining
        );
    }

    /// Calculate recommended delay before next request
    ///
    /// Returns a Duration based on current quota state:
    /// - High quota (>5 remaining): Use minimum delay
    /// - Medium quota (2-5 remaining): Gradual backoff
    /// - Low quota (<=1 remaining): Wait until reset time
    pub fn get_delay(&self) -> Duration {
        // If no header info yet, use conservative default
        if !self.has_header_info {
            return Duration::from_millis(1000);
        }

        // Check requests remaining
        if let Some(remaining) = self.state.requests_remaining {
            // Plenty of quota - use minimal delay
            if remaining > 5 {
                return self.min_delay;
            }

            // Very low quota - wait until reset
            if remaining <= 1 {
                if let Some(reset) = self.state.requests_reset {
                    let now = current_timestamp_secs();
                    if reset > now {
                        let wait_secs = (reset - now).min(self.max_delay.as_secs());
                        let delay = Duration::from_secs(wait_secs);
                        eprintln!(
                            "[RateLimiter] Low quota ({} remaining), waiting {:?} until reset",
                            remaining, delay
                        );
                        return delay;
                    }
                }
                // No reset info, use max delay as fallback
                return Duration::from_secs(5);
            }

            // Moderate quota - gradual backoff
            // remaining: 5 -> 500ms, 4 -> 1000ms, 3 -> 1500ms, 2 -> 2000ms
            let backoff_factor = (6 - remaining) as u64;
            return Duration::from_millis(500 * backoff_factor);
        }

        // No request info but have header info - use default
        Duration::from_millis(1000)
    }

    /// Check if we should wait before making a request
    ///
    /// Returns true if quota is exhausted (0-1 remaining)
    pub fn should_wait(&self) -> bool {
        if let Some(remaining) = self.state.requests_remaining {
            remaining <= 1
        } else {
            false
        }
    }

    /// Check if we're in a rate-limited state
    pub fn is_rate_limited(&self) -> bool {
        matches!(self.state.requests_remaining, Some(0))
    }

    /// Get current state for debugging/logging
    pub fn state(&self) -> &RateLimitState {
        &self.state
    }

    /// Get the minimum delay setting
    pub fn min_delay(&self) -> Duration {
        self.min_delay
    }

    /// Get the maximum delay setting
    pub fn max_delay(&self) -> Duration {
        self.max_delay
    }

    /// Reset state (e.g., after a long pause)
    pub fn reset(&mut self) {
        self.state = RateLimitState::default();
        self.has_header_info = false;
    }
}

impl Default for RateLimitManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current timestamp in seconds since Unix epoch
fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parse reset timestamp from header value
///
/// Supports both Unix timestamp and ISO 8601 formats
fn parse_reset_timestamp(s: &str) -> Option<u64> {
    // Try parsing as Unix timestamp first (most common)
    if let Ok(ts) = s.parse::<u64>() {
        return Some(ts);
    }

    // Try parsing as ISO 8601 (RFC 3339)
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_default() {
        let limiter = RateLimitManager::new();
        // Without header info, should use conservative default
        assert!(limiter.get_delay() >= Duration::from_millis(500));
    }

    #[test]
    fn test_parse_reset_timestamp_unix() {
        assert_eq!(parse_reset_timestamp("1700000000"), Some(1700000000));
    }

    #[test]
    fn test_parse_reset_timestamp_iso() {
        let result = parse_reset_timestamp("2024-01-15T12:00:00Z");
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_reset_timestamp_invalid() {
        assert_eq!(parse_reset_timestamp("invalid"), None);
    }

    #[test]
    fn test_should_wait() {
        let mut limiter = RateLimitManager::new();

        // Initially should not wait (no info)
        assert!(!limiter.should_wait());

        // Simulate low quota
        limiter.state.requests_remaining = Some(1);
        assert!(limiter.should_wait());

        // Simulate high quota
        limiter.state.requests_remaining = Some(10);
        assert!(!limiter.should_wait());
    }

    #[test]
    fn test_is_rate_limited() {
        let mut limiter = RateLimitManager::new();

        // Initially not rate limited
        assert!(!limiter.is_rate_limited());

        // Zero remaining = rate limited
        limiter.state.requests_remaining = Some(0);
        assert!(limiter.is_rate_limited());

        // One remaining = not rate limited (but should wait)
        limiter.state.requests_remaining = Some(1);
        assert!(!limiter.is_rate_limited());
    }

    #[test]
    fn test_custom_bounds() {
        let limiter = RateLimitManager::with_bounds(
            Duration::from_millis(100),
            Duration::from_secs(30),
        );
        assert_eq!(limiter.min_delay(), Duration::from_millis(100));
        assert_eq!(limiter.max_delay(), Duration::from_secs(30));
    }

    #[test]
    fn test_reset() {
        let mut limiter = RateLimitManager::new();
        limiter.state.requests_remaining = Some(5);
        limiter.has_header_info = true;

        limiter.reset();

        assert!(limiter.state.requests_remaining.is_none());
        assert!(!limiter.has_header_info);
    }
}
