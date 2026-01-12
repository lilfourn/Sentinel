import { useEffect, useRef, useCallback } from "react";
import { useMutation } from "convex/react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../../convex/_generated/api";
import { useSubscriptionStore } from "../stores/subscription-store";

/**
 * Event payload for chat completion
 */
interface ChatCompletePayload {
  model: "haiku" | "sonnet" | "gpt52" | "gpt5mini" | "gpt5nano";
  extendedThinking: boolean;
}

/**
 * Event payload for organize completion
 */
interface OrganizeCompletePayload {
  folderPath: string;
  folderName: string;
  operationCount: number;
  operations: Array<{
    type: "create_folder" | "move" | "rename" | "trash";
    sourcePath: string;
    destPath?: string;
  }>;
  summary: string;
}

/**
 * Event payload for rename completion
 */
interface RenameCompletePayload {
  originalName: string;
  newName: string;
  filePath: string;
  fileSize?: number;
  mimeType?: string;
  aiModel: string;
}

/**
 * Invisible component that syncs usage data to Convex.
 * Listens for Tauri events emitted by stores after operations complete,
 * then calls Convex mutations to persist the data.
 *
 * Tables populated:
 * - dailyUsage: Chat and rename API usage tracking
 * - organizeHistory: Completed organize operations
 * - renameHistory: Auto-rename operations
 * - usageStats: Monthly aggregated stats (incremented by recordOrganize/recordRename)
 */
export function UsageSync() {
  const recordUsage = useMutation(api.subscriptions.recordUsage);
  const recordUsageByClerkId = useMutation(api.subscriptions.recordUsageByClerkId);
  const recordOrganize = useMutation(api.history.recordOrganize);
  const recordRename = useMutation(api.history.recordRename);

  // Get user ID from subscription store (set by AuthSync/DesktopAuthSync)
  const userId = useSubscriptionStore((s) => s.userId);

  // Track component mount state and listener setup
  const isMountedRef = useRef(true);
  const listenersRef = useRef<UnlistenFn[]>([]);
  const isSettingUpRef = useRef(false);

  // Stable callback refs for mutations (prevents re-registration on mutation identity changes)
  const recordUsageRef = useRef(recordUsage);
  const recordUsageByClerkIdRef = useRef(recordUsageByClerkId);
  const recordOrganizeRef = useRef(recordOrganize);
  const recordRenameRef = useRef(recordRename);
  const userIdRef = useRef(userId);

  // Keep refs up to date
  useEffect(() => {
    recordUsageRef.current = recordUsage;
    recordUsageByClerkIdRef.current = recordUsageByClerkId;
    recordOrganizeRef.current = recordOrganize;
    recordRenameRef.current = recordRename;
    userIdRef.current = userId;
  }, [recordUsage, recordUsageByClerkId, recordOrganize, recordRename, userId]);

  /**
   * Helper to record usage with automatic fallback
   * Tries JWT-authenticated mutation first, falls back to clerkId-based mutation
   */
  const recordUsageWithFallback = useCallback(async (
    model: "haiku" | "sonnet" | "gpt52" | "gpt5mini" | "gpt5nano",
    isExtendedThinking: boolean,
    requestType: "chat" | "organize" | "rename"
  ) => {
    try {
      // Try JWT-authenticated mutation first
      await recordUsageRef.current({
        model,
        isExtendedThinking,
        requestType,
      });
      return { success: true, method: "jwt" };
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);

      // If auth failed, try fallback with clerk ID
      if (errorMessage.includes("Not authenticated") || errorMessage.includes("Unauthenticated")) {
        const clerkId = userIdRef.current;
        if (clerkId && clerkId.startsWith("user_")) {
          console.log("[UsageSync] JWT auth failed, falling back to clerkId:", clerkId);
          await recordUsageByClerkIdRef.current({
            clerkUserId: clerkId,
            model,
            isExtendedThinking,
            requestType,
          });
          return { success: true, method: "clerkId" };
        } else {
          console.error("[UsageSync] No valid clerkId available for fallback:", clerkId);
          throw new Error("Not authenticated and no clerkId available");
        }
      }

      // Re-throw other errors
      throw err;
    }
  }, []);

  // Stable handler for chat events
  const handleChatEvent = useCallback(async (event: { payload: ChatCompletePayload }) => {
    const { model, extendedThinking } = event.payload;
    try {
      const result = await recordUsageWithFallback(model, extendedThinking, "chat");
      console.log("[UsageSync] Recorded chat usage:", model, extendedThinking ? "(extended thinking)" : "", `(via ${result.method})`);
    } catch (err) {
      console.error("[UsageSync] Failed to record chat usage:", err);
    }
  }, [recordUsageWithFallback]);

  // Stable handler for organize events
  const handleOrganizeEvent = useCallback(async (event: { payload: OrganizeCompletePayload }) => {
    const { folderPath, folderName, operationCount, operations, summary } = event.payload;
    try {
      await recordOrganizeRef.current({
        folderPath,
        folderName,
        operationCount,
        operations,
        summary,
      });
      console.log("[UsageSync] Recorded organize history:", folderName, `(${operationCount} ops)`);
    } catch (err) {
      console.error("[UsageSync] Failed to record organize:", err);
    }
  }, []);

  // Stable handler for rename events
  const handleRenameEvent = useCallback(async (event: { payload: RenameCompletePayload }) => {
    const { originalName, newName, filePath, fileSize, mimeType, aiModel } = event.payload;
    try {
      // Record the rename in history
      await recordRenameRef.current({
        originalName,
        newName,
        filePath,
        fileSize,
        mimeType,
        aiModel,
      });
      // Also record usage for the rename API call (with fallback)
      const result = await recordUsageWithFallback("haiku", false, "rename");
      console.log("[UsageSync] Recorded rename:", originalName, "->", newName, `(via ${result.method})`);
    } catch (err) {
      console.error("[UsageSync] Failed to record rename:", err);
    }
  }, [recordUsageWithFallback]);

  useEffect(() => {
    // Reset mount state
    isMountedRef.current = true;

    const setupListeners = async () => {
      // Prevent duplicate setup
      if (isSettingUpRef.current) {
        return;
      }
      isSettingUpRef.current = true;

      try {
        // Set up all listeners
        const unlistenChat = await listen<ChatCompletePayload>(
          "usage:record-chat",
          handleChatEvent
        );

        // Check if still mounted before continuing
        if (!isMountedRef.current) {
          unlistenChat();
          isSettingUpRef.current = false;
          return;
        }

        const unlistenOrganize = await listen<OrganizeCompletePayload>(
          "usage:record-organize",
          handleOrganizeEvent
        );

        if (!isMountedRef.current) {
          unlistenChat();
          unlistenOrganize();
          isSettingUpRef.current = false;
          return;
        }

        const unlistenRename = await listen<RenameCompletePayload>(
          "usage:record-rename",
          handleRenameEvent
        );

        if (!isMountedRef.current) {
          unlistenChat();
          unlistenOrganize();
          unlistenRename();
          isSettingUpRef.current = false;
          return;
        }

        // Store listeners for cleanup
        listenersRef.current = [unlistenChat, unlistenOrganize, unlistenRename];
        console.log("[UsageSync] Event listeners registered");
      } catch (err) {
        console.error("[UsageSync] Failed to setup listeners:", err);
      } finally {
        isSettingUpRef.current = false;
      }
    };

    setupListeners();

    return () => {
      // Mark as unmounted to prevent listeners being stored after cleanup
      isMountedRef.current = false;

      // Clean up any registered listeners
      listenersRef.current.forEach((unlisten) => {
        try {
          unlisten();
        } catch (err) {
          console.error("[UsageSync] Error during listener cleanup:", err);
        }
      });
      listenersRef.current = [];
    };
  }, [handleChatEvent, handleOrganizeEvent, handleRenameEvent]); // Stable callbacks, won't cause re-registration

  return null;
}
