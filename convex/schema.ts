import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  // User profile - linked to Clerk auth provider
  users: defineTable({
    name: v.string(),
    email: v.string(),
    tokenIdentifier: v.string(), // From Clerk (e.g., "https://clerk.xxx|user_xxx")
    clerkId: v.optional(v.string()), // Just the Clerk user ID (e.g., "user_xxx") for direct lookup
    avatarUrl: v.optional(v.string()),
    createdAt: v.number(),
  })
    .index("by_token", ["tokenIdentifier"])
    .index("by_email", ["email"])
    .index("by_clerk_id", ["clerkId"]),

  // User settings - synced across devices
  userSettings: defineTable({
    userId: v.id("users"),
    // Appearance
    theme: v.union(v.literal("light"), v.literal("dark"), v.literal("system")),
    // Auto-rename sentinel
    autoRenameEnabled: v.boolean(),
    watchDownloads: v.optional(v.boolean()), // Whether to watch Downloads folder for auto-rename (optional for migration)
    watchedFolders: v.array(v.string()), // Additional paths to watch for auto-rename
    // File browser preferences
    showHiddenFiles: v.boolean(),
    defaultView: v.union(
      v.literal("list"),
      v.literal("grid"),
      v.literal("columns")
    ),
    sortBy: v.union(
      v.literal("name"),
      v.literal("date"),
      v.literal("size"),
      v.literal("type")
    ),
    sortDirection: v.union(v.literal("asc"), v.literal("desc")),
    // AI preferences
    aiModel: v.union(v.literal("haiku"), v.literal("sonnet")),
  }).index("by_user", ["userId"]),

  // Organization history - track AI organize operations
  organizeHistory: defineTable({
    userId: v.id("users"),
    folderPath: v.string(),
    folderName: v.string(),
    operationCount: v.number(),
    operations: v.array(
      v.object({
        type: v.union(
          v.literal("create_folder"),
          v.literal("move"),
          v.literal("rename"),
          v.literal("trash")
        ),
        sourcePath: v.string(),
        destPath: v.optional(v.string()),
      })
    ),
    completedAt: v.number(),
    summary: v.string(),
    wasUndone: v.boolean(),
  })
    .index("by_user", ["userId"])
    .index("by_user_date", ["userId", "completedAt"]),

  // Rename history - track auto-rename operations
  renameHistory: defineTable({
    userId: v.id("users"),
    originalName: v.string(),
    newName: v.string(),
    filePath: v.string(),
    fileSize: v.optional(v.number()),
    mimeType: v.optional(v.string()),
    renamedAt: v.number(),
    wasUndone: v.boolean(),
    aiModel: v.string(), // Which model suggested the rename
  })
    .index("by_user", ["userId"])
    .index("by_user_date", ["userId", "renamedAt"]),

  // Usage analytics - track API usage for billing awareness
  usageStats: defineTable({
    userId: v.id("users"),
    month: v.string(), // "2025-01" format
    organizeCount: v.number(),
    renameCount: v.number(),
    tokensUsed: v.number(),
  })
    .index("by_user", ["userId"])
    .index("by_user_month", ["userId", "month"]),

  // Subscription management - links Clerk user to Stripe billing
  subscriptions: defineTable({
    userId: v.id("users"),
    // Stripe identifiers
    stripeCustomerId: v.string(),
    stripeSubscriptionId: v.optional(v.string()), // null for free tier
    // Subscription state
    tier: v.union(v.literal("free"), v.literal("pro")),
    status: v.union(
      v.literal("active"),
      v.literal("past_due"),
      v.literal("canceled"),
      v.literal("incomplete"),
      v.literal("trialing")
    ),
    // Billing period
    currentPeriodStart: v.optional(v.number()), // Unix timestamp (ms)
    currentPeriodEnd: v.optional(v.number()), // Unix timestamp (ms)
    cancelAtPeriodEnd: v.boolean(),
    // Timestamps
    createdAt: v.number(),
    updatedAt: v.number(),
  })
    .index("by_user", ["userId"])
    .index("by_stripe_customer", ["stripeCustomerId"])
    .index("by_stripe_subscription", ["stripeSubscriptionId"]),

  // Daily usage tracking per model - resets at midnight UTC
  dailyUsage: defineTable({
    userId: v.id("users"),
    date: v.string(), // "2025-01-15" format (UTC)
    // Per-model request counts (Claude)
    haikuRequests: v.number(),
    sonnetRequests: v.number(),
    opusRequests: v.number(),
    extendedThinkingRequests: v.number(),
    // Feature usage counts
    organizeRequests: v.number(),
    renameRequests: v.number(),
    // Per-model request counts (OpenAI GPT) - optional for migration
    gpt52Requests: v.optional(v.number()),
    gpt5miniRequests: v.optional(v.number()),
    gpt5nanoRequests: v.optional(v.number()),
    // Last updated timestamp
    updatedAt: v.number(),
  })
    .index("by_user_date", ["userId", "date"])
    .index("by_date", ["date"]), // For cleanup jobs

  // Stripe webhook event log - for idempotency and debugging
  stripeWebhookEvents: defineTable({
    eventId: v.string(), // Stripe event ID
    eventType: v.string(),
    processedAt: v.number(),
    payload: v.optional(v.string()), // JSON string of relevant data
    status: v.union(v.literal("processed"), v.literal("failed")),
    errorMessage: v.optional(v.string()),
  })
    .index("by_event_id", ["eventId"])
    .index("by_processed_at", ["processedAt"]), // For cleanup
});
