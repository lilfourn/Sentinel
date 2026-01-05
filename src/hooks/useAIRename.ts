/**
 * useAIRename - Hook for AI-powered file renaming
 *
 * Manages state for fetching AI rename suggestions and applying them.
 */

import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { showRenameToast, showError, showInfo } from '../stores/toast-store';
import { useSubscriptionStore } from '../stores/subscription-store';
import type { FileEntry } from '../types/file';

interface RenameSuggestion {
  originalName: string;
  suggestedName: string;
  path: string;
}

interface RenameResult {
  success: boolean;
  oldPath: string;
  newPath: string;
}

interface AIRenameState {
  isOpen: boolean;
  isLoading: boolean;
  entry: FileEntry | null;
  suggestion: RenameSuggestion | null;
  error: string | null;
}

interface UseAIRenameReturn extends AIRenameState {
  /** Request an AI rename suggestion for a file */
  request: (entry: FileEntry) => Promise<void>;
  /** Apply the suggested rename */
  apply: () => Promise<void>;
  /** Cancel and close the dialog */
  cancel: () => void;
  /** Retry fetching a suggestion after an error */
  retry: () => Promise<void>;
}

/**
 * Hook for managing AI rename state and operations
 * @param onSuccess - Optional callback when rename succeeds (e.g., refresh directory)
 */
export function useAIRename(onSuccess?: () => void): UseAIRenameReturn {
  const [state, setState] = useState<AIRenameState>({
    isOpen: false,
    isLoading: false,
    entry: null,
    suggestion: null,
    error: null,
  });

  const userId = useSubscriptionStore((s) => s.userId);

  /**
   * Fetch content preview for a file (first ~500 chars for context)
   */
  const getContentPreview = useCallback(async (path: string): Promise<string | null> => {
    try {
      // Try to read a small portion of the file for context
      const content = await invoke<string>('read_file_preview', {
        path,
        maxBytes: 1024,
      });
      return content;
    } catch {
      // File might be binary or unreadable, that's fine
      return null;
    }
  }, []);

  /**
   * Request an AI rename suggestion
   */
  const request = useCallback(async (entry: FileEntry) => {
    // Open dialog immediately with loading state
    setState({
      isOpen: true,
      isLoading: true,
      entry,
      suggestion: null,
      error: null,
    });

    try {
      // Get content preview for better AI suggestions
      const contentPreview = await getContentPreview(entry.path);

      // Call backend to get AI suggestion
      const suggestion = await invoke<RenameSuggestion>('get_rename_suggestion', {
        userId,
        path: entry.path,
        filename: entry.name,
        extension: entry.extension,
        size: entry.size,
        contentPreview,
      });

      // Check if suggestion is the same as original
      if (suggestion.suggestedName === suggestion.originalName) {
        setState((s) => ({
          ...s,
          isOpen: false,
          isLoading: false,
        }));
        showInfo('No changes needed', 'AI suggests keeping the current name');
        return;
      }

      setState((s) => ({
        ...s,
        isLoading: false,
        suggestion,
      }));
    } catch (error) {
      const message = String(error);

      // Handle specific error cases
      if (message.includes('Authentication required')) {
        setState((s) => ({
          ...s,
          isLoading: false,
          error: 'Please sign in to use AI rename',
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
  }, [userId, getContentPreview]);

  /**
   * Apply the rename suggestion
   */
  const apply = useCallback(async () => {
    const { entry, suggestion } = state;
    if (!entry || !suggestion) return;

    setState((s) => ({ ...s, isLoading: true }));

    try {
      const result = await invoke<RenameResult>('apply_rename', {
        oldPath: entry.path,
        newName: suggestion.suggestedName,
      });

      if (result.success) {
        // Close dialog
        setState({
          isOpen: false,
          isLoading: false,
          entry: null,
          suggestion: null,
          error: null,
        });

        // Show toast with undo option
        showRenameToast(
          'File renamed',
          `${suggestion.originalName} → ${suggestion.suggestedName}`,
          async () => {
            try {
              await invoke('undo_rename', {
                currentPath: result.newPath,
                originalPath: result.oldPath,
              });
              onSuccess?.();
            } catch (error) {
              showError('Failed to undo', String(error));
            }
          }
        );

        // Refresh directory
        onSuccess?.();
      }
    } catch (error) {
      setState((s) => ({
        ...s,
        isLoading: false,
        error: String(error),
      }));
    }
  }, [state, onSuccess]);

  /**
   * Cancel and close the dialog
   */
  const cancel = useCallback(() => {
    setState({
      isOpen: false,
      isLoading: false,
      entry: null,
      suggestion: null,
      error: null,
    });
  }, []);

  /**
   * Retry after an error
   */
  const retry = useCallback(async () => {
    const { entry } = state;
    if (entry) {
      await request(entry);
    }
  }, [state, request]);

  return {
    ...state,
    request,
    apply,
    cancel,
    retry,
  };
}
