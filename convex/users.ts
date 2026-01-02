import { v } from "convex/values";
import { mutation, query } from "./_generated/server";

/**
 * Get or create user from Clerk identity
 * Called on first login to ensure user exists in our database
 */
export const getOrCreateUser = mutation({
  args: {},
  handler: async (ctx) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      throw new Error("Not authenticated");
    }

    // Check if user already exists
    const existingUser = await ctx.db
      .query("users")
      .withIndex("by_token", (q) => q.eq("tokenIdentifier", identity.tokenIdentifier))
      .unique();

    if (existingUser) {
      // Update name/email if changed
      if (
        existingUser.name !== identity.name ||
        existingUser.email !== identity.email
      ) {
        await ctx.db.patch(existingUser._id, {
          name: identity.name ?? existingUser.name,
          email: identity.email ?? existingUser.email,
          avatarUrl: identity.pictureUrl,
        });
      }
      return existingUser._id;
    }

    // Create new user
    const userId = await ctx.db.insert("users", {
      name: identity.name ?? "User",
      email: identity.email ?? "",
      tokenIdentifier: identity.tokenIdentifier,
      avatarUrl: identity.pictureUrl,
      createdAt: Date.now(),
    });

    // Create default settings for new user
    await ctx.db.insert("userSettings", {
      userId,
      theme: "system",
      autoRenameEnabled: false,
      watchDownloads: false,
      watchedFolders: [],
      showHiddenFiles: false,
      defaultView: "list",
      sortBy: "name",
      sortDirection: "asc",
      aiModel: "sonnet",
    });

    return userId;
  },
});

/**
 * Get current user profile
 */
export const getCurrentUser = query({
  args: {},
  handler: async (ctx) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      return null;
    }

    return await ctx.db
      .query("users")
      .withIndex("by_token", (q) => q.eq("tokenIdentifier", identity.tokenIdentifier))
      .unique();
  },
});

/**
 * Get user by ID
 */
export const getUser = query({
  args: { userId: v.id("users") },
  handler: async (ctx, args) => {
    return await ctx.db.get(args.userId);
  },
});

/**
 * Update user profile
 */
export const updateProfile = mutation({
  args: {
    name: v.optional(v.string()),
    avatarUrl: v.optional(v.string()),
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

    const updates: Partial<{ name: string; avatarUrl: string }> = {};
    if (args.name !== undefined) updates.name = args.name;
    if (args.avatarUrl !== undefined) updates.avatarUrl = args.avatarUrl;

    await ctx.db.patch(user._id, updates);
    return user._id;
  },
});

/**
 * Delete user account and all associated data
 */
export const deleteAccount = mutation({
  args: {},
  handler: async (ctx) => {
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

    // Delete all associated data
    const settings = await ctx.db
      .query("userSettings")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .collect();
    for (const setting of settings) {
      await ctx.db.delete(setting._id);
    }

    const organizeHistory = await ctx.db
      .query("organizeHistory")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .collect();
    for (const history of organizeHistory) {
      await ctx.db.delete(history._id);
    }

    const renameHistory = await ctx.db
      .query("renameHistory")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .collect();
    for (const history of renameHistory) {
      await ctx.db.delete(history._id);
    }

    const usageStats = await ctx.db
      .query("usageStats")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .collect();
    for (const stat of usageStats) {
      await ctx.db.delete(stat._id);
    }

    // Delete subscription data
    const subscriptions = await ctx.db
      .query("subscriptions")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .collect();
    for (const sub of subscriptions) {
      await ctx.db.delete(sub._id);
    }

    // Delete daily usage records
    const dailyUsage = await ctx.db
      .query("dailyUsage")
      .withIndex("by_user_date", (q) => q.eq("userId", user._id))
      .collect();
    for (const usage of dailyUsage) {
      await ctx.db.delete(usage._id);
    }

    // Delete user
    await ctx.db.delete(user._id);
    return true;
  },
});
