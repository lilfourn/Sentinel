import { useEffect, useRef, useCallback } from "react";
import { useMutation } from "convex/react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../../convex/_generated/api";
import { useSubscriptionStore } from "../stores/subscription-store";

type Model = "haiku" | "sonnet" | "gpt52" | "gpt5mini" | "gpt5nano";
type RequestType = "chat" | "organize" | "rename";

interface ChatCompletePayload {
  model: Model;
  extendedThinking: boolean;
}

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
 * Listens for Tauri events and persists to dailyUsage, organizeHistory, renameHistory tables.
 */
const CONVEX_URL = import.meta.env.VITE_CONVEX_URL?.replace(".cloud", ".site") || "";

export function UsageSync(): null {
  const recordUsage = useMutation(api.subscriptions.recordUsage);
  const recordOrganize = useMutation(api.history.recordOrganize);
  const recordRename = useMutation(api.history.recordRename);
  const userId = useSubscriptionStore((s) => s.userId);

  // Refs for stable access in callbacks
  const recordUsageRef = useRef(recordUsage);
  const recordOrganizeRef = useRef(recordOrganize);
  const recordRenameRef = useRef(recordRename);
  const userIdRef = useRef(userId);

  // Keep refs current
  recordUsageRef.current = recordUsage;
  recordOrganizeRef.current = recordOrganize;
  recordRenameRef.current = recordRename;
  userIdRef.current = userId;

  /**
   * Record usage with JWT auth, falling back to HTTP action with clerkId if auth fails
   */
  const recordUsageWithFallback = useCallback(async (
    model: Model,
    isExtendedThinking: boolean,
    requestType: RequestType
  ): Promise<{ method: string }> => {
    try {
      await recordUsageRef.current({ model, isExtendedThinking, requestType });
      return { method: "jwt" };
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      const isAuthError = errorMessage.includes("Not authenticated") || errorMessage.includes("Unauthenticated");

      if (!isAuthError) throw err;

      const clerkId = userIdRef.current;
      if (!clerkId?.startsWith("user_")) {
        console.error("[UsageSync] No valid clerkId for fallback:", clerkId);
        throw new Error("Not authenticated and no clerkId available");
      }

      console.log("[UsageSync] JWT auth failed, falling back to HTTP action");
      const response = await fetch(`${CONVEX_URL}/record-usage`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ clerkUserId: clerkId, model, isExtendedThinking, requestType }),
      });

      if (!response.ok) {
        const error = await response.json().catch(() => ({ error: "Unknown error" }));
        throw new Error(error.error || "Failed to record usage");
      }

      const result = await response.json();
      return { method: result.method || "http" };
    }
  }, []);

  const handleChatEvent = useCallback(async (event: { payload: ChatCompletePayload }) => {
    const { model, extendedThinking } = event.payload;
    try {
      const { method } = await recordUsageWithFallback(model, extendedThinking, "chat");
      const thinkingTag = extendedThinking ? "(extended thinking)" : "";
      console.log(`[UsageSync] Recorded chat usage: ${model} ${thinkingTag} (via ${method})`);
    } catch (err) {
      console.error("[UsageSync] Failed to record chat usage:", err);
    }
  }, [recordUsageWithFallback]);

  const handleOrganizeEvent = useCallback(async (event: { payload: OrganizeCompletePayload }) => {
    const { folderPath, folderName, operationCount, operations, summary } = event.payload;
    try {
      await recordOrganizeRef.current({ folderPath, folderName, operationCount, operations, summary });
      console.log(`[UsageSync] Recorded organize: ${folderName} (${operationCount} ops)`);
    } catch (err) {
      console.error("[UsageSync] Failed to record organize:", err);
    }
  }, []);

  const handleRenameEvent = useCallback(async (event: { payload: RenameCompletePayload }) => {
    const { originalName, newName, filePath, fileSize, mimeType, aiModel } = event.payload;
    try {
      await recordRenameRef.current({ originalName, newName, filePath, fileSize, mimeType, aiModel });
      const { method } = await recordUsageWithFallback("haiku", false, "rename");
      console.log(`[UsageSync] Recorded rename: ${originalName} -> ${newName} (via ${method})`);
    } catch (err) {
      console.error("[UsageSync] Failed to record rename:", err);
    }
  }, [recordUsageWithFallback]);

  useEffect(() => {
    let isMounted = true;
    const listeners: UnlistenFn[] = [];

    async function setupListeners(): Promise<void> {
      try {
        const [unlistenChat, unlistenOrganize, unlistenRename] = await Promise.all([
          listen<ChatCompletePayload>("usage:record-chat", handleChatEvent),
          listen<OrganizeCompletePayload>("usage:record-organize", handleOrganizeEvent),
          listen<RenameCompletePayload>("usage:record-rename", handleRenameEvent),
        ]);

        if (!isMounted) {
          unlistenChat();
          unlistenOrganize();
          unlistenRename();
          return;
        }

        listeners.push(unlistenChat, unlistenOrganize, unlistenRename);
        console.log("[UsageSync] Event listeners registered");
      } catch (err) {
        console.error("[UsageSync] Failed to setup listeners:", err);
      }
    }

    setupListeners();

    return () => {
      isMounted = false;
      listeners.forEach((unlisten) => unlisten());
    };
  }, [handleChatEvent, handleOrganizeEvent, handleRenameEvent]);

  return null;
}
