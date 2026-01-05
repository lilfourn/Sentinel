import { useEffect, useRef } from "react";
import { useMutation } from "convex/react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../../convex/_generated/api";

/**
 * Event payload for chat completion
 */
interface ChatCompletePayload {
  model: "haiku" | "sonnet" | "opus";
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
  const recordOrganize = useMutation(api.history.recordOrganize);
  const recordRename = useMutation(api.history.recordRename);
  const listenersRef = useRef<UnlistenFn[]>([]);

  useEffect(() => {
    const setupListeners = async () => {
      // Listen for chat completion events
      const unlistenChat = await listen<ChatCompletePayload>(
        "usage:record-chat",
        async (event) => {
          const { model, extendedThinking } = event.payload;
          try {
            await recordUsage({
              model,
              isExtendedThinking: extendedThinking,
              requestType: "chat",
            });
            console.log("[UsageSync] Recorded chat usage:", model, extendedThinking ? "(extended thinking)" : "");
          } catch (err) {
            console.error("[UsageSync] Failed to record chat usage:", err);
          }
        }
      );

      // Listen for organize completion events
      const unlistenOrganize = await listen<OrganizeCompletePayload>(
        "usage:record-organize",
        async (event) => {
          const { folderPath, folderName, operationCount, operations, summary } = event.payload;
          try {
            await recordOrganize({
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
        }
      );

      // Listen for rename completion events
      const unlistenRename = await listen<RenameCompletePayload>(
        "usage:record-rename",
        async (event) => {
          const { originalName, newName, filePath, fileSize, mimeType, aiModel } = event.payload;
          try {
            // Record the rename in history
            await recordRename({
              originalName,
              newName,
              filePath,
              fileSize,
              mimeType,
              aiModel,
            });
            // Also record usage for the rename API call
            await recordUsage({
              model: "haiku", // Auto-rename uses Haiku
              isExtendedThinking: false,
              requestType: "rename",
            });
            console.log("[UsageSync] Recorded rename:", originalName, "->", newName);
          } catch (err) {
            console.error("[UsageSync] Failed to record rename:", err);
          }
        }
      );

      listenersRef.current = [unlistenChat, unlistenOrganize, unlistenRename];
    };

    setupListeners();

    return () => {
      listenersRef.current.forEach((unlisten) => unlisten());
      listenersRef.current = [];
    };
  }, [recordUsage, recordOrganize, recordRename]);

  return null;
}
