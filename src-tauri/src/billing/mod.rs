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

/// Maximum allowed length for user IDs
const MAX_USER_ID_LENGTH: usize = 256;

/// Validate user ID format and length
pub fn validate_user_id(user_id: &str) -> Result<(), String> {
    if user_id.is_empty() {
        return Err("User ID cannot be empty".to_string());
    }
    if user_id.len() > MAX_USER_ID_LENGTH {
        return Err(format!(
            "User ID too long (max {} characters)",
            MAX_USER_ID_LENGTH
        ));
    }
    // Check for obviously invalid characters
    if user_id.contains('\0') || user_id.contains('\n') || user_id.contains('\r') {
        return Err("User ID contains invalid characters".to_string());
    }
    Ok(())
}

/// Validate date format (YYYY-MM-DD)
pub fn validate_date_format(date: &str) -> Result<(), String> {
    use chrono::NaiveDate;
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| "Invalid date format, expected YYYY-MM-DD".to_string())?;
    Ok(())
}

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
