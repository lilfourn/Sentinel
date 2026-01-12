import { useEffect, useRef, useCallback } from "react";
import { useUser, useAuth } from "@clerk/clerk-react";
import { useMutation, useQuery } from "convex/react";
import { invoke } from "@tauri-apps/api/core";
import { api } from "../../../convex/_generated/api";
import { setAuthTokenGetter, useSubscriptionStore, DailyUsage } from "../../stores/subscription-store";

/**
 * Invisible component that syncs Clerk user to Convex on sign-in.
 * Creates user record and default settings in Convex database.
 * Also sets up auth token getter for subscription HTTP endpoints.
 * Syncs subscription data AND daily usage from Convex to the local store.
 */
export function AuthSync() {
  const { user, isSignedIn, isLoaded } = useUser();
  const { getToken } = useAuth();
  const getOrCreateUser = useMutation(api.users.getOrCreateUser);
  const hasSynced = useRef(false);

  // Fetch subscription from Convex (auto-updates on changes)
  // undefined = loading, null = not authenticated, object = subscription data
  const subscription = useQuery(api.subscriptions.getSubscription);

  // Fetch daily usage from Convex (auto-updates on changes)
  // This is the source of truth for usage tracking
  const dailyUsage = useQuery(api.subscriptions.getDailyUsage);

  // Update subscription store when Convex data changes
  const setUserId = useSubscriptionStore((s) => s.setUserId);

  useEffect(() => {
    if (isSignedIn && user?.id) {
      setUserId(user.id);
    } else {
      setUserId(null);
    }
  }, [isSignedIn, user?.id, setUserId]);

  // Handle loading state - subscription is undefined while Convex query is loading
  useEffect(() => {
    if (subscription === undefined && isSignedIn) {
      // Convex query is still loading - mark as loading
      useSubscriptionStore.setState({ isLoading: true });
    }
  }, [subscription, isSignedIn]);

  // Sync subscription data from Convex to both frontend store AND Rust backend cache
  // IMPORTANT: Always update when subscription data is available to prevent stale state
  useEffect(() => {
    if (subscription && user?.id) {
      console.log("[AuthSync] Syncing subscription from Convex:", {
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

      // CRITICAL: Also sync to Rust backend cache for billing checks
      // Without this, chat_stream defaults to FREE tier and denies requests
      invoke('update_subscription_cache', {
        userId: user.id,
        tier: subscription.tier,
        status: subscription.status,
        stripeCustomerId: subscription.stripeCustomerId ?? null,
        stripeSubscriptionId: 'stripeSubscriptionId' in subscription ? subscription.stripeSubscriptionId ?? null : null,
        currentPeriodEnd: subscription.currentPeriodEnd ?? null,
      }).catch((err) => {
        console.error("[AuthSync] Failed to sync subscription to Rust backend:", err);
      });
    } else if (subscription === null && isSignedIn) {
      // User is signed in but subscription query returned null (shouldn't happen normally)
      console.warn("[AuthSync] Subscription query returned null for signed-in user");
      useSubscriptionStore.setState({ isLoading: false });
    }
  }, [subscription, isSignedIn, user?.id]);

  // Sync daily usage from Convex to local store
  // This ensures usage persists across app refreshes
  useEffect(() => {
    if (dailyUsage && user?.id) {
      console.log("[AuthSync] Syncing daily usage from Convex:", {
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
        // Convex doesn't track tokens, set to 0
        totalInputTokens: 0,
        totalOutputTokens: 0,
        // GPT models (with fallback for older records)
        gpt52Requests: dailyUsage.gpt52Requests ?? 0,
        gpt5miniRequests: dailyUsage.gpt5miniRequests ?? 0,
        gpt5nanoRequests: dailyUsage.gpt5nanoRequests ?? 0,
      };

      // Update frontend Zustand store with Convex usage data
      useSubscriptionStore.setState({ usage });

      // Also sync to Rust backend for consistency (optional but helps keep local cache in sync)
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
        // Non-critical - Rust backend may not have this command yet
        console.debug("[AuthSync] Failed to sync usage to Rust backend:", err);
      });
    }
  }, [dailyUsage, user?.id]);

  // Create stable token getter for subscription store
  const getConvexToken = useCallback(async () => {
    // Get token for Convex (uses the convex template from Clerk)
    return await getToken({ template: "convex" });
  }, [getToken]);

  // Register token getter with subscription store
  useEffect(() => {
    setAuthTokenGetter(getConvexToken);
  }, [getConvexToken]);

  useEffect(() => {
    if (isLoaded && isSignedIn && !hasSynced.current) {
      hasSynced.current = true;
      getOrCreateUser()
        .then(() => {
          console.log("[AuthSync] User synced to Convex");
        })
        .catch((error) => {
          console.error("[AuthSync] Failed to sync user:", error);
          // Reset so we can retry
          hasSynced.current = false;
        });
    }
  }, [isLoaded, isSignedIn, getOrCreateUser]);

  return null;
}
