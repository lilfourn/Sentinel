//! Shared HTTP Client Module
//!
//! Provides a global, lazy-initialized HTTP client with optimized connection pooling.
//! This eliminates the overhead of creating new clients per request and enables
//! connection reuse across all AI API calls.
//!
//! Performance benefits:
//! - TLS session resumption (avoids 1-2 RTT handshake per request)
//! - Connection pooling (reuses existing TCP connections)
//! - Single client initialization (avoids repeated builder overhead)

use once_cell::sync::Lazy;
use reqwest::Client;
use std::time::Duration;

/// Global HTTP client for Anthropic API calls
///
/// Configuration optimized for streaming AI responses:
/// - 120s timeout for long-running streaming requests
/// - 20 idle connections per host for parallel requests
/// - 90s idle timeout to balance resource usage and performance
/// - HTTP/2 enabled for multiplexing
pub static ANTHROPIC_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(20)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(60))
        .tcp_nodelay(true)
        .build()
        .expect("Failed to create Anthropic HTTP client")
});

/// Global HTTP client for OpenAI API calls
///
/// Similar configuration to Anthropic client, tuned for OpenAI's API patterns.
#[allow(dead_code)]
pub static OPENAI_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(32) // Higher for parallel batch analysis
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(60))
        .tcp_nodelay(true)
        .build()
        .expect("Failed to create OpenAI HTTP client")
});

/// Global HTTP client for short API validation requests
///
/// Shorter timeout optimized for quick operations like API key validation.
#[allow(dead_code)]
pub static VALIDATION_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(5)
        .pool_idle_timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to create validation HTTP client")
});

/// Get the global Anthropic HTTP client
///
/// This function returns a reference to the lazy-initialized client.
/// The client is created on first access and reused for all subsequent calls.
#[inline]
pub fn anthropic_client() -> &'static Client {
    &ANTHROPIC_CLIENT
}

/// Get the global OpenAI HTTP client
#[inline]
#[allow(dead_code)]
pub fn openai_client() -> &'static Client {
    &OPENAI_CLIENT
}

/// Get the global validation HTTP client
#[inline]
#[allow(dead_code)]
pub fn validation_client() -> &'static Client {
    &VALIDATION_CLIENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clients_are_created() {
        // Ensure clients can be created without panicking
        let _ = anthropic_client();
        let _ = openai_client();
        let _ = validation_client();
    }

    #[test]
    fn test_clients_are_same_instance() {
        // Verify singleton pattern works
        let client1 = anthropic_client();
        let client2 = anthropic_client();
        assert!(std::ptr::eq(client1, client2));
    }
}
