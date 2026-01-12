import { useEffect, useRef, useCallback } from "react";
import { useMutation, useQuery } from "convex/react";
import { invoke } from "@tauri-apps/api/core";
import { api } from "../../../convex/_generated/api";
import { useDesktopAuth } from "../../contexts/DesktopAuthContext";
import { setAuthTokenGetter, useSubscriptionStore, DailyUsage } from "../../stores/subscription-store";

/**
 * Desktop Auth Sync - Similar to AuthSync but uses desktop auth context
 *
 * This component syncs user data to Convex when using desktop auth mode
 * (production Tauri builds). It mirrors AuthSync functionality but uses
 * the DesktopAuthContext instead of Clerk hooks.
 */
export function DesktopAuthSync() {
  const { isLoaded, isSignedIn, user, getToken } = useDesktopAuth();
  const getOrCreateUser = useMutation(api.users.getOrCreateUser);
  const hasSynced = useRef(false);

  // Fetch subscription from Convex (auto-updates on changes)
  const subscription = useQuery(api.subscriptions.getSubscription);

  // Fetch daily usage from Convex (auto-updates on changes)
  const dailyUsage = useQuery(api.subscriptions.getDailyUsage);

  // Update subscription store when user changes
  const setUserId = useSubscriptionStore((s) => s.setUserId);

  useEffect(() => {
    if (isSignedIn && user?.id) {
      setUserId(user.id);
    } else {
      setUserId(null);
    }
  }, [isSignedIn, user?.id, setUserId]);

  // Handle loading state
  useEffect(() => {
    if (subscription === undefined && isSignedIn) {
      useSubscriptionStore.setState({ isLoading: true });
    }
  }, [subscription, isSignedIn]);

  // Sync subscription data from Convex to both frontend store AND Rust backend cache
  useEffect(() => {
    if (subscription && user?.id) {
      console.log("[DesktopAuthSync] Syncing subscription from Convex:", {
        tier: subscription.tier,
        status: subscription.status,
        cancelAtPeriodEnd: subscription.cancelAtPeriodEnd,
        stripeCustomerId: subscription.stripeCustomerId,
      });

      // Update frontend Zustand store
      useSubscriptionStore.setState({
        tier: subscription.tier,
        status: subscription.status,
        stripeCustomerId: subscription.stripeCustomerId ?? null,
        currentPeriodEnd: subscription.currentPeriodEnd ?? null,
        cancelAtPeriodEnd: subscription.cancelAtPeriodEnd ?? false,
        isLoading: false,
        lastSyncedAt: Date.now(),
        error: null,
      });

      // Sync to Rust backend cache for billing checks
      invoke('update_subscription_cache', {
        userId: user.id,
        tier: subscription.tier,
        status: subscription.status,
        stripeCustomerId: subscription.stripeCustomerId ?? null,
        stripeSubscriptionId: 'stripeSubscriptionId' in subscription ? subscription.stripeSubscriptionId ?? null : null,
        currentPeriodEnd: subscription.currentPeriodEnd ?? null,
      }).catch((err) => {
        console.error("[DesktopAuthSync] Failed to sync subscription to Rust backend:", err);
      });
    } else if (subscription === null && isSignedIn) {
      console.warn("[DesktopAuthSync] Subscription query returned null for signed-in user");
      useSubscriptionStore.setState({ isLoading: false });
    }
  }, [subscription, isSignedIn, user?.id]);

  // Sync daily usage from Convex to local store
  useEffect(() => {
    if (dailyUsage && user?.id) {
      console.log("[DesktopAuthSync] Syncing daily usage from Convex:", {
        date: dailyUsage.date,
        haiku: dailyUsage.haikuRequests,
        sonnet: dailyUsage.sonnetRequests,
        gpt52: dailyUsage.gpt52Requests ?? 0,
        gpt5mini: dailyUsage.gpt5miniRequests ?? 0,
        gpt5nano: dailyUsage.gpt5nanoRequests ?? 0,
      });

      // Map Convex usage to local store format
      const usage: DailyUsage = {
        date: dailyUsage.date,
        haikuRequests: dailyUsage.haikuRequests,
        sonnetRequests: dailyUsage.sonnetRequests,
        extendedThinkingRequests: dailyUsage.extendedThinkingRequests,
        totalInputTokens: 0,
        totalOutputTokens: 0,
        gpt52Requests: dailyUsage.gpt52Requests ?? 0,
        gpt5miniRequests: dailyUsage.gpt5miniRequests ?? 0,
        gpt5nanoRequests: dailyUsage.gpt5nanoRequests ?? 0,
      };

      // Update frontend Zustand store with Convex usage data
      useSubscriptionStore.setState({ usage });

      // Sync to Rust backend for consistency
      invoke('sync_usage_from_convex', {
        userId: user.id,
        date: usage.date,
        haikuRequests: usage.haikuRequests,
        sonnetRequests: usage.sonnetRequests,
        extendedThinkingRequests: usage.extendedThinkingRequests,
        gpt52Requests: usage.gpt52Requests,
        gpt5miniRequests: usage.gpt5miniRequests,
        gpt5nanoRequests: usage.gpt5nanoRequests,
      }).catch((err) => {
        console.debug("[DesktopAuthSync] Failed to sync usage to Rust backend:", err);
      });
    }
  }, [dailyUsage, user?.id]);

  // Create stable token getter for subscription store
  const getConvexToken = useCallback(async () => {
    return await getToken();
  }, [getToken]);

  // Register token getter with subscription store
  useEffect(() => {
    setAuthTokenGetter(getConvexToken);
  }, [getConvexToken]);

  // Sync user to Convex on first sign-in
  useEffect(() => {
    if (isLoaded && isSignedIn && !hasSynced.current) {
      hasSynced.current = true;
      getOrCreateUser()
        .then(() => {
          console.log("[DesktopAuthSync] User synced to Convex");
        })
        .catch((error) => {
          console.error("[DesktopAuthSync] Failed to sync user:", error);
          hasSynced.current = false;
        });
    }
  }, [isLoaded, isSignedIn, getOrCreateUser]);

  return null;
}
