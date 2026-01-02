//! Tauri commands for billing and subscription management

use tauri::State;

use crate::billing::{
    BillingState, DailyLimits, DailyUsage, LimitCheckResult, SubscriptionCache, SubscriptionInfo,
};

/// Get current daily usage
#[tauri::command]
pub async fn get_daily_usage(
    billing: State<'_, BillingState>,
    user_id: String,
) -> Result<DailyUsage, String> {
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
