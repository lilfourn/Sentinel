import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { invoke } from '@tauri-apps/api/core';

// ============================================================================
// MIGRATION: Clean up stale localStorage data
// ============================================================================

/**
 * Migrate localStorage to remove persisted tier/status fields.
 * These should always come fresh from Convex to prevent stale "free" state.
 * Runs immediately on module load.
 */
function migrateLocalStorage() {
  const STORAGE_KEY = 'sentinel-subscription';
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      // Check if we have the old format with tier/status persisted
      if (parsed.state && ('tier' in parsed.state || 'status' in parsed.state)) {
        console.log('[SubscriptionStore] Migrating localStorage - removing stale tier/status');
        // Remove tier and status from persisted state
        delete parsed.state.tier;
        delete parsed.state.status;
        delete parsed.state.currentPeriodEnd;
        localStorage.setItem(STORAGE_KEY, JSON.stringify(parsed));
      }
    }
  } catch (e) {
    // If parsing fails, clear the whole thing to start fresh
    console.warn('[SubscriptionStore] Clearing corrupted localStorage');
    localStorage.removeItem(STORAGE_KEY);
  }
}

// Run migration immediately
migrateLocalStorage();

// Auth token getter - set by AuthSync component
let getAuthToken: (() => Promise<string | null>) | null = null;

/**
 * Set the auth token getter (called by AuthSync)
 */
export function setAuthTokenGetter(getter: () => Promise<string | null>) {
  getAuthToken = getter;
}

// ============================================================================
// CONSTANTS
// ============================================================================

/** Tier limits - must match Rust/Convex */
export const TIER_LIMITS = {
  free: {
    haiku: 100,
    sonnet: 0,
    opus: 0,
    extendedThinking: 0,
  },
  pro: {
    haiku: 300,
    sonnet: 50,
    opus: 10,
    extendedThinking: 5,
  },
} as const;

/** Price for Pro tier */
export const PRO_PRICE = 19.99;

// ============================================================================
// TYPES
// ============================================================================

export type SubscriptionTier = 'free' | 'pro';
export type SubscriptionStatus = 'active' | 'past_due' | 'canceled' | 'incomplete' | 'trialing';
export type ModelType = 'haiku' | 'sonnet' | 'opus';

export interface DailyUsage {
  date: string;
  haikuRequests: number;
  sonnetRequests: number;
  opusRequests: number;
  extendedThinkingRequests: number;
  totalInputTokens: number;
  totalOutputTokens: number;
}

export interface TierLimits {
  haiku: number;
  sonnet: number;
  opus: number;
  extendedThinking: number;
}

export interface SubscriptionInfo {
  tier: SubscriptionTier;
  status: SubscriptionStatus;
  limits: TierLimits;
  usage: DailyUsage;
  stripeCustomerId: string | null;
  currentPeriodEnd: number | null;
}

export interface LimitCheckResult {
  type: 'allowed' | 'denied';
  remaining?: number;
  reason?: string;
  upgradeUrl?: string;
}

// ============================================================================
// STATE
// ============================================================================

interface SubscriptionState {
  // User identification
  userId: string | null;

  // Subscription info
  tier: SubscriptionTier;
  status: SubscriptionStatus;
  stripeCustomerId: string | null;
  currentPeriodEnd: number | null;

  // Usage tracking
  usage: DailyUsage;

  // Loading states
  isLoading: boolean;
  lastSyncedAt: number | null;
  error: string | null;
}

interface SubscriptionActions {
  // User management
  setUserId: (userId: string | null) => void;

  // Sync from backend
  syncSubscription: () => Promise<void>;
  refreshUsage: () => Promise<void>;

  // Usage tracking (optimistic)
  incrementUsage: (model: ModelType, extendedThinking?: boolean) => void;

  // Limit checks
  canUseModel: (model: ModelType) => boolean;
  canUseExtendedThinking: () => boolean;
  getRemainingForModel: (model: ModelType) => number;
  checkLimit: (model: ModelType, extendedThinking?: boolean) => LimitCheckResult;

  // Checkout
  openCheckout: () => Promise<void>;
  openCustomerPortal: () => Promise<void>;

  // Internal
  _setError: (error: string | null) => void;
  _reset: () => void;
}

type SubscriptionStore = SubscriptionState & SubscriptionActions;

// ============================================================================
// INITIAL STATE
// ============================================================================

const initialUsage: DailyUsage = {
  date: new Date().toISOString().split('T')[0],
  haikuRequests: 0,
  sonnetRequests: 0,
  opusRequests: 0,
  extendedThinkingRequests: 0,
  totalInputTokens: 0,
  totalOutputTokens: 0,
};

const initialState: SubscriptionState = {
  userId: null,
  tier: 'free',
  status: 'active',
  stripeCustomerId: null,
  currentPeriodEnd: null,
  usage: initialUsage,
  isLoading: false,
  lastSyncedAt: null,
  error: null,
};

// ============================================================================
// STORE
// ============================================================================

export const useSubscriptionStore = create<SubscriptionStore>()(
  persist(
    (set, get) => ({
      ...initialState,

      // ========================================
      // User Management
      // ========================================

      setUserId: (userId) => {
        set({ userId });
        if (userId) {
          // Sync subscription when user ID is set
          get().syncSubscription();
        } else {
          // Reset to free tier when logged out
          get()._reset();
        }
      },

      // ========================================
      // Sync from Backend
      // ========================================

      syncSubscription: async () => {
        const { userId } = get();
        if (!userId) return;

        set({ isLoading: true, error: null });

        try {
          const info = await invoke<SubscriptionInfo>('get_subscription_info', { userId });

          set({
            tier: info.tier,
            status: info.status,
            stripeCustomerId: info.stripeCustomerId,
            currentPeriodEnd: info.currentPeriodEnd,
            usage: info.usage,
            lastSyncedAt: Date.now(),
            isLoading: false,
          });
        } catch (error) {
          console.error('[SubscriptionStore] Failed to sync:', error);
          set({
            error: error instanceof Error ? error.message : 'Failed to sync subscription',
            isLoading: false,
          });
        }
      },

      refreshUsage: async () => {
        const { userId } = get();
        if (!userId) return;

        try {
          const usage = await invoke<DailyUsage>('get_daily_usage', { userId });
          set({ usage });
        } catch (error) {
          console.error('[SubscriptionStore] Failed to refresh usage:', error);
        }
      },

      // ========================================
      // Usage Tracking (Optimistic)
      // ========================================

      incrementUsage: (model, extendedThinking = false) => {
        set((state) => {
          const usage = { ...state.usage };
          const today = new Date().toISOString().split('T')[0];

          // Reset if date changed
          if (usage.date !== today) {
            return { usage: { ...initialUsage, date: today } };
          }

          // Increment model usage
          switch (model) {
            case 'haiku':
              usage.haikuRequests += 1;
              break;
            case 'sonnet':
              usage.sonnetRequests += 1;
              break;
            case 'opus':
              usage.opusRequests += 1;
              break;
          }

          if (extendedThinking) {
            usage.extendedThinkingRequests += 1;
          }

          return { usage };
        });
      },

      // ========================================
      // Limit Checks
      // ========================================

      canUseModel: (model) => {
        const { tier } = get();
        const limits = TIER_LIMITS[tier];
        return limits[model] > 0;
      },

      canUseExtendedThinking: () => {
        const { tier } = get();
        return tier === 'pro';
      },

      getRemainingForModel: (model) => {
        const { tier, usage } = get();
        const limits = TIER_LIMITS[tier];
        const limit = limits[model];

        let used = 0;
        switch (model) {
          case 'haiku':
            used = usage.haikuRequests;
            break;
          case 'sonnet':
            used = usage.sonnetRequests;
            break;
          case 'opus':
            used = usage.opusRequests;
            break;
        }

        return Math.max(0, limit - used);
      },

      checkLimit: (model, extendedThinking = false) => {
        const { tier, usage, canUseModel, canUseExtendedThinking, getRemainingForModel } = get();

        // Check model access
        if (!canUseModel(model)) {
          return {
            type: 'denied',
            reason: `${model.charAt(0).toUpperCase() + model.slice(1)} requires Pro subscription`,
            upgradeUrl: 'sentinel://upgrade',
          };
        }

        // Check extended thinking access
        if (extendedThinking && !canUseExtendedThinking()) {
          return {
            type: 'denied',
            reason: 'Extended thinking requires Pro subscription',
            upgradeUrl: 'sentinel://upgrade',
          };
        }

        // Check remaining quota
        const remaining = getRemainingForModel(model);
        if (remaining <= 0) {
          const limits = TIER_LIMITS[tier];
          return {
            type: 'denied',
            reason: `Daily limit reached for ${model}: ${limits[model]}/${limits[model]} used`,
            upgradeUrl: tier === 'free' ? 'sentinel://upgrade' : undefined,
          };
        }

        // Check extended thinking quota
        if (extendedThinking) {
          const thinkingLimit = TIER_LIMITS[tier].extendedThinking;
          if (usage.extendedThinkingRequests >= thinkingLimit) {
            return {
              type: 'denied',
              reason: `Daily limit reached for extended thinking: ${thinkingLimit}/${thinkingLimit} used`,
            };
          }
        }

        return { type: 'allowed', remaining };
      },

      // ========================================
      // Checkout
      // ========================================

      openCheckout: async () => {
        try {
          const convexUrl = import.meta.env.VITE_CONVEX_URL;
          console.log('[SubscriptionStore] Convex URL:', convexUrl);
          if (!convexUrl) {
            throw new Error('Convex URL not configured');
          }

          // Get auth token for Convex HTTP endpoint
          console.log('[SubscriptionStore] getAuthToken available:', !!getAuthToken);
          if (!getAuthToken) {
            throw new Error('Auth not initialized. Please sign in first.');
          }
          const token = await getAuthToken();
          console.log('[SubscriptionStore] Token received:', !!token);
          if (!token) {
            throw new Error('Please sign in to upgrade');
          }

          // Call Convex HTTP endpoint to create Stripe checkout session
          const checkoutEndpoint = convexUrl.replace('.convex.cloud', '.convex.site') + '/create-checkout';
          console.log('[SubscriptionStore] Calling:', checkoutEndpoint);

          const response = await fetch(checkoutEndpoint, {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              Authorization: `Bearer ${token}`,
            },
          });

          console.log('[SubscriptionStore] Response status:', response.status);
          if (!response.ok) {
            const error = await response.text();
            console.error('[SubscriptionStore] Response error:', error);
            throw new Error(`Checkout failed: ${error}`);
          }

          const data = await response.json();
          console.log('[SubscriptionStore] Response data:', data);
          const { url } = data;

          if (url) {
            // Open Stripe checkout in default browser
            console.log('[SubscriptionStore] Opening URL:', url);
            const { openUrl } = await import('@tauri-apps/plugin-opener');
            await openUrl(url);
            console.log('[SubscriptionStore] Opened checkout successfully');
          } else {
            throw new Error('No checkout URL returned');
          }
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to open checkout';
          console.error('[SubscriptionStore] Failed to open checkout:', error);
          set({ error: message });
        }
      },

      openCustomerPortal: async () => {
        const { stripeCustomerId } = get();
        if (!stripeCustomerId) {
          console.warn('[SubscriptionStore] No Stripe customer ID');
          set({ error: 'No subscription found. Please subscribe first.' });
          return;
        }

        try {
          const convexUrl = import.meta.env.VITE_CONVEX_URL;
          if (!convexUrl) {
            throw new Error('Convex URL not configured');
          }

          // Get auth token for Convex HTTP endpoint
          if (!getAuthToken) {
            throw new Error('Auth not initialized');
          }
          const token = await getAuthToken();
          if (!token) {
            throw new Error('Please sign in to manage subscription');
          }

          // Call Convex HTTP endpoint to create Stripe portal session
          const portalEndpoint = convexUrl.replace('.convex.cloud', '.convex.site') + '/create-portal';

          const response = await fetch(portalEndpoint, {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              Authorization: `Bearer ${token}`,
            },
            body: JSON.stringify({ customerId: stripeCustomerId }),
          });

          if (!response.ok) {
            const error = await response.text();
            throw new Error(`Portal failed: ${error}`);
          }

          const { url } = await response.json();

          if (url) {
            // Open Stripe customer portal in default browser
            const { openUrl } = await import('@tauri-apps/plugin-opener');
            await openUrl(url);
            console.log('[SubscriptionStore] Opened customer portal');
          }
        } catch (error) {
          console.error('[SubscriptionStore] Failed to open portal:', error);
          set({ error: error instanceof Error ? error.message : 'Failed to open customer portal' });
        }
      },

      // ========================================
      // Internal
      // ========================================

      _setError: (error) => set({ error }),

      _reset: () => set({
        ...initialState,
        userId: get().userId, // Keep userId
      }),
    }),
    {
      name: 'sentinel-subscription',
      partialize: (state) => ({
        // Only persist user identification - NOT tier/status
        // Tier and status must always come fresh from Convex to prevent stale "free" state
        userId: state.userId,
        stripeCustomerId: state.stripeCustomerId,
        lastSyncedAt: state.lastSyncedAt,
      }),
    }
  )
);

// ============================================================================
// HELPERS
// ============================================================================

/**
 * Map chat model ID to subscription model type
 */
export function chatModelToSubscriptionModel(chatModel: string): ModelType {
  if (chatModel.includes('haiku')) return 'haiku';
  if (chatModel.includes('sonnet')) return 'sonnet';
  if (chatModel.includes('opus')) return 'opus';
  return 'haiku'; // Default
}

/**
 * Get display name for model
 */
export function getModelDisplayName(model: ModelType): string {
  switch (model) {
    case 'haiku':
      return 'Haiku 4.5';
    case 'sonnet':
      return 'Sonnet 4.5';
    case 'opus':
      return 'Opus 4.5';
  }
}

/**
 * Format tier for display
 */
export function formatTier(tier: SubscriptionTier): string {
  return tier === 'pro' ? 'Pro' : 'Free';
}
