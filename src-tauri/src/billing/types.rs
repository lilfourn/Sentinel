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

/// Monthly token quotas by tier (in tokens)
/// Based on approximate usage patterns and cost management
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyTokenQuota {
    /// Maximum input tokens per month
    pub max_input_tokens: u64,
    /// Maximum output tokens per month
    pub max_output_tokens: u64,
    /// Soft limit warning threshold (percentage of max)
    pub warning_threshold_percent: u8,
}

impl MonthlyTokenQuota {
    /// Get token quota for a given tier
    pub fn for_tier(tier: SubscriptionTier) -> Self {
        match tier {
            // Free tier: ~$0.50 worth of Haiku per month
            // Haiku: $0.25/1M input, $1.25/1M output
            // ~500K input, ~100K output
            SubscriptionTier::Free => Self {
                max_input_tokens: 500_000,
                max_output_tokens: 100_000,
                warning_threshold_percent: 80,
            },
            // Pro tier: ~$25 worth of mixed usage per month
            // Allows significant usage across all models
            // ~10M input (mix of models), ~2M output
            SubscriptionTier::Pro => Self {
                max_input_tokens: 10_000_000,
                max_output_tokens: 2_000_000,
                warning_threshold_percent: 80,
            },
        }
    }

    /// Check if usage exceeds quota
    pub fn is_exceeded(&self, input_used: u64, output_used: u64) -> bool {
        input_used >= self.max_input_tokens || output_used >= self.max_output_tokens
    }

    /// Check if approaching quota (soft warning)
    pub fn is_approaching(&self, input_used: u64, output_used: u64) -> bool {
        let input_threshold = (self.max_input_tokens * self.warning_threshold_percent as u64) / 100;
        let output_threshold = (self.max_output_tokens * self.warning_threshold_percent as u64) / 100;
        input_used >= input_threshold || output_used >= output_threshold
    }

    /// Get remaining tokens
    pub fn remaining(&self, input_used: u64, output_used: u64) -> (u64, u64) {
        (
            self.max_input_tokens.saturating_sub(input_used),
            self.max_output_tokens.saturating_sub(output_used),
        )
    }
}

/// Estimated cost per 1M tokens by model (in cents)
/// Based on Anthropic API pricing as of 2025
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ModelPricing {
    pub input_per_million: u32,  // cents per 1M input tokens
    pub output_per_million: u32, // cents per 1M output tokens
}

#[allow(dead_code)]
impl ModelPricing {
    /// Get pricing for a model
    pub fn for_model(model: &str) -> Self {
        if model.contains("haiku") {
            // Claude Haiku: $0.25/1M input, $1.25/1M output
            Self {
                input_per_million: 25,
                output_per_million: 125,
            }
        } else if model.contains("sonnet") {
            // Claude Sonnet: $3/1M input, $15/1M output
            Self {
                input_per_million: 300,
                output_per_million: 1500,
            }
        } else if model.contains("opus") {
            // Claude Opus: $15/1M input, $75/1M output
            Self {
                input_per_million: 1500,
                output_per_million: 7500,
            }
        } else {
            // Default to Haiku pricing
            Self {
                input_per_million: 25,
                output_per_million: 125,
            }
        }
    }

    /// Calculate cost in cents for given token counts
    pub fn calculate_cost_cents(&self, input_tokens: u64, output_tokens: u64) -> u64 {
        let input_cost = (input_tokens * self.input_per_million as u64) / 1_000_000;
        let output_cost = (output_tokens * self.output_per_million as u64) / 1_000_000;
        input_cost + output_cost
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
    #[allow(dead_code)]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    /// Get the denial reason if denied
    #[allow(dead_code)]
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
    /// Monthly token quota exceeded
    #[serde(rename = "tokenQuotaExceeded")]
    TokenQuotaExceeded {
        input_used: u64,
        input_limit: u64,
        output_used: u64,
        output_limit: u64,
    },
    /// Approaching token quota (soft warning)
    #[serde(rename = "tokenQuotaWarning")]
    TokenQuotaWarning {
        input_percent: u8,
        output_percent: u8,
    },
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
            Self::TokenQuotaExceeded {
                input_used,
                input_limit,
                output_used,
                output_limit,
            } => {
                write!(
                    f,
                    "Monthly token quota exceeded: input {}/{}, output {}/{}",
                    input_used, input_limit, output_used, output_limit
                )
            }
            Self::TokenQuotaWarning {
                input_percent,
                output_percent,
            } => {
                write!(
                    f,
                    "Approaching token quota: {}% input, {}% output used",
                    input_percent, output_percent
                )
            }
        }
    }
}

/// Request to check limits (from frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
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
