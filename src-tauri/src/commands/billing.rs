//! Tauri commands for billing and subscription management

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::billing::{
    validate_date_format, validate_user_id, BillingState, DailyLimits, DailyUsage,
    LimitCheckResult, LimitDenialReason, MonthlyTokenQuota, SubscriptionCache, SubscriptionInfo,
};

/// Get current daily usage
#[tauri::command]
pub async fn get_daily_usage(
    billing: State<'_, BillingState>,
    user_id: String,
) -> Result<DailyUsage, String> {
    validate_user_id(&user_id)?;
    billing.usage_tracker.get_today_usage(&user_id)
}

/// Get usage history for the current month
#[tauri::command]
pub async fn get_usage_history(
    billing: State<'_, BillingState>,
    user_id: String,
) -> Result<Vec<DailyUsage>, String> {
    billing.usage_tracker.get_month_usage(&user_id)
}

/// Check if a request would be allowed
#[tauri::command]
pub async fn check_request_limit(
    billing: State<'_, BillingState>,
    user_id: Option<String>,
    model: String,
    extended_thinking: bool,
) -> Result<LimitCheckResult, String> {
    let user_id = match user_id {
        Some(id) => id,
        None => {
            return Ok(LimitCheckResult::Denied {
                reason: crate::billing::LimitDenialReason::NotAuthenticated,
                upgrade_url: None,
            });
        }
    };

    // Get subscription (from cache or default to free)
    let subscription = billing.subscription_manager.get_cached_or_default(&user_id);

    // Get today's usage
    let usage = billing.usage_tracker.get_today_usage(&user_id)?;

    // Check limit
    let result = billing
        .limit_enforcer
        .check_limit(&subscription, &usage, &model, extended_thinking);

    Ok(result)
}

/// Update subscription cache from Convex data
#[tauri::command]
pub async fn update_subscription_cache(
    billing: State<'_, BillingState>,
    user_id: String,
    tier: String,
    status: String,
    stripe_customer_id: Option<String>,
    stripe_subscription_id: Option<String>,
    current_period_end: Option<i64>,
) -> Result<(), String> {
    billing.subscription_manager.update_from_convex(
        &user_id,
        &tier,
        &status,
        stripe_customer_id,
        stripe_subscription_id,
        current_period_end,
    );
    Ok(())
}

/// Get cached subscription info
#[tauri::command]
pub async fn get_subscription(
    billing: State<'_, BillingState>,
    user_id: String,
) -> Result<Option<SubscriptionCache>, String> {
    Ok(billing.subscription_manager.get_cached(&user_id))
}

/// Get full subscription info with limits and usage
#[tauri::command]
pub async fn get_subscription_info(
    billing: State<'_, BillingState>,
    user_id: String,
) -> Result<SubscriptionInfo, String> {
    let subscription = billing.subscription_manager.get_cached_or_default(&user_id);
    let usage = billing.usage_tracker.get_today_usage(&user_id)?;

    Ok(SubscriptionInfo {
        tier: subscription.tier,
        status: subscription.status,
        limits: DailyLimits::for_tier(subscription.tier),
        usage,
        stripe_customer_id: subscription.stripe_customer_id,
        current_period_end: subscription.current_period_end,
    })
}

/// Clear subscription cache on logout
#[tauri::command]
pub fn clear_subscription_cache(
    billing: State<'_, BillingState>,
    user_id: String,
) -> Result<(), String> {
    billing.subscription_manager.clear_cache(&user_id);
    Ok(())
}

/// Record API usage after successful request
#[tauri::command]
pub async fn record_usage(
    billing: State<'_, BillingState>,
    user_id: String,
    model: String,
    extended_thinking: bool,
    input_tokens: u64,
    output_tokens: u64,
) -> Result<(), String> {
    billing.usage_tracker.increment_request(
        &user_id,
        &model,
        extended_thinking,
        input_tokens,
        output_tokens,
    )
}

/// Check if user can use a specific model (quick check without usage)
#[tauri::command]
pub fn can_use_model(
    billing: State<'_, BillingState>,
    user_id: String,
    model: String,
) -> bool {
    let tier = billing.subscription_manager.get_tier(&user_id);
    billing.limit_enforcer.can_use_model(tier, &model)
}

/// Check if user can use extended thinking
#[tauri::command]
pub fn can_use_extended_thinking(
    billing: State<'_, BillingState>,
    user_id: String,
) -> bool {
    let tier = billing.subscription_manager.get_tier(&user_id);
    billing.limit_enforcer.can_use_extended_thinking(tier)
}

/// Token quota status response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenQuotaStatus {
    /// Whether quota is exceeded
    pub exceeded: bool,
    /// Whether approaching quota (soft warning)
    pub warning: bool,
    /// Input tokens used this month
    pub input_used: u64,
    /// Output tokens used this month
    pub output_used: u64,
    /// Maximum input tokens allowed
    pub input_limit: u64,
    /// Maximum output tokens allowed
    pub output_limit: u64,
    /// Remaining input tokens
    pub input_remaining: u64,
    /// Remaining output tokens
    pub output_remaining: u64,
    /// Usage percentage (input)
    pub input_percent: u8,
    /// Usage percentage (output)
    pub output_percent: u8,
}

/// Check monthly token quota status
#[tauri::command]
pub async fn check_token_quota(
    billing: State<'_, BillingState>,
    user_id: String,
) -> Result<TokenQuotaStatus, String> {
    let tier = billing.subscription_manager.get_tier(&user_id);
    let (input_used, output_used) = billing.usage_tracker.get_monthly_token_totals(&user_id)?;

    let quota = MonthlyTokenQuota::for_tier(tier);
    let (input_remaining, output_remaining) = quota.remaining(input_used, output_used);

    let input_percent = if quota.max_input_tokens > 0 {
        ((input_used * 100) / quota.max_input_tokens).min(100) as u8
    } else {
        0
    };
    let output_percent = if quota.max_output_tokens > 0 {
        ((output_used * 100) / quota.max_output_tokens).min(100) as u8
    } else {
        0
    };

    // Check quota status
    let check_result = billing
        .limit_enforcer
        .check_token_quota(tier, input_used, output_used);

    let (exceeded, warning) = match check_result {
        Err(LimitDenialReason::TokenQuotaExceeded { .. }) => (true, false),
        Ok(Some(LimitDenialReason::TokenQuotaWarning { .. })) => (false, true),
        _ => (false, false),
    };

    Ok(TokenQuotaStatus {
        exceeded,
        warning,
        input_used,
        output_used,
        input_limit: quota.max_input_tokens,
        output_limit: quota.max_output_tokens,
        input_remaining,
        output_remaining,
        input_percent,
        output_percent,
    })
}

/// Get monthly token totals
#[tauri::command]
pub async fn get_monthly_tokens(
    billing: State<'_, BillingState>,
    user_id: String,
) -> Result<(u64, u64), String> {
    billing.usage_tracker.get_monthly_token_totals(&user_id)
}

/// Sync usage from Convex (source of truth)
/// Called by frontend when Convex usage data is fetched
#[tauri::command]
pub async fn sync_usage_from_convex(
    billing: State<'_, BillingState>,
    user_id: String,
    date: String,
    haiku_requests: u64,
    sonnet_requests: u64,
    extended_thinking_requests: u64,
    gpt52_requests: u64,
    gpt5mini_requests: u64,
    gpt5nano_requests: u64,
) -> Result<(), String> {
    validate_user_id(&user_id)?;
    validate_date_format(&date)?;
    billing.usage_tracker.sync_from_convex(
        &user_id,
        &date,
        haiku_requests,
        sonnet_requests,
        extended_thinking_requests,
        gpt52_requests,
        gpt5mini_requests,
        gpt5nano_requests,
    )
}
