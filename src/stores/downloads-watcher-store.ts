import { create } from "zustand";
import { persist } from "zustand/middleware";
import { invoke } from "@tauri-apps/api/core";

// ============================================================================
// Types
// ============================================================================

export interface RenameHistoryItem {
  id: string;
  /** @deprecated Use folderId + originalName instead. Kept for migration only. */
  originalPath?: string;
  originalName: string;
  /** @deprecated Use folderId + newName instead. Kept for migration only. */
  newPath?: string;
  newName: string;
  timestamp: number;
  canUndo: boolean;
  undone: boolean;
  /** Folder ID (preferred) or folder path (legacy) for display */
  folderId: string;
  /** Folder display name (cached for offline display) */
  folderName: string;
}

export interface WatchedFolder {
  id: string;
  path: string;
  name: string; // Display name (e.g., "Downloads", "Desktop")
  enabled: boolean;
  addedAt: number;
}

export interface CustomRenameRule {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  priority: number; // Lower = higher priority
  // Match conditions
  matchType: "extension" | "pattern" | "folder" | "content";
  matchValue: string; // e.g., ".pdf", "screenshot*", "/Downloads"
  // Transform action
  transformType: "prefix" | "suffix" | "replace" | "template" | "ai-prompt";
  transformValue: string; // e.g., "doc-", "-backup", custom prompt
  // Examples for AI
  examples?: string[];
}

export interface WatcherStatus {
  enabled: boolean;
  watchingPaths: string[];
  processingCount: number;
}

// ============================================================================
// Default Rules
// ============================================================================

export const DEFAULT_RULES: CustomRenameRule[] = [
  {
    id: "rule-screenshots",
    name: "Screenshots",
    description: "Clean up screenshot filenames with date",
    enabled: true,
    priority: 1,
    matchType: "pattern",
    matchValue: "Screenshot*|Screen Shot*|Capture*",
    transformType: "template",
    transformValue: "screenshot-{date}",
  },
  {
    id: "rule-downloads",
    name: "Downloaded Files",
    description: "Remove (1), (2) suffixes and clean names",
    enabled: true,
    priority: 2,
    matchType: "pattern",
    matchValue: "* (1)*|* (2)*|* (3)*|*copy*",
    transformType: "ai-prompt",
    transformValue: "Remove duplicate indicators and clean the filename",
  },
  {
    id: "rule-invoices",
    name: "Invoices & Receipts",
    description: "Format as invoice-{vendor}-{date}.pdf",
    enabled: true,
    priority: 3,
    matchType: "content",
    matchValue: "invoice|receipt|payment|order confirmation",
    transformType: "template",
    transformValue: "invoice-{vendor}-{date}",
  },
];

// ============================================================================
// Store State
// ============================================================================

interface DownloadsWatcherState {
  // History - O(1) lookup via historyIndex
  history: RenameHistoryItem[];
  historyIndex: Map<string, number>; // id -> array index for O(1) lookup
  maxHistoryItems: number;

  // Watched folders - O(1) lookup via Maps
  watchedFolders: WatchedFolder[];
  folderById: Map<string, WatchedFolder>;   // id -> folder for O(1) lookup
  folderByPath: Map<string, WatchedFolder>; // path -> folder for O(1) lookup

  // Custom rules - O(1) sorted access via pre-computed array
  customRules: CustomRenameRule[];
  sortedEnabledRules: CustomRenameRule[]; // Pre-sorted for O(1) access
  rulesEnabled: boolean;

  // Status
  isWatching: boolean;
  processingFiles: Set<string>;
}

// ============================================================================
// Helpers for O(1) index maintenance
// ============================================================================

/** Rebuild history index - O(n) but only on mutation, not lookup */
const rebuildHistoryIndex = (history: RenameHistoryItem[]): Map<string, number> => {
  const index = new Map<string, number>();
  history.forEach((item, i) => index.set(item.id, i));
  return index;
};

/** Rebuild folder indices - O(n) but only on mutation, not lookup */
const rebuildFolderIndices = (folders: WatchedFolder[]) => ({
  folderById: new Map(folders.map((f) => [f.id, f])),
  folderByPath: new Map(folders.map((f) => [f.path, f])),
});

/** Compute sorted enabled rules - O(n log n) but only on mutation */
const computeSortedEnabledRules = (rules: CustomRenameRule[]): CustomRenameRule[] =>
  rules.filter((r) => r.enabled).sort((a, b) => a.priority - b.priority);

interface DownloadsWatcherActions {
  // History actions
  addToHistory: (item: Omit<RenameHistoryItem, "id" | "timestamp" | "canUndo" | "undone">) => void;
  markUndone: (id: string) => void;
  clearHistory: () => void;
  removeFromHistory: (id: string) => void;

  // Folder actions
  addWatchedFolder: (path: string, name?: string) => void;
  removeWatchedFolder: (id: string) => void;
  toggleFolderEnabled: (id: string) => void;
  getEnabledFolders: () => WatchedFolder[];

  // Rule actions
  addRule: (rule: Omit<CustomRenameRule, "id">) => void;
  updateRule: (id: string, updates: Partial<CustomRenameRule>) => void;
  removeRule: (id: string) => void;
  toggleRuleEnabled: (id: string) => void;
  reorderRules: (ruleIds: string[]) => void;
  setRulesEnabled: (enabled: boolean) => void;
  resetToDefaultRules: () => void;

  // Status actions
  setIsWatching: (watching: boolean) => void;
  addProcessingFile: (path: string) => void;
  removeProcessingFile: (path: string) => void;

  // Undo action
  undoRename: (historyId: string) => Promise<boolean>;
}

// ============================================================================
// Store
// ============================================================================

export const useDownloadsWatcherStore = create<DownloadsWatcherState & DownloadsWatcherActions>()(
  persist(
    (set, get) => ({
      // Initial state with O(1) indices
      history: [],
      historyIndex: new Map(),
      maxHistoryItems: 100,
      watchedFolders: [],
      folderById: new Map(),
      folderByPath: new Map(),
      customRules: DEFAULT_RULES,
      sortedEnabledRules: computeSortedEnabledRules(DEFAULT_RULES),
      rulesEnabled: true,
      isWatching: false,
      processingFiles: new Set(),

      // History actions - O(1) lookup, O(n) mutation
      addToHistory: (item) => {
        const newItem: RenameHistoryItem = {
          ...item,
          id: `rename-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
          timestamp: Date.now(),
          canUndo: true,
          undone: false,
        };

        set((state) => {
          const newHistory = [newItem, ...state.history].slice(0, state.maxHistoryItems);
          return {
            history: newHistory,
            historyIndex: rebuildHistoryIndex(newHistory),
          };
        });
      },

      markUndone: (id) => {
        set((state) => {
          // O(1) lookup
          const index = state.historyIndex.get(id);
          if (index === undefined) return state;

          const newHistory = [...state.history];
          newHistory[index] = { ...newHistory[index], undone: true, canUndo: false };
          return { history: newHistory };
        });
      },

      clearHistory: () => set({ history: [], historyIndex: new Map() }),

      removeFromHistory: (id) => {
        set((state) => {
          const newHistory = state.history.filter((item) => item.id !== id);
          return {
            history: newHistory,
            historyIndex: rebuildHistoryIndex(newHistory),
          };
        });
      },

      // Folder actions - O(1) lookup via Maps
      addWatchedFolder: (path, name) => {
        const folderName = name || path.split("/").pop() || path;
        const newFolder: WatchedFolder = {
          id: `folder-${Date.now()}`,
          path,
          name: folderName,
          enabled: true,
          addedAt: Date.now(),
        };

        set((state) => {
          // O(1) duplicate check
          if (state.folderByPath.has(path)) {
            return state;
          }
          const newFolders = [...state.watchedFolders, newFolder];
          return {
            watchedFolders: newFolders,
            ...rebuildFolderIndices(newFolders),
          };
        });
      },

      removeWatchedFolder: (id) => {
        set((state) => {
          // O(1) lookup
          const folder = state.folderById.get(id);
          if (!folder) return state;

          const newFolders = state.watchedFolders.filter((f) => f.id !== id);
          return {
            watchedFolders: newFolders,
            ...rebuildFolderIndices(newFolders),
          };
        });
      },

      toggleFolderEnabled: (id) => {
        set((state) => {
          // O(1) lookup
          if (!state.folderById.has(id)) return state;

          const newFolders = state.watchedFolders.map((f) =>
            f.id === id ? { ...f, enabled: !f.enabled } : f
          );
          return {
            watchedFolders: newFolders,
            ...rebuildFolderIndices(newFolders),
          };
        });
      },

      getEnabledFolders: () => {
        return get().watchedFolders.filter((f) => f.enabled);
      },

      // Rule actions - maintain pre-sorted list
      addRule: (rule) => {
        const newRule: CustomRenameRule = {
          ...rule,
          id: `rule-${Date.now()}`,
        };

        set((state) => {
          const newRules = [...state.customRules, newRule];
          return {
            customRules: newRules,
            sortedEnabledRules: computeSortedEnabledRules(newRules),
          };
        });
      },

      updateRule: (id, updates) => {
        set((state) => {
          const newRules = state.customRules.map((r) =>
            r.id === id ? { ...r, ...updates } : r
          );
          return {
            customRules: newRules,
            sortedEnabledRules: computeSortedEnabledRules(newRules),
          };
        });
      },

      removeRule: (id) => {
        set((state) => {
          const newRules = state.customRules.filter((r) => r.id !== id);
          return {
            customRules: newRules,
            sortedEnabledRules: computeSortedEnabledRules(newRules),
          };
        });
      },

      toggleRuleEnabled: (id) => {
        set((state) => {
          const newRules = state.customRules.map((r) =>
            r.id === id ? { ...r, enabled: !r.enabled } : r
          );
          return {
            customRules: newRules,
            sortedEnabledRules: computeSortedEnabledRules(newRules),
          };
        });
      },

      reorderRules: (ruleIds) => {
        set((state) => {
          const ruleMap = new Map(state.customRules.map((r) => [r.id, r]));
          const reordered = ruleIds
            .map((id, index) => {
              const rule = ruleMap.get(id);
              return rule ? { ...rule, priority: index + 1 } : null;
            })
            .filter(Boolean) as CustomRenameRule[];
          return {
            customRules: reordered,
            sortedEnabledRules: computeSortedEnabledRules(reordered),
          };
        });
      },

      setRulesEnabled: (enabled) => set({ rulesEnabled: enabled }),

      resetToDefaultRules: () => set({
        customRules: DEFAULT_RULES,
        sortedEnabledRules: computeSortedEnabledRules(DEFAULT_RULES),
      }),

      // Status actions
      setIsWatching: (watching) => set({ isWatching: watching }),

      addProcessingFile: (path) => {
        set((state) => {
          const newSet = new Set(state.processingFiles);
          newSet.add(path);
          return { processingFiles: newSet };
        });
      },

      removeProcessingFile: (path) => {
        set((state) => {
          const newSet = new Set(state.processingFiles);
          newSet.delete(path);
          return { processingFiles: newSet };
        });
      },

      // Undo action - O(1) lookups
      undoRename: async (historyId) => {
        const state = get();
        // O(1) history lookup
        const index = state.historyIndex.get(historyId);
        if (index === undefined) return false;

        const item = state.history[index];
        if (!item || !item.canUndo || item.undone) {
          return false;
        }

        // O(1) folder lookup
        const folder = state.folderById.get(item.folderId);
        if (!folder) {
          console.error("Cannot undo: watched folder not found");
          return false;
        }

        // Reconstruct full paths from folder path + filenames
        const currentPath = `${folder.path}/${item.newName}`;
        const originalPath = `${folder.path}/${item.originalName}`;

        try {
          await invoke("undo_rename", {
            currentPath,
            originalPath,
          });

          get().markUndone(historyId);
          return true;
        } catch (error) {
          console.error("Failed to undo rename:", error);
          return false;
        }
      },
    }),
    {
      name: "sentinel-downloads-watcher",
      partialize: (state) => ({
        // Strip deprecated full path fields from history for privacy
        // Don't persist indices - they're rebuilt on rehydration
        history: state.history.map((item) => ({
          id: item.id,
          originalName: item.originalName,
          newName: item.newName,
          timestamp: item.timestamp,
          canUndo: item.canUndo,
          undone: item.undone,
          folderId: item.folderId,
          folderName: item.folderName,
        })),
        maxHistoryItems: state.maxHistoryItems,
        watchedFolders: state.watchedFolders.map((f) => ({
          id: f.id,
          path: f.path,
          name: f.name,
          enabled: f.enabled,
          addedAt: f.addedAt,
        })),
        customRules: state.customRules,
        rulesEnabled: state.rulesEnabled,
      }),
      // Rebuild O(1) indices after rehydration
      onRehydrateStorage: () => (state) => {
        if (state) {
          // Rebuild history index
          state.historyIndex = rebuildHistoryIndex(state.history);
          // Rebuild folder indices
          const folderIndices = rebuildFolderIndices(state.watchedFolders);
          state.folderById = folderIndices.folderById;
          state.folderByPath = folderIndices.folderByPath;
          // Rebuild sorted rules
          state.sortedEnabledRules = computeSortedEnabledRules(state.customRules);
        }
      },
    }
  )
);

// ============================================================================
// Selectors - Use getState() for non-React contexts (callbacks, effects)
// ============================================================================

/** Get recent renames (for use outside React components) */
export const selectRecentRenames = (limit = 10) => {
  const { history } = useDownloadsWatcherStore.getState();
  return history.slice(0, limit);
};

/** Get undoable renames (for use outside React components) */
export const selectUndoableRenames = () => {
  const { history } = useDownloadsWatcherStore.getState();
  return history.filter((h) => h.canUndo && !h.undone);
};

/** Get folder by path - O(1) lookup */
export const selectFolderByPath = (path: string): WatchedFolder | undefined => {
  return useDownloadsWatcherStore.getState().folderByPath.get(path);
};

/** Get folder by ID - O(1) lookup */
export const selectFolderById = (id: string): WatchedFolder | undefined => {
  return useDownloadsWatcherStore.getState().folderById.get(id);
};

/**
 * Convert a glob pattern to a safe regex pattern
 * Escapes regex special chars first, then converts glob wildcards
 */
function globToSafeRegex(pattern: string): RegExp | null {
  try {
    // Limit pattern length to prevent DoS
    if (pattern.length > 200) {
      console.warn("Pattern too long, skipping:", pattern.slice(0, 50));
      return null;
    }

    // Escape regex special characters FIRST (except * and ?)
    const escaped = pattern
      .replace(/[.+^${}()|[\]\\]/g, "\\$&")
      // Then convert glob wildcards to regex
      .replace(/\*/g, "[^/]*")  // * matches any chars except path separator
      .replace(/\?/g, ".");     // ? matches single char

    return new RegExp("^" + escaped + "$", "i");
  } catch {
    console.warn("Invalid pattern:", pattern);
    return null;
  }
}

/**
 * Match a filename against rules - returns the first matching rule
 * Uses pre-sorted rules for O(n) instead of O(n log n) per call
 * SECURITY: Patterns are sanitized to prevent ReDoS attacks
 */
export const selectRuleByMatch = (filename: string, content?: string) => {
  const { sortedEnabledRules, rulesEnabled } = useDownloadsWatcherStore.getState();

  if (!rulesEnabled) return null;

  // Limit filename length for regex matching
  const safeFilename = filename.slice(0, 500);

  // Use pre-sorted rules - O(n) iteration, no sort needed
  for (const rule of sortedEnabledRules) {
    switch (rule.matchType) {
      case "extension": {
        const ext = safeFilename.split(".").pop()?.toLowerCase();
        if (ext && rule.matchValue.toLowerCase().includes(ext)) {
          return rule;
        }
        break;
      }
      case "pattern": {
        const patterns = rule.matchValue.split("|").map((p) => p.trim()).slice(0, 20);
        for (const pattern of patterns) {
          const regex = globToSafeRegex(pattern);
          if (regex && regex.test(safeFilename)) {
            return rule;
          }
        }
        break;
      }
      case "folder": {
        if (rule.matchValue && safeFilename.toLowerCase().includes(rule.matchValue.toLowerCase())) {
          return rule;
        }
        break;
      }
      case "content": {
        if (content) {
          const keywords = rule.matchValue.toLowerCase().split("|").map((k) => k.trim()).slice(0, 20);
          const safeContent = content.slice(0, 10000).toLowerCase();
          if (keywords.some((k) => k.length > 0 && safeContent.includes(k))) {
            return rule;
          }
        }
        break;
      }
    }
  }

  return null;
};
