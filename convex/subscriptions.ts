import { v } from "convex/values";
import { mutation, query, internalMutation, internalQuery } from "./_generated/server";

// =============================================================================
// TIER CONFIGURATION
// =============================================================================

export const TIER_LIMITS = {
  free: {
    haiku: 100,
    sonnet: 0,
    opus: 0,
    extendedThinking: 0,
    // GPT models
    gpt52: 0,        // Pro only
    gpt5mini: 50,    // Mid-tier GPT
    gpt5nano: 100,   // Budget GPT
  },
  pro: {
    haiku: 300,
    sonnet: 50,
    opus: 10,
    extendedThinking: 5,
    // GPT models
    gpt52: 30,       // Most capable GPT
    gpt5mini: 200,   // Mid-tier GPT
    gpt5nano: 500,   // Budget GPT
  },
} as const;

export type Tier = keyof typeof TIER_LIMITS;
export type ModelType = "haiku" | "sonnet" | "opus" | "extendedThinking" | "gpt52" | "gpt5mini" | "gpt5nano";

// Helper: Get UTC date string
function getUTCDate(): string {
  const now = new Date();
  return now.toISOString().split("T")[0]; // "2025-01-15"
}

// =============================================================================
// QUERIES
// =============================================================================

/**
 * Get current user's subscription (or default free tier)
 */
export const getSubscription = query({
  args: {},
  handler: async (ctx) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) return null;

    const user = await ctx.db
      .query("users")
      .withIndex("by_token", (q) => q.eq("tokenIdentifier", identity.tokenIdentifier))
      .unique();

    if (!user) return null;

    const subscription = await ctx.db
      .query("subscriptions")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .unique();

    // Return default free tier if no subscription exists
    if (!subscription) {
      return {
        tier: "free" as const,
        status: "active" as const,
        limits: TIER_LIMITS.free,
        cancelAtPeriodEnd: false,
        currentPeriodEnd: null,
        stripeCustomerId: null,
      };
    }

    return {
      ...subscription,
      limits: TIER_LIMITS[subscription.tier],
    };
  },
});

/**
 * Get daily usage for current user
 */
export const getDailyUsage = query({
  args: {},
  handler: async (ctx) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) return null;

    const user = await ctx.db
      .query("users")
      .withIndex("by_token", (q) => q.eq("tokenIdentifier", identity.tokenIdentifier))
      .unique();

    if (!user) return null;

    const today = getUTCDate();
    const usage = await ctx.db
      .query("dailyUsage")
      .withIndex("by_user_date", (q) => q.eq("userId", user._id).eq("date", today))
      .unique();

    // Return zero usage if no record exists
    if (!usage) {
      return {
        date: today,
        haikuRequests: 0,
        sonnetRequests: 0,
        opusRequests: 0,
        extendedThinkingRequests: 0,
        organizeRequests: 0,
        renameRequests: 0,
        // GPT models
        gpt52Requests: 0,
        gpt5miniRequests: 0,
        gpt5nanoRequests: 0,
      };
    }

    return usage;
  },
});

/**
 * Check if user can make a request for a specific model
 * Returns { allowed: boolean, remaining: number, limit: number, tier: string }
 */
export const checkLimit = query({
  args: {
    model: v.union(
      v.literal("haiku"),
      v.literal("sonnet"),
      v.literal("opus"),
      v.literal("extendedThinking"),
      v.literal("gpt52"),
      v.literal("gpt5mini"),
      v.literal("gpt5nano")
    ),
  },
  handler: async (ctx, args) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      return { allowed: false, remaining: 0, limit: 0, reason: "Not authenticated" };
    }

    const user = await ctx.db
      .query("users")
      .withIndex("by_token", (q) => q.eq("tokenIdentifier", identity.tokenIdentifier))
      .unique();

    if (!user) {
      return { allowed: false, remaining: 0, limit: 0, reason: "User not found" };
    }

    // Get subscription (default to free)
    const subscription = await ctx.db
      .query("subscriptions")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .unique();

    const tier = subscription?.tier ?? "free";
    const status = subscription?.status ?? "active";

    // Check subscription status
    if (status !== "active" && status !== "trialing") {
      return { allowed: false, remaining: 0, limit: 0, reason: "Subscription inactive", tier };
    }

    const limits = TIER_LIMITS[tier];
    const limit = limits[args.model];

    // Model not available on this tier
    if (limit === 0) {
      return {
        allowed: false,
        remaining: 0,
        limit: 0,
        reason: `${args.model} requires Pro subscription`,
        tier,
      };
    }

    // Get today's usage
    const today = getUTCDate();
    const usage = await ctx.db
      .query("dailyUsage")
      .withIndex("by_user_date", (q) => q.eq("userId", user._id).eq("date", today))
      .unique();

    const modelKeyMap = {
      haiku: "haikuRequests",
      sonnet: "sonnetRequests",
      opus: "opusRequests",
      extendedThinking: "extendedThinkingRequests",
      gpt52: "gpt52Requests",
      gpt5mini: "gpt5miniRequests",
      gpt5nano: "gpt5nanoRequests",
    } as const;

    const modelKey = modelKeyMap[args.model];
    const used = (usage?.[modelKey] as number | undefined) ?? 0;
    const remaining = Math.max(0, limit - used);

    return {
      allowed: remaining > 0,
      remaining,
      limit,
      used,
      tier,
    };
  },
});

/**
 * Get usage history for the current month
 */
export const getUsageHistory = query({
  args: {},
  handler: async (ctx) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) return [];

    const user = await ctx.db
      .query("users")
      .withIndex("by_token", (q) => q.eq("tokenIdentifier", identity.tokenIdentifier))
      .unique();

    if (!user) return [];

    // Get first day of current month
    const now = new Date();
    const monthStart = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-01`;

    const usageRecords = await ctx.db
      .query("dailyUsage")
      .withIndex("by_user_date", (q) => q.eq("userId", user._id).gte("date", monthStart))
      .collect();

    return usageRecords;
  },
});

// =============================================================================
// MUTATIONS
// =============================================================================

/**
 * Record API usage (called after successful API call)
 * Uses JWT auth - may fail in desktop mode when tokens expire
 */
export const recordUsage = mutation({
  args: {
    model: v.union(
      v.literal("haiku"),
      v.literal("sonnet"),
      v.literal("opus"),
      v.literal("gpt52"),
      v.literal("gpt5mini"),
      v.literal("gpt5nano")
    ),
    isExtendedThinking: v.optional(v.boolean()),
    requestType: v.optional(
      v.union(v.literal("chat"), v.literal("organize"), v.literal("rename"))
    ),
  },
  handler: async (ctx, args) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      throw new Error("Not authenticated");
    }

    const user = await ctx.db
      .query("users")
      .withIndex("by_token", (q) => q.eq("tokenIdentifier", identity.tokenIdentifier))
      .unique();

    if (!user) {
      throw new Error("User not found");
    }

    return await recordUsageForUser(ctx, user._id, args.model, args.isExtendedThinking, args.requestType);
  },
});

/**
 * Record API usage by Clerk user ID (fallback for desktop auth when JWT expires)
 * This is used when the JWT auth fails but we still need to track usage
 */
export const recordUsageByClerkId = mutation({
  args: {
    clerkUserId: v.string(),
    model: v.union(
      v.literal("haiku"),
      v.literal("sonnet"),
      v.literal("opus"),
      v.literal("gpt52"),
      v.literal("gpt5mini"),
      v.literal("gpt5nano")
    ),
    isExtendedThinking: v.optional(v.boolean()),
    requestType: v.optional(
      v.union(v.literal("chat"), v.literal("organize"), v.literal("rename"))
    ),
  },
  handler: async (ctx, args) => {
    // Validate clerk user ID format (basic sanity check)
    if (!args.clerkUserId || !args.clerkUserId.startsWith("user_")) {
      throw new Error("Invalid Clerk user ID format");
    }

    // Try to find user by clerkId index first (fast path)
    let user = await ctx.db
      .query("users")
      .withIndex("by_clerk_id", (q) => q.eq("clerkId", args.clerkUserId))
      .first();

    // Fallback: try finding by tokenIdentifier suffix (for users created before clerkId field)
    if (!user) {
      const allUsers = await ctx.db.query("users").collect();
      const matchedUser = allUsers.find(u =>
        u.tokenIdentifier?.endsWith(`|${args.clerkUserId}`)
      );
      if (matchedUser) {
        // Backfill the clerkId for future lookups
        await ctx.db.patch(matchedUser._id, { clerkId: args.clerkUserId });
        console.log(`[recordUsageByClerkId] Backfilled clerkId for user ${matchedUser._id}`);
        user = matchedUser;
      }
    }

    if (!user) {
      console.error(`[recordUsageByClerkId] User not found for clerkId: ${args.clerkUserId}`);
      throw new Error("User not found");
    }

    return await recordUsageForUser(ctx, user._id, args.model, args.isExtendedThinking, args.requestType);
  },
});

/**
 * Internal helper to record usage for a user
 */
async function recordUsageForUser(
  ctx: { db: any },
  userId: any,
  model: "haiku" | "sonnet" | "opus" | "gpt52" | "gpt5mini" | "gpt5nano",
  isExtendedThinking?: boolean,
  requestType?: "chat" | "organize" | "rename"
) {
  const today = getUTCDate();
  const now = Date.now();

  // Get or create daily usage record
  let usage = await ctx.db
    .query("dailyUsage")
    .withIndex("by_user_date", (q: any) => q.eq("userId", userId).eq("date", today))
    .unique();

  if (!usage) {
    // Create new daily record
    const usageId = await ctx.db.insert("dailyUsage", {
      userId: userId,
      date: today,
      haikuRequests: 0,
      sonnetRequests: 0,
      opusRequests: 0,
      extendedThinkingRequests: 0,
      organizeRequests: 0,
      renameRequests: 0,
      // GPT models
      gpt52Requests: 0,
      gpt5miniRequests: 0,
      gpt5nanoRequests: 0,
      updatedAt: now,
    });
    usage = await ctx.db.get(usageId);
  }

  if (!usage) throw new Error("Failed to get usage record");

  // Build update object
  const updates: Record<string, number> = { updatedAt: now };

  // Increment model counter
  const modelKeyMap = {
    haiku: "haikuRequests",
    sonnet: "sonnetRequests",
    opus: "opusRequests",
    gpt52: "gpt52Requests",
    gpt5mini: "gpt5miniRequests",
    gpt5nano: "gpt5nanoRequests",
  } as const;
  const modelKey = modelKeyMap[model];
  updates[modelKey] = ((usage[modelKey] as number) ?? 0) + 1;

  // Increment extended thinking if applicable
  if (isExtendedThinking) {
    updates.extendedThinkingRequests = usage.extendedThinkingRequests + 1;
  }

  // Increment request type counter
  if (requestType === "organize") {
    updates.organizeRequests = usage.organizeRequests + 1;
  } else if (requestType === "rename") {
    updates.renameRequests = usage.renameRequests + 1;
  }

  await ctx.db.patch(usage._id, updates);
  console.log(`[recordUsage] Recorded ${model} usage for user ${userId}`);
  return { success: true };
}

/**
 * Link Stripe customer to user (called during checkout setup)
 */
export const linkStripeCustomer = mutation({
  args: {
    stripeCustomerId: v.string(),
  },
  handler: async (ctx, args) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      throw new Error("Not authenticated");
    }

    const user = await ctx.db
      .query("users")
      .withIndex("by_token", (q) => q.eq("tokenIdentifier", identity.tokenIdentifier))
      .unique();

    if (!user) {
      throw new Error("User not found");
    }

    // Check if subscription already exists
    const existing = await ctx.db
      .query("subscriptions")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .unique();

    const now = Date.now();

    if (existing) {
      // Update existing with Stripe customer ID
      await ctx.db.patch(existing._id, {
        stripeCustomerId: args.stripeCustomerId,
        updatedAt: now,
      });
      return existing._id;
    }

    // Create new subscription record (free tier until webhook confirms payment)
    const subscriptionId = await ctx.db.insert("subscriptions", {
      userId: user._id,
      stripeCustomerId: args.stripeCustomerId,
      stripeSubscriptionId: undefined,
      tier: "free",
      status: "active",
      cancelAtPeriodEnd: false,
      createdAt: now,
      updatedAt: now,
    });

    return subscriptionId;
  },
});

// =============================================================================
// INTERNAL MUTATIONS (for webhook handlers)
// =============================================================================

/**
 * Create or update subscription from checkout.session.completed
 * Uses the Clerk tokenIdentifier to find the user
 */
export const createOrUpdateFromCheckout = internalMutation({
  args: {
    tokenIdentifier: v.string(),
    stripeCustomerId: v.string(),
    stripeSubscriptionId: v.string(),
  },
  handler: async (ctx, args) => {
    // Find user by Clerk token identifier
    const user = await ctx.db
      .query("users")
      .withIndex("by_token", (q) => q.eq("tokenIdentifier", args.tokenIdentifier))
      .unique();

    if (!user) {
      console.error(`No user found for tokenIdentifier: ${args.tokenIdentifier}`);
      return { success: false, reason: "User not found" };
    }

    // Check if subscription already exists for this user
    const existingSubscription = await ctx.db
      .query("subscriptions")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .unique();

    const now = Date.now();

    if (existingSubscription) {
      // Update existing subscription
      await ctx.db.patch(existingSubscription._id, {
        stripeCustomerId: args.stripeCustomerId,
        stripeSubscriptionId: args.stripeSubscriptionId,
        tier: "pro",
        status: "active",
        cancelAtPeriodEnd: false,
        updatedAt: now,
      });
      console.log(`Updated subscription for user ${user._id} to Pro`);
    } else {
      // Create new subscription
      await ctx.db.insert("subscriptions", {
        userId: user._id,
        stripeCustomerId: args.stripeCustomerId,
        stripeSubscriptionId: args.stripeSubscriptionId,
        tier: "pro",
        status: "active",
        cancelAtPeriodEnd: false,
        createdAt: now,
        updatedAt: now,
      });
      console.log(`Created Pro subscription for user ${user._id}`);
    }

    return { success: true };
  },
});

/**
 * Update subscription from Stripe webhook
 * Only callable from HTTP actions (not from client)
 */
export const updateFromWebhook = internalMutation({
  args: {
    stripeCustomerId: v.string(),
    stripeSubscriptionId: v.optional(v.string()),
    tier: v.union(v.literal("free"), v.literal("pro")),
    status: v.union(
      v.literal("active"),
      v.literal("past_due"),
      v.literal("canceled"),
      v.literal("incomplete"),
      v.literal("trialing")
    ),
    currentPeriodStart: v.optional(v.number()),
    currentPeriodEnd: v.optional(v.number()),
    cancelAtPeriodEnd: v.boolean(),
  },
  handler: async (ctx, args) => {
    const subscription = await ctx.db
      .query("subscriptions")
      .withIndex("by_stripe_customer", (q) =>
        q.eq("stripeCustomerId", args.stripeCustomerId)
      )
      .unique();

    if (!subscription) {
      console.error(`No subscription found for Stripe customer: ${args.stripeCustomerId}`);
      return { success: false, reason: "Subscription not found" };
    }

    await ctx.db.patch(subscription._id, {
      stripeSubscriptionId: args.stripeSubscriptionId,
      tier: args.tier,
      status: args.status,
      currentPeriodStart: args.currentPeriodStart,
      currentPeriodEnd: args.currentPeriodEnd,
      cancelAtPeriodEnd: args.cancelAtPeriodEnd,
      updatedAt: Date.now(),
    });

    return { success: true };
  },
});

/**
 * Log webhook event for idempotency
 */
export const logWebhookEvent = internalMutation({
  args: {
    eventId: v.string(),
    eventType: v.string(),
    payload: v.optional(v.string()),
    status: v.union(v.literal("processed"), v.literal("failed")),
    errorMessage: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    // Check if already processed (idempotency)
    const existing = await ctx.db
      .query("stripeWebhookEvents")
      .withIndex("by_event_id", (q) => q.eq("eventId", args.eventId))
      .unique();

    if (existing) {
      return { alreadyProcessed: true };
    }

    await ctx.db.insert("stripeWebhookEvents", {
      eventId: args.eventId,
      eventType: args.eventType,
      processedAt: Date.now(),
      payload: args.payload,
      status: args.status,
      errorMessage: args.errorMessage,
    });

    return { alreadyProcessed: false };
  },
});

/**
 * Get subscription by Stripe customer ID (internal)
 */
export const getByStripeCustomer = internalQuery({
  args: { stripeCustomerId: v.string() },
  handler: async (ctx, args) => {
    return await ctx.db
      .query("subscriptions")
      .withIndex("by_stripe_customer", (q) =>
        q.eq("stripeCustomerId", args.stripeCustomerId)
      )
      .unique();
  },
});

/**
 * Get subscription by Clerk token identifier (internal)
 * Used for verifying customer ownership in billing portal
 */
export const getByTokenIdentifier = internalQuery({
  args: { tokenIdentifier: v.string() },
  handler: async (ctx, args) => {
    const user = await ctx.db
      .query("users")
      .withIndex("by_token", (q) => q.eq("tokenIdentifier", args.tokenIdentifier))
      .first();

    if (!user) return null;

    return await ctx.db
      .query("subscriptions")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .first();
  },
});

/**
 * Cleanup old webhook events (run periodically)
 */
export const cleanupOldWebhookEvents = internalMutation({
  args: {},
  handler: async (ctx) => {
    // Delete events older than 30 days
    const thirtyDaysAgo = Date.now() - 30 * 24 * 60 * 60 * 1000;

    const oldEvents = await ctx.db
      .query("stripeWebhookEvents")
      .withIndex("by_processed_at", (q) => q.lt("processedAt", thirtyDaysAgo))
      .take(100); // Batch delete

    for (const event of oldEvents) {
      await ctx.db.delete(event._id);
    }

    return { deleted: oldEvents.length };
  },
});

/**
 * Cleanup old daily usage records (run periodically)
 */
export const cleanupOldUsageRecords = internalMutation({
  args: {},
  handler: async (ctx) => {
    // Delete usage records older than 90 days
    const ninetyDaysAgo = new Date(Date.now() - 90 * 24 * 60 * 60 * 1000);
    const cutoffDate = ninetyDaysAgo.toISOString().split("T")[0];

    const oldRecords = await ctx.db
      .query("dailyUsage")
      .withIndex("by_date", (q) => q.lt("date", cutoffDate))
      .take(100); // Batch delete

    for (const record of oldRecords) {
      await ctx.db.delete(record._id);
    }

    return { deleted: oldRecords.length };
  },
});

/**
 * Reset all usage for a user by email (admin function)
 * Deletes all dailyUsage records for the user
 */
export const resetUserUsageByEmail = internalMutation({
  args: {
    email: v.string(),
  },
  handler: async (ctx, args) => {
    // Find user by email
    const user = await ctx.db
      .query("users")
      .withIndex("by_email", (q) => q.eq("email", args.email))
      .unique();

    if (!user) {
      return { success: false, reason: `User not found: ${args.email}` };
    }

    // Get all daily usage records for this user
    const usageRecords = await ctx.db
      .query("dailyUsage")
      .withIndex("by_user_date", (q) => q.eq("userId", user._id))
      .collect();

    // Delete all usage records
    for (const record of usageRecords) {
      await ctx.db.delete(record._id);
    }

    console.log(`Reset usage for ${args.email}: deleted ${usageRecords.length} records`);
    return { success: true, deleted: usageRecords.length, userId: user._id };
  },
});

/**
 * Get user info by email (admin helper)
 */
export const getUserInfoByEmail = internalQuery({
  args: {
    email: v.string(),
  },
  handler: async (ctx, args) => {
    const user = await ctx.db
      .query("users")
      .withIndex("by_email", (q) => q.eq("email", args.email))
      .unique();

    if (!user) {
      return { success: false, reason: `User not found: ${args.email}` };
    }

    // Extract clerkId from tokenIdentifier if not already set
    let clerkId = user.clerkId;
    if (!clerkId && user.tokenIdentifier) {
      const parts = user.tokenIdentifier.split("|");
      const lastPart = parts[parts.length - 1];
      if (lastPart?.startsWith("user_")) {
        clerkId = lastPart;
      }
    }

    return {
      success: true,
      userId: user._id,
      email: user.email,
      name: user.name,
      tokenIdentifier: user.tokenIdentifier,
      clerkId: clerkId ?? null,
      hasClerkIdField: !!user.clerkId,
    };
  },
});

/**
 * Backfill clerkId for a user by email (admin helper)
 */
export const backfillClerkIdByEmail = internalMutation({
  args: {
    email: v.string(),
  },
  handler: async (ctx, args) => {
    const user = await ctx.db
      .query("users")
      .withIndex("by_email", (q) => q.eq("email", args.email))
      .unique();

    if (!user) {
      return { success: false, reason: `User not found: ${args.email}` };
    }

    if (user.clerkId) {
      return { success: true, alreadySet: true, clerkId: user.clerkId };
    }

    // Extract clerkId from tokenIdentifier
    if (!user.tokenIdentifier) {
      return { success: false, reason: "User has no tokenIdentifier" };
    }

    const parts = user.tokenIdentifier.split("|");
    const clerkId = parts[parts.length - 1];

    if (!clerkId?.startsWith("user_")) {
      return { success: false, reason: `Could not extract clerkId from tokenIdentifier: ${user.tokenIdentifier}` };
    }

    await ctx.db.patch(user._id, { clerkId });
    console.log(`Backfilled clerkId for ${args.email}: ${clerkId}`);
    return { success: true, clerkId, backfilled: true };
  },
});
