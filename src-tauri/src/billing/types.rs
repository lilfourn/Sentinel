//! Billing data types

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Subscription tier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionTier {
    Free,
    Pro,
}

impl Default for SubscriptionTier {
    fn default() -> Self {
        Self::Free
    }
}

impl std::fmt::Display for SubscriptionTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Free => write!(f, "free"),
            Self::Pro => write!(f, "pro"),
        }
    }
}

/// Subscription status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,
    PastDue,
    Canceled,
    Incomplete,
    Trialing,
}

impl Default for SubscriptionStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Model-specific daily limits
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyLimits {
    pub haiku_requests: u32,
    pub sonnet_requests: u32,
    pub opus_requests: u32,
    pub extended_thinking_requests: u32,
}

impl DailyLimits {
    /// Get limits for a given tier
    pub fn for_tier(tier: SubscriptionTier) -> Self {
        match tier {
            SubscriptionTier::Free => Self {
                haiku_requests: 100,
                sonnet_requests: 0,
                opus_requests: 0,
                extended_thinking_requests: 0,
            },
            SubscriptionTier::Pro => Self {
                haiku_requests: 300,
                sonnet_requests: 50,
                opus_requests: 10,
                extended_thinking_requests: 5,
            },
        }
    }

    /// Get limit for a specific model
    pub fn get_limit(&self, model: &str) -> u32 {
        if model.contains("haiku") {
            self.haiku_requests
        } else if model.contains("sonnet") {
            self.sonnet_requests
        } else if model.contains("opus") {
            self.opus_requests
        } else {
            self.haiku_requests // Default to haiku
        }
    }
}

/// Daily usage counters
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsage {
    pub date: String, // "YYYY-MM-DD" format
    pub haiku_requests: u32,
    pub sonnet_requests: u32,
    pub opus_requests: u32,
    pub extended_thinking_requests: u32,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

impl DailyUsage {
    /// Get usage for a specific model
    pub fn get_usage(&self, model: &str) -> u32 {
        if model.contains("haiku") {
            self.haiku_requests
        } else if model.contains("sonnet") {
            self.sonnet_requests
        } else if model.contains("opus") {
            self.opus_requests
        } else {
            self.haiku_requests // Default to haiku
        }
    }
}

/// Cached subscription status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionCache {
    pub user_id: String, // Clerk user ID
    pub tier: SubscriptionTier,
    pub status: SubscriptionStatus,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub current_period_end: Option<i64>, // Unix timestamp in ms
    pub cached_at: i64,                  // Unix timestamp in ms
}

impl Default for SubscriptionCache {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            tier: SubscriptionTier::Free,
            status: SubscriptionStatus::Active,
            stripe_customer_id: None,
            stripe_subscription_id: None,
            current_period_end: None,
            cached_at: Utc::now().timestamp_millis(),
        }
    }
}

/// Result of a limit check
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum LimitCheckResult {
    #[serde(rename = "allowed")]
    Allowed { remaining: u32 },
    #[serde(rename = "denied")]
    Denied {
        reason: LimitDenialReason,
        upgrade_url: Option<String>,
    },
}

impl LimitCheckResult {
    /// Check if the request is allowed
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    /// Get the denial reason if denied
    pub fn denial_reason(&self) -> Option<&LimitDenialReason> {
        match self {
            Self::Denied { reason, .. } => Some(reason),
            _ => None,
        }
    }
}

/// Reason for denying a request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum LimitDenialReason {
    #[serde(rename = "dailyLimitExceeded")]
    DailyLimitExceeded {
        model: String,
        limit: u32,
        used: u32,
    },
    #[serde(rename = "modelNotAllowed")]
    ModelNotAllowed {
        model: String,
        required_tier: SubscriptionTier,
    },
    #[serde(rename = "extendedThinkingNotAllowed")]
    ExtendedThinkingNotAllowed,
    #[serde(rename = "notAuthenticated")]
    NotAuthenticated,
    #[serde(rename = "subscriptionInactive")]
    SubscriptionInactive { status: SubscriptionStatus },
}

impl std::fmt::Display for LimitDenialReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DailyLimitExceeded { model, limit, used } => {
                write!(
                    f,
                    "Daily limit exceeded for {}: {}/{} requests used",
                    model, used, limit
                )
            }
            Self::ModelNotAllowed {
                model,
                required_tier,
            } => {
                write!(f, "{} requires {} subscription", model, required_tier)
            }
            Self::ExtendedThinkingNotAllowed => {
                write!(f, "Extended thinking requires Pro subscription")
            }
            Self::NotAuthenticated => {
                write!(f, "Authentication required")
            }
            Self::SubscriptionInactive { status } => {
                write!(f, "Subscription is {:?}", status)
            }
        }
    }
}

/// Request to check limits (from frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckLimitRequest {
    pub user_id: Option<String>,
    pub model: String,
    pub extended_thinking: bool,
}

/// Response with subscription and usage info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInfo {
    pub tier: SubscriptionTier,
    pub status: SubscriptionStatus,
    pub limits: DailyLimits,
    pub usage: DailyUsage,
    pub stripe_customer_id: Option<String>,
    pub current_period_end: Option<i64>,
}
