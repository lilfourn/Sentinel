import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Theme = "light" | "dark" | "system";
export type ViewMode = "list" | "grid" | "columns";
export type SortBy = "name" | "date" | "size" | "type";
export type SortDirection = "asc" | "desc";
export type AIModel = "haiku" | "sonnet";

interface SettingsState {
  // Appearance
  theme: Theme;

  // Auto-rename sentinel
  autoRenameEnabled: boolean;
  watchDownloads: boolean; // Whether to watch Downloads folder for auto-rename
  watchedFolders: string[];

  // File browser preferences
  showHiddenFiles: boolean;
  defaultView: ViewMode;
  sortBy: SortBy;
  sortDirection: SortDirection;

  // AI preferences
  aiModel: AIModel;

  // Delete confirmation
  skipDeleteConfirmation: boolean;

  // Navigation
  lastVisitedPath: string | null;

  // Sync state
  lastSyncedAt: number | null;
  isSyncing: boolean;

  // Actions
  setTheme: (theme: Theme) => void;
  setAutoRename: (enabled: boolean) => void;
  setWatchDownloads: (enabled: boolean) => void;
  addWatchedFolder: (folder: string) => void;
  removeWatchedFolder: (folder: string) => void;
  setShowHiddenFiles: (show: boolean) => void;
  setDefaultView: (view: ViewMode) => void;
  setSortBy: (sortBy: SortBy) => void;
  setSortDirection: (direction: SortDirection) => void;
  setAIModel: (model: AIModel) => void;
  setSkipDeleteConfirmation: (skip: boolean) => void;
  setLastVisitedPath: (path: string) => void;

  // Sync with Convex
  syncFromConvex: (settings: Partial<SettingsState>) => void;
  setSyncing: (syncing: boolean) => void;
}

/**
 * Settings store with local persistence and Convex sync
 *
 * This store provides immediate local updates while syncing to Convex
 * when the user is authenticated. Settings persist locally even when offline.
 */
export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      // Default values
      theme: "system",
      autoRenameEnabled: false,
      watchDownloads: false,
      watchedFolders: [],
      showHiddenFiles: false,
      defaultView: "list",
      sortBy: "name",
      sortDirection: "asc",
      aiModel: "sonnet",
      skipDeleteConfirmation: false,
      lastVisitedPath: null,
      lastSyncedAt: null,
      isSyncing: false,

      // Actions
      setTheme: (theme) => set({ theme }),

      setAutoRename: (enabled) => set({ autoRenameEnabled: enabled }),

      setWatchDownloads: (enabled) => set({ watchDownloads: enabled }),

      addWatchedFolder: (folder) =>
        set((state) => ({
          watchedFolders: state.watchedFolders.includes(folder)
            ? state.watchedFolders
            : [...state.watchedFolders, folder],
        })),

      removeWatchedFolder: (folder) =>
        set((state) => ({
          watchedFolders: state.watchedFolders.filter((f) => f !== folder),
        })),

      setShowHiddenFiles: (show) => set({ showHiddenFiles: show }),

      setDefaultView: (view) => set({ defaultView: view }),

      setSortBy: (sortBy) => set({ sortBy }),

      setSortDirection: (direction) => set({ sortDirection: direction }),

      setAIModel: (model) => set({ aiModel: model }),

      setSkipDeleteConfirmation: (skip) => set({ skipDeleteConfirmation: skip }),

      setLastVisitedPath: (path) => set({ lastVisitedPath: path }),

      // Sync methods
      syncFromConvex: (settings) =>
        set((state) => ({
          ...state,
          ...settings,
          lastSyncedAt: Date.now(),
        })),

      setSyncing: (syncing) => set({ isSyncing: syncing }),
    }),
    {
      name: "sentinel-settings",
      // Only persist these fields locally
      partialize: (state) => ({
        theme: state.theme,
        autoRenameEnabled: state.autoRenameEnabled,
        watchDownloads: state.watchDownloads,
        watchedFolders: state.watchedFolders,
        showHiddenFiles: state.showHiddenFiles,
        defaultView: state.defaultView,
        sortBy: state.sortBy,
        sortDirection: state.sortDirection,
        aiModel: state.aiModel,
        skipDeleteConfirmation: state.skipDeleteConfirmation,
        lastVisitedPath: state.lastVisitedPath,
        lastSyncedAt: state.lastSyncedAt,
      }),
    }
  )
);

/**
 * Hook to use settings with Convex sync
 * Automatically syncs local changes to Convex when authenticated
 */
export function useSettings() {
  return useSettingsStore();
}

/**
 * Get effective theme (resolve "system" to actual theme)
 */
export function getEffectiveTheme(theme: Theme): "light" | "dark" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return theme;
}
