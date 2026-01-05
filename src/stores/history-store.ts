/**
 * History store for managing organization history and multi-level undo.
 */

import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type {
  HistorySummary,
  SessionSummary,
  HistorySession,
  UndoPreflightResult,
  UndoResult,
  ConflictResolution,
  FolderIndexEntry,
} from '../types/history';

interface HistoryState {
  // Current folder context
  currentFolder: string | null;
  summary: HistorySummary | null;
  sessions: SessionSummary[];
  isLoading: boolean;
  error: string | null;

  // Undo modal state
  isUndoModalOpen: boolean;
  targetSessionId: string | null;
  targetSession: SessionSummary | null;
  preflightResult: UndoPreflightResult | null;
  isRunningPreflight: boolean;
  isUndoing: boolean;
  undoProgress: { completed: number; total: number } | null;

  // Global folder list
  allFolders: FolderIndexEntry[];
}

interface HistoryActions {
  // Load history for a folder
  loadHistory: (folderPath: string) => Promise<void>;
  clearHistory: () => void;

  // Check if folder has history
  hasHistory: (folderPath: string) => Promise<boolean>;

  // Undo modal actions
  openUndoModal: (sessionId: string) => void;
  closeUndoModal: () => void;
  runPreflight: () => Promise<void>;
  executeUndo: (resolution: ConflictResolution) => Promise<UndoResult>;

  // Management
  deleteHistory: (folderPath: string) => Promise<void>;
  loadAllFolders: () => Promise<void>;

  // Get detailed session
  getSessionDetail: (sessionId: string) => Promise<HistorySession | null>;
}

type HistoryStore = HistoryState & HistoryActions;

export const useHistoryStore = create<HistoryStore>((set, get) => ({
  // Initial state
  currentFolder: null,
  summary: null,
  sessions: [],
  isLoading: false,
  error: null,
  isUndoModalOpen: false,
  targetSessionId: null,
  targetSession: null,
  preflightResult: null,
  isRunningPreflight: false,
  isUndoing: false,
  undoProgress: null,
  allFolders: [],

  // Load history for a folder
  loadHistory: async (folderPath: string) => {
    set({ isLoading: true, currentFolder: folderPath, error: null });

    try {
      const [summary, sessions] = await Promise.all([
        invoke<HistorySummary | null>('history_get_summary', { folderPath }),
        invoke<SessionSummary[]>('history_get_sessions', { folderPath }),
      ]);

      set({
        summary,
        sessions,
        isLoading: false,
      });
    } catch (error) {
      set({
        error: String(error),
        isLoading: false,
        summary: null,
        sessions: [],
      });
    }
  },

  // Clear current history state
  clearHistory: () => {
    set({
      currentFolder: null,
      summary: null,
      sessions: [],
      error: null,
    });
  },

  // Check if folder has history
  hasHistory: async (folderPath: string) => {
    try {
      return await invoke<boolean>('history_has_history', { folderPath });
    } catch {
      return false;
    }
  },

  // Open undo modal for a session
  openUndoModal: (sessionId: string) => {
    const { sessions } = get();
    const targetSession = sessions.find((s) => s.sessionId === sessionId) || null;

    set({
      isUndoModalOpen: true,
      targetSessionId: sessionId,
      targetSession,
      preflightResult: null,
      isRunningPreflight: false,
    });
  },

  // Close undo modal
  closeUndoModal: () => {
    set({
      isUndoModalOpen: false,
      targetSessionId: null,
      targetSession: null,
      preflightResult: null,
      isRunningPreflight: false,
      isUndoing: false,
      undoProgress: null,
    });
  },

  // Run preflight check
  runPreflight: async () => {
    const { currentFolder, targetSessionId } = get();
    if (!currentFolder || !targetSessionId) return;

    set({ isRunningPreflight: true, error: null });

    try {
      const result = await invoke<UndoPreflightResult>('history_undo_preflight', {
        folderPath: currentFolder,
        targetSessionId,
      });

      set({ preflightResult: result, isRunningPreflight: false });
    } catch (error) {
      set({
        error: String(error),
        isRunningPreflight: false,
      });
    }
  },

  // Execute undo operation
  executeUndo: async (resolution: ConflictResolution): Promise<UndoResult> => {
    const { currentFolder, targetSessionId } = get();
    if (!currentFolder || !targetSessionId) {
      return {
        success: false,
        operationsUndone: 0,
        operationsSkipped: 0,
        errors: ['No folder or session selected'],
      };
    }

    set({ isUndoing: true, undoProgress: { completed: 0, total: 0 }, error: null });

    // Set up progress listener
    let unlisten: UnlistenFn | null = null;

    try {
      unlisten = await listen<{ completed: number; total: number }>(
        'undo-progress',
        (event) => {
          set({ undoProgress: event.payload });
        }
      );

      const result = await invoke<UndoResult>('history_undo_execute', {
        folderPath: currentFolder,
        targetSessionId,
        resolution,
      });

      // Reload history after undo
      await get().loadHistory(currentFolder);

      set({
        isUndoing: false,
        isUndoModalOpen: false,
        targetSessionId: null,
        targetSession: null,
        preflightResult: null,
        undoProgress: null,
      });

      return result;
    } catch (error) {
      set({
        error: String(error),
        isUndoing: false,
      });

      return {
        success: false,
        operationsUndone: 0,
        operationsSkipped: 0,
        errors: [String(error)],
      };
    } finally {
      if (unlisten) {
        unlisten();
      }
    }
  },

  // Delete history for a folder
  deleteHistory: async (folderPath: string) => {
    try {
      await invoke('history_delete', { folderPath });

      // Clear current state if this was the current folder
      const { currentFolder } = get();
      if (currentFolder === folderPath) {
        get().clearHistory();
      }

      // Reload all folders list
      await get().loadAllFolders();
    } catch (error) {
      set({ error: String(error) });
    }
  },

  // Load all folders with history
  loadAllFolders: async () => {
    try {
      const folders = await invoke<FolderIndexEntry[]>('history_list_folders');
      set({ allFolders: folders });
    } catch (error) {
      set({ error: String(error), allFolders: [] });
    }
  },

  // Get detailed session information
  getSessionDetail: async (sessionId: string): Promise<HistorySession | null> => {
    const { currentFolder } = get();
    if (!currentFolder) return null;

    try {
      return await invoke<HistorySession | null>('history_get_session_detail', {
        folderPath: currentFolder,
        sessionId,
      });
    } catch {
      return null;
    }
  },
}));
