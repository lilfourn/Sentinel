//! Subscription management with local caching
//!
//! Caches subscription status locally and syncs from Convex.
//! Uses a TTL-based cache to minimize API calls.

use chrono::Utc;
use std::collections::HashMap;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard, PoisonError};
use tracing::warn;

use super::types::{SubscriptionCache, SubscriptionStatus, SubscriptionTier};

/// Helper to acquire read lock with poison recovery
fn acquire_read_lock<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned: PoisonError<RwLockReadGuard<'_, T>>| {
        warn!("RwLock was poisoned on read, recovering inner value");
        poisoned.into_inner()
    })
}

/// Helper to acquire write lock with poison recovery
fn acquire_write_lock<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|poisoned: PoisonError<RwLockWriteGuard<'_, T>>| {
        warn!("RwLock was poisoned on write, recovering inner value");
        poisoned.into_inner()
    })
}

/// Cache TTL in milliseconds (5 minutes)
const CACHE_TTL_MS: i64 = 5 * 60 * 1000;

/// Subscription manager with local cache
pub struct SubscriptionManager {
    cache: RwLock<HashMap<String, SubscriptionCache>>,
}

impl SubscriptionManager {
    /// Create a new subscription manager
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Get cached subscription, returning None if not cached or stale
    pub fn get_cached(&self, user_id: &str) -> Option<SubscriptionCache> {
        let cache = acquire_read_lock(&self.cache);
        if let Some(sub) = cache.get(user_id) {
            // Check if cache is still fresh
            let now = Utc::now().timestamp_millis();
            let age = now - sub.cached_at;
            if age < CACHE_TTL_MS {
                return Some(sub.clone());
            }
        }
        None
    }

    /// Get cached subscription or default free tier
    pub fn get_cached_or_default(&self, user_id: &str) -> SubscriptionCache {
        self.get_cached(user_id).unwrap_or_else(|| SubscriptionCache {
            user_id: user_id.to_string(),
            tier: SubscriptionTier::Free,
            status: SubscriptionStatus::Active,
            stripe_customer_id: None,
            stripe_subscription_id: None,
            current_period_end: None,
            cached_at: Utc::now().timestamp_millis(),
        })
    }

    /// Update cache with subscription data
    pub fn update_cache(&self, subscription: SubscriptionCache) {
        let mut cache = acquire_write_lock(&self.cache);
        tracing::debug!(
            user_id = subscription.user_id,
            tier = ?subscription.tier,
            "Caching subscription"
        );
        cache.insert(subscription.user_id.clone(), subscription);
    }

    /// Update cache from Convex response
    pub fn update_from_convex(
        &self,
        user_id: &str,
        tier: &str,
        status: &str,
        stripe_customer_id: Option<String>,
        stripe_subscription_id: Option<String>,
        current_period_end: Option<i64>,
    ) {
        let tier = match tier {
            "pro" => SubscriptionTier::Pro,
            _ => SubscriptionTier::Free,
        };

        let status = match status {
            "active" => SubscriptionStatus::Active,
            "past_due" => SubscriptionStatus::PastDue,
            "canceled" => SubscriptionStatus::Canceled,
            "incomplete" => SubscriptionStatus::Incomplete,
            "trialing" => SubscriptionStatus::Trialing,
            _ => SubscriptionStatus::Active,
        };

        self.update_cache(SubscriptionCache {
            user_id: user_id.to_string(),
            tier,
            status,
            stripe_customer_id,
            stripe_subscription_id,
            current_period_end,
            cached_at: Utc::now().timestamp_millis(),
        });
    }

    /// Clear cache for a user (on logout)
    pub fn clear_cache(&self, user_id: &str) {
        let mut cache = acquire_write_lock(&self.cache);
        cache.remove(user_id);
        tracing::debug!(user_id = user_id, "Cleared subscription cache");
    }

    /// Invalidate cache for a user (forces refresh on next access)
    #[allow(dead_code)]
    pub fn invalidate(&self, user_id: &str) {
        self.clear_cache(user_id);
    }

    /// Check if user has Pro subscription
    #[allow(dead_code)]
    pub fn is_pro(&self, user_id: &str) -> bool {
        if let Some(sub) = self.get_cached(user_id) {
            sub.tier == SubscriptionTier::Pro
                && (sub.status == SubscriptionStatus::Active
                    || sub.status == SubscriptionStatus::Trialing)
        } else {
            false
        }
    }

    /// Check if subscription is active (not canceled or past_due)
    #[allow(dead_code)]
    pub fn is_active(&self, user_id: &str) -> bool {
        if let Some(sub) = self.get_cached(user_id) {
            sub.status == SubscriptionStatus::Active
                || sub.status == SubscriptionStatus::Trialing
        } else {
            true // Default to active for free tier
        }
    }

    /// Get tier for user
    pub fn get_tier(&self, user_id: &str) -> SubscriptionTier {
        self.get_cached(user_id)
            .map(|s| s.tier)
            .unwrap_or(SubscriptionTier::Free)
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

// Note: SubscriptionManager is automatically Send + Sync because:
// - RwLock<T> is Send when T: Send (HashMap<String, SubscriptionCache> is Send)
// - RwLock<T> is Sync when T: Send + Sync (HashMap<String, SubscriptionCache> is Send + Sync)
// No unsafe impl needed - the compiler derives it correctly.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_caching() {
        let manager = SubscriptionManager::new();
        let user_id = "test_user_123";

        // Initially no cache
        assert!(manager.get_cached(user_id).is_none());

        // Update cache
        manager.update_from_convex(
            user_id,
            "pro",
            "active",
            Some("cus_123".to_string()),
            Some("sub_456".to_string()),
            Some(1735689600000),
        );

        // Should be cached now
        let cached = manager.get_cached(user_id).unwrap();
        assert_eq!(cached.tier, SubscriptionTier::Pro);
        assert_eq!(cached.status, SubscriptionStatus::Active);
        assert!(manager.is_pro(user_id));

        // Clear cache
        manager.clear_cache(user_id);
        assert!(manager.get_cached(user_id).is_none());
        assert!(!manager.is_pro(user_id));
    }
}
