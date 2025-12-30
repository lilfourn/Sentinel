import { useCallback, useEffect, useRef } from "react";
import {
  useSettingsStore,
  type Theme,
  type ViewMode,
  type SortBy,
  type SortDirection,
  type AIModel,
} from "../stores/settings-store";

// Check if Convex/Clerk are configured at module level
const isConvexConfigured = Boolean(import.meta.env.VITE_CONVEX_URL);
const isClerkConfigured = Boolean(import.meta.env.VITE_CLERK_PUBLISHABLE_KEY);

/**
 * Hook that provides settings with Convex sync capability
 *
 * When authenticated with Clerk and Convex is configured:
 * - Settings are loaded from Convex on mount
 * - Local changes are synced to Convex automatically
 * - Settings persist across devices
 */
export function useSyncedSettings() {
  const store = useSettingsStore();

  /**
   * Update settings (local-first with cloud sync)
   */
  const updateSettings = useCallback(
    async (
      updates: Partial<{
        theme: Theme;
        autoRenameEnabled: boolean;
        watchDownloads: boolean;
        showHiddenFiles: boolean;
        defaultView: ViewMode;
        sortBy: SortBy;
        sortDirection: SortDirection;
        aiModel: AIModel;
        skipDeleteConfirmation: boolean;
      }>
    ) => {
      // Update local store immediately
      if (updates.theme !== undefined) store.setTheme(updates.theme);
      if (updates.autoRenameEnabled !== undefined)
        store.setAutoRename(updates.autoRenameEnabled);
      if (updates.watchDownloads !== undefined)
        store.setWatchDownloads(updates.watchDownloads);
      if (updates.showHiddenFiles !== undefined)
        store.setShowHiddenFiles(updates.showHiddenFiles);
      if (updates.defaultView !== undefined)
        store.setDefaultView(updates.defaultView);
      if (updates.sortBy !== undefined) store.setSortBy(updates.sortBy);
      if (updates.sortDirection !== undefined)
        store.setSortDirection(updates.sortDirection);
      if (updates.aiModel !== undefined) store.setAIModel(updates.aiModel);
      if (updates.skipDeleteConfirmation !== undefined)
        store.setSkipDeleteConfirmation(updates.skipDeleteConfirmation);

      // Sync to Convex if configured
      if (isConvexConfigured && isClerkConfigured) {
        try {
          store.setSyncing(true);
          const { ConvexHttpClient } = await import("convex/browser");
          const { api } = await import("../../convex/_generated/api");

          // Use HTTP client for mutations (doesn't require hooks context)
          const client = new ConvexHttpClient(import.meta.env.VITE_CONVEX_URL);

          // Get auth token from Clerk (window.Clerk is set by ClerkProvider)
          const clerkWindow = window as unknown as { Clerk?: { session?: { getToken: (opts: { template: string }) => Promise<string | null> } } };
          if (clerkWindow.Clerk?.session) {
            const token = await clerkWindow.Clerk.session.getToken({ template: "convex" });
            if (token) {
              client.setAuth(token);
              await client.mutation(api.settings.updateSettings, updates);
            }
          }
        } catch (error) {
          console.error("Failed to sync settings to Convex:", error);
        } finally {
          store.setSyncing(false);
        }
      }
    },
    [store]
  );

  return {
    ...store,
    updateSettings,
    isAuthenticated: false, // Will be updated by Clerk in the component
    isConvexAvailable: isConvexConfigured,
    isClerkAvailable: isClerkConfigured,
  };
}

/**
 * Hook to sync settings from Convex on initial load
 * This should be called from a component that's inside ClerkProvider
 */
export function useConvexSettingsSync() {
  const store = useSettingsStore();
  const hasSynced = useRef(false);

  useEffect(() => {
    if (!isConvexConfigured || !isClerkConfigured || hasSynced.current) return;

    const syncFromConvex = async () => {
      try {
        const { ConvexHttpClient } = await import("convex/browser");
        const { api } = await import("../../convex/_generated/api");

        const client = new ConvexHttpClient(import.meta.env.VITE_CONVEX_URL);

        // Get auth token from Clerk
        const clerk = (window as unknown as { Clerk?: { session?: { getToken: (opts: { template: string }) => Promise<string | null> } } }).Clerk;
        if (clerk?.session) {
          const token = await clerk.session.getToken({ template: "convex" });
          if (token) {
            client.setAuth(token);
            const settings = await client.query(api.settings.getSettings);

            if (settings) {
              hasSynced.current = true;
              store.syncFromConvex({
                theme: settings.theme as Theme,
                autoRenameEnabled: settings.autoRenameEnabled,
                watchDownloads: settings.watchDownloads ?? false, // Default to false if undefined
                watchedFolders: settings.watchedFolders,
                showHiddenFiles: settings.showHiddenFiles,
                defaultView: settings.defaultView as ViewMode,
                sortBy: settings.sortBy as SortBy,
                sortDirection: settings.sortDirection as SortDirection,
                aiModel: settings.aiModel as AIModel,
              });
            }
          }
        }
      } catch (error) {
        console.error("Failed to sync settings from Convex:", error);
      }
    };

    // Wait a bit for Clerk to initialize
    const timer = setTimeout(syncFromConvex, 1000);
    return () => clearTimeout(timer);
  }, [store]);
}
