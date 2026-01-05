//! Billing module for subscription management and usage tracking
//!
//! This module handles:
//! - Subscription tier management (Free/Pro)
//! - Daily usage tracking per model
//! - Limit enforcement before API calls
//! - Local caching with Convex sync

mod limits;
mod subscription;
mod types;
mod usage;

pub use limits::LimitEnforcer;
pub use subscription::SubscriptionManager;
#[allow(unused_imports)]
pub use types::{
    DailyLimits, DailyUsage, LimitCheckResult, LimitDenialReason, ModelPricing,
    MonthlyTokenQuota, SubscriptionCache, SubscriptionInfo, SubscriptionStatus, SubscriptionTier,
};
pub use usage::UsageTracker;

use std::sync::Arc;

/// Billing state managed by Tauri
pub struct BillingState {
    pub usage_tracker: UsageTracker,
    pub subscription_manager: SubscriptionManager,
    pub limit_enforcer: Arc<LimitEnforcer>,
}

impl BillingState {
    /// Create a new billing state
    pub fn new() -> Result<Self, String> {
        let usage_tracker = UsageTracker::new()?;
        let subscription_manager = SubscriptionManager::new();
        let limit_enforcer = Arc::new(LimitEnforcer::new());

        Ok(Self {
            usage_tracker,
            subscription_manager,
            limit_enforcer,
        })
    }
}

impl Default for BillingState {
    fn default() -> Self {
        Self::new().expect("Failed to initialize billing state")
    }
}
