//! Limit enforcement for API requests
//!
//! Checks subscription tier and daily usage before allowing API calls.

use chrono::Utc;

use super::types::{
    DailyLimits, DailyUsage, LimitCheckResult, LimitDenialReason, MonthlyTokenQuota,
    SubscriptionCache, SubscriptionStatus, SubscriptionTier,
};

/// Limit enforcement service
pub struct LimitEnforcer;

impl LimitEnforcer {
    /// Create a new limit enforcer
    pub fn new() -> Self {
        Self
    }

    /// Check if a request is allowed based on subscription and usage
    ///
    /// Returns `LimitCheckResult::Allowed` with remaining count,
    /// or `LimitCheckResult::Denied` with reason and optional upgrade URL.
    pub fn check_limit(
        &self,
        subscription: &SubscriptionCache,
        usage: &DailyUsage,
        model: &str,
        extended_thinking: bool,
    ) -> LimitCheckResult {
        let tier = subscription.tier;
        let status = subscription.status;

        // Check subscription status
        // Allow active and trialing subscriptions
        // Also allow canceled subscriptions if the period hasn't ended yet
        // (user canceled but paid through end of billing period)
        let is_subscription_valid = match status {
            SubscriptionStatus::Active | SubscriptionStatus::Trialing => true,
            SubscriptionStatus::Canceled => {
                // Check if the subscription period is still valid
                if let Some(period_end) = subscription.current_period_end {
                    let now_ms = Utc::now().timestamp_millis();
                    let still_valid = period_end > now_ms;
                    if still_valid {
                        eprintln!(
                            "[LimitEnforcer] Canceled subscription still valid until {}",
                            period_end
                        );
                    }
                    still_valid
                } else {
                    // No period end set, treat as expired
                    false
                }
            }
            SubscriptionStatus::PastDue | SubscriptionStatus::Incomplete => false,
        };

        if !is_subscription_valid {
            return LimitCheckResult::Denied {
                reason: LimitDenialReason::SubscriptionInactive { status },
                upgrade_url: Some(Self::get_upgrade_url()),
            };
        }

        let limits = DailyLimits::for_tier(tier);

        // Check model access
        let model_limit = limits.get_limit(model);
        if model_limit == 0 {
            // Model not available on this tier
            return LimitCheckResult::Denied {
                reason: LimitDenialReason::ModelNotAllowed {
                    model: Self::model_display_name(model),
                    required_tier: SubscriptionTier::Pro,
                },
                upgrade_url: Some(Self::get_upgrade_url()),
            };
        }

        // Check extended thinking access
        if extended_thinking && tier == SubscriptionTier::Free {
            return LimitCheckResult::Denied {
                reason: LimitDenialReason::ExtendedThinkingNotAllowed,
                upgrade_url: Some(Self::get_upgrade_url()),
            };
        }

        // Check daily limit for model
        let model_usage = usage.get_usage(model);
        if model_usage >= model_limit {
            return LimitCheckResult::Denied {
                reason: LimitDenialReason::DailyLimitExceeded {
                    model: Self::model_display_name(model),
                    limit: model_limit,
                    used: model_usage,
                },
                upgrade_url: if tier == SubscriptionTier::Free {
                    Some(Self::get_upgrade_url())
                } else {
                    None
                },
            };
        }

        // Check extended thinking limit
        if extended_thinking {
            if usage.extended_thinking_requests >= limits.extended_thinking_requests {
                return LimitCheckResult::Denied {
                    reason: LimitDenialReason::DailyLimitExceeded {
                        model: "Extended Thinking".to_string(),
                        limit: limits.extended_thinking_requests,
                        used: usage.extended_thinking_requests,
                    },
                    upgrade_url: None,
                };
            }
        }

        // All checks passed
        let remaining = model_limit - model_usage;
        LimitCheckResult::Allowed { remaining }
    }

    /// Quick check if user can use a model (without usage data)
    pub fn can_use_model(&self, tier: SubscriptionTier, model: &str) -> bool {
        let limits = DailyLimits::for_tier(tier);
        limits.get_limit(model) > 0
    }

    /// Quick check if user can use extended thinking
    pub fn can_use_extended_thinking(&self, tier: SubscriptionTier) -> bool {
        tier == SubscriptionTier::Pro
    }

    /// Check if monthly token quota allows a request
    ///
    /// Returns Ok(()) if within quota, or Err with denial reason if exceeded.
    /// Also emits a warning if approaching quota.
    pub fn check_token_quota(
        &self,
        tier: SubscriptionTier,
        monthly_input_tokens: u64,
        monthly_output_tokens: u64,
    ) -> Result<Option<LimitDenialReason>, LimitDenialReason> {
        let quota = MonthlyTokenQuota::for_tier(tier);

        // Check if exceeded
        if quota.is_exceeded(monthly_input_tokens, monthly_output_tokens) {
            return Err(LimitDenialReason::TokenQuotaExceeded {
                input_used: monthly_input_tokens,
                input_limit: quota.max_input_tokens,
                output_used: monthly_output_tokens,
                output_limit: quota.max_output_tokens,
            });
        }

        // Check if approaching (return warning but still allow)
        if quota.is_approaching(monthly_input_tokens, monthly_output_tokens) {
            let input_percent = if quota.max_input_tokens > 0 {
                ((monthly_input_tokens * 100) / quota.max_input_tokens) as u8
            } else {
                0
            };
            let output_percent = if quota.max_output_tokens > 0 {
                ((monthly_output_tokens * 100) / quota.max_output_tokens) as u8
            } else {
                0
            };

            return Ok(Some(LimitDenialReason::TokenQuotaWarning {
                input_percent,
                output_percent,
            }));
        }

        Ok(None)
    }

    /// Get remaining token quota
    #[allow(dead_code)]
    pub fn get_remaining_quota(
        &self,
        tier: SubscriptionTier,
        monthly_input_tokens: u64,
        monthly_output_tokens: u64,
    ) -> (u64, u64) {
        let quota = MonthlyTokenQuota::for_tier(tier);
        quota.remaining(monthly_input_tokens, monthly_output_tokens)
    }

    /// Get display name for a model
    fn model_display_name(model: &str) -> String {
        if model.contains("haiku") {
            "Haiku".to_string()
        } else if model.contains("sonnet") {
            "Sonnet".to_string()
        } else if model.contains("opus") {
            "Opus".to_string()
        } else {
            model.to_string()
        }
    }

    /// Get the upgrade URL
    fn get_upgrade_url() -> String {
        // This will be handled by the frontend to open Stripe checkout
        "sentinel://upgrade".to_string()
    }
}

impl Default for LimitEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_subscription(tier: SubscriptionTier) -> SubscriptionCache {
        SubscriptionCache {
            user_id: "test_user".to_string(),
            tier,
            status: SubscriptionStatus::Active,
            stripe_customer_id: None,
            stripe_subscription_id: None,
            current_period_end: None,
            cached_at: Utc::now().timestamp_millis(),
        }
    }

    fn make_usage(haiku: u32, sonnet: u32, opus: u32, thinking: u32) -> DailyUsage {
        DailyUsage {
            date: "2025-01-01".to_string(),
            haiku_requests: haiku,
            sonnet_requests: sonnet,
            opus_requests: opus,
            extended_thinking_requests: thinking,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    #[test]
    fn test_free_tier_haiku_allowed() {
        let enforcer = LimitEnforcer::new();
        let sub = make_subscription(SubscriptionTier::Free);
        let usage = make_usage(50, 0, 0, 0);

        let result = enforcer.check_limit(&sub, &usage, "claude-haiku-4-5", false);
        assert!(result.is_allowed());
        if let LimitCheckResult::Allowed { remaining } = result {
            assert_eq!(remaining, 50); // 100 - 50
        }
    }

    #[test]
    fn test_free_tier_sonnet_denied() {
        let enforcer = LimitEnforcer::new();
        let sub = make_subscription(SubscriptionTier::Free);
        let usage = make_usage(0, 0, 0, 0);

        let result = enforcer.check_limit(&sub, &usage, "claude-sonnet-4-5", false);
        assert!(!result.is_allowed());
        assert!(matches!(
            result.denial_reason(),
            Some(LimitDenialReason::ModelNotAllowed { .. })
        ));
    }

    #[test]
    fn test_free_tier_extended_thinking_denied() {
        let enforcer = LimitEnforcer::new();
        let sub = make_subscription(SubscriptionTier::Free);
        let usage = make_usage(0, 0, 0, 0);

        let result = enforcer.check_limit(&sub, &usage, "claude-haiku-4-5", true);
        assert!(!result.is_allowed());
        assert!(matches!(
            result.denial_reason(),
            Some(LimitDenialReason::ExtendedThinkingNotAllowed)
        ));
    }

    #[test]
    fn test_free_tier_limit_exceeded() {
        let enforcer = LimitEnforcer::new();
        let sub = make_subscription(SubscriptionTier::Free);
        let usage = make_usage(100, 0, 0, 0); // At limit

        let result = enforcer.check_limit(&sub, &usage, "claude-haiku-4-5", false);
        assert!(!result.is_allowed());
        assert!(matches!(
            result.denial_reason(),
            Some(LimitDenialReason::DailyLimitExceeded { .. })
        ));
    }

    #[test]
    fn test_pro_tier_all_models_allowed() {
        let enforcer = LimitEnforcer::new();
        let sub = make_subscription(SubscriptionTier::Pro);
        let usage = make_usage(0, 0, 0, 0);

        // Haiku
        assert!(enforcer
            .check_limit(&sub, &usage, "claude-haiku-4-5", false)
            .is_allowed());

        // Sonnet
        assert!(enforcer
            .check_limit(&sub, &usage, "claude-sonnet-4-5", false)
            .is_allowed());

        // Opus
        assert!(enforcer
            .check_limit(&sub, &usage, "claude-opus-4-5", false)
            .is_allowed());

        // Extended thinking
        assert!(enforcer
            .check_limit(&sub, &usage, "claude-opus-4-5", true)
            .is_allowed());
    }

    #[test]
    fn test_pro_tier_limits() {
        let enforcer = LimitEnforcer::new();
        let sub = make_subscription(SubscriptionTier::Pro);

        // Sonnet at limit
        let usage = make_usage(0, 50, 0, 0);
        let result = enforcer.check_limit(&sub, &usage, "claude-sonnet-4-5", false);
        assert!(!result.is_allowed());

        // Opus at limit
        let usage = make_usage(0, 0, 10, 0);
        let result = enforcer.check_limit(&sub, &usage, "claude-opus-4-5", false);
        assert!(!result.is_allowed());
    }
}
