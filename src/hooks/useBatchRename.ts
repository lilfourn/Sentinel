/**
 * useBatchRename - Hook for AI-powered batch file renaming in folders
 *
 * Manages state for fetching batch AI rename suggestions and applying them.
 */

import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { showSuccess } from '../stores/toast-store';
import { useSubscriptionStore } from '../stores/subscription-store';
import type { FileEntry } from '../types/file';

export interface BatchRenameSuggestion {
  originalName: string;
  suggestedName: string;
  path: string;
  selected: boolean;
}

interface BatchRenameResponse {
  suggestions: BatchRenameSuggestion[];
  totalFiles: number;
  skippedFiles: number;
}

interface BatchRenameProgress {
  stage: 'scanning' | 'analyzing' | 'complete';
  current: number;
  total: number;
  message: string;
}

interface BatchRenameResult {
  successCount: number;
  failedCount: number;
  results: { success: boolean; oldPath: string; newPath: string }[];
}

interface BatchRenameState {
  isOpen: boolean;
  isLoading: boolean;
  isApplying: boolean;
  entry: FileEntry | null;
  suggestions: BatchRenameSuggestion[];
  progress: BatchRenameProgress | null;
  error: string | null;
}

interface UseBatchRenameReturn extends BatchRenameState {
  /** Request batch rename suggestions for a folder */
  request: (entry: FileEntry) => Promise<void>;
  /** Apply selected renames */
  apply: () => Promise<void>;
  /** Cancel and close the dialog */
  cancel: () => void;
  /** Toggle selection for a specific suggestion */
  toggleSelection: (path: string) => void;
  /** Select or deselect all */
  selectAll: (selected: boolean) => void;
}

/**
 * Hook for managing batch rename state and operations
 * @param onSuccess - Optional callback when rename succeeds (e.g., refresh directory)
 */
export function useBatchRename(onSuccess?: () => void): UseBatchRenameReturn {
  const [state, setState] = useState<BatchRenameState>({
    isOpen: false,
    isLoading: false,
    isApplying: false,
    entry: null,
    suggestions: [],
    progress: null,
    error: null,
  });

  const userId = useSubscriptionStore((s) => s.userId);

  // Listen for progress events
  useEffect(() => {
    if (!state.isLoading) return;

    let unlistenFn: (() => void) | null = null;

    listen<BatchRenameProgress>('batch-rename-progress', (event) => {
      setState((s) => ({
        ...s,
        progress: event.payload,
      }));
    })
      .then((fn) => {
        unlistenFn = fn;
      })
      .catch(console.error);

    return () => {
      unlistenFn?.();
    };
  }, [state.isLoading]);

  /**
   * Request batch rename suggestions for a folder
   */
  const request = useCallback(
    async (entry: FileEntry) => {
      if (!entry.isDirectory) {
        return;
      }

      // Open dialog immediately with loading state
      setState({
        isOpen: true,
        isLoading: true,
        isApplying: false,
        entry,
        suggestions: [],
        progress: { stage: 'scanning', current: 0, total: 0, message: 'Scanning folder...' },
        error: null,
      });

      try {
        const response = await invoke<BatchRenameResponse>('get_batch_rename_suggestions', {
          userId,
          folderPath: entry.path,
        });

        if (response.suggestions.length === 0) {
          setState((s) => ({
            ...s,
            isOpen: false,
            isLoading: false,
          }));
          showSuccess(
            'No changes needed',
            `All ${response.totalFiles} files already have good names`
          );
          return;
        }

        setState((s) => ({
          ...s,
          isLoading: false,
          suggestions: response.suggestions,
          progress: null,
        }));
      } catch (error) {
        const message = String(error);

        if (message.includes('Authentication required')) {
          setState((s) => ({
            ...s,
            isLoading: false,
            error: 'Please sign in to use AI batch rename',
          }));
        } else if (message.includes('limit exceeded') || message.includes('Limit exceeded')) {
          setState((s) => ({
            ...s,
            isLoading: false,
            error: 'Daily rename limit reached. Upgrade to Pro for more.',
          }));
        } else {
          setState((s) => ({
            ...s,
            isLoading: false,
            error: message,
          }));
        }
      }
    },
    [userId]
  );

  /**
   * Apply selected renames
   */
  const apply = useCallback(async () => {
    const selectedSuggestions = state.suggestions.filter((s) => s.selected);
    if (selectedSuggestions.length === 0) return;

    setState((s) => ({ ...s, isApplying: true }));

    try {
      const items = selectedSuggestions.map((s) => ({
        path: s.path,
        newName: s.suggestedName,
      }));

      const result = await invoke<BatchRenameResult>('apply_batch_rename', { items });

      // Close dialog
      setState({
        isOpen: false,
        isLoading: false,
        isApplying: false,
        entry: null,
        suggestions: [],
        progress: null,
        error: null,
      });

      // Show result
      if (result.failedCount === 0) {
        showSuccess('Files renamed', `Successfully renamed ${result.successCount} files`);
      } else {
        showSuccess(
          'Batch rename complete',
          `Renamed ${result.successCount} files, ${result.failedCount} failed`
        );
      }

      // Refresh directory
      onSuccess?.();
    } catch (error) {
      setState((s) => ({
        ...s,
        isApplying: false,
        error: String(error),
      }));
    }
  }, [state.suggestions, onSuccess]);

  /**
   * Cancel and close the dialog
   */
  const cancel = useCallback(() => {
    setState({
      isOpen: false,
      isLoading: false,
      isApplying: false,
      entry: null,
      suggestions: [],
      progress: null,
      error: null,
    });
  }, []);

  /**
   * Toggle selection for a specific suggestion
   */
  const toggleSelection = useCallback((path: string) => {
    setState((s) => ({
      ...s,
      suggestions: s.suggestions.map((suggestion) =>
        suggestion.path === path ? { ...suggestion, selected: !suggestion.selected } : suggestion
      ),
    }));
  }, []);

  /**
   * Select or deselect all suggestions
   */
  const selectAll = useCallback((selected: boolean) => {
    setState((s) => ({
      ...s,
      suggestions: s.suggestions.map((suggestion) => ({ ...suggestion, selected })),
    }));
  }, []);

  return {
    ...state,
    request,
    apply,
    cancel,
    toggleSelection,
    selectAll,
  };
}
