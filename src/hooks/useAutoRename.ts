import { useEffect, useCallback, useRef } from 'react';
import { listen, emit } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { showRenameToast, showError } from '../stores/toast-store';
import { useDownloadsWatcherStore, selectRuleByMatch, selectFolderByPath } from '../stores/downloads-watcher-store';
import { useSubscriptionStore } from '../stores/subscription-store';

interface FileCreatedEvent {
  id: string;
  eventType: string;
  path: string;
  fileName: string;
  extension: string | null;
  size: number;
  contentPreview: string | null;
  watchedFolder: string;
}

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

/** Rate limiting: minimum delay between AI API calls (ms) */
const RATE_LIMIT_MS = 1000;
/** Maximum queue size to prevent memory issues */
const MAX_QUEUE_SIZE = 50;

export function useAutoRename(enabled: boolean) {
  const processingRef = useRef<Set<string>>(new Set());
  const queueRef = useRef<FileCreatedEvent[]>([]);
  const queuePathsRef = useRef<Set<string>>(new Set()); // O(1) duplicate check
  const isProcessingQueueRef = useRef(false);
  const lastProcessTimeRef = useRef(0);

  const { addToHistory, addProcessingFile, removeProcessingFile, setIsWatching } =
    useDownloadsWatcherStore();

  // Sync watching state with store
  useEffect(() => {
    setIsWatching(enabled);
  }, [enabled, setIsWatching]);

  const processFile = useCallback(async (event: FileCreatedEvent) => {
    // Skip if already processing this specific file
    if (processingRef.current.has(event.path)) {
      return;
    }

    processingRef.current.add(event.path);
    addProcessingFile(event.path);

    try {
      // Check for matching custom rule (for future custom prompt support)
      const matchingRule = selectRuleByMatch(event.fileName, event.contentPreview || undefined);

      // Build custom prompt if rule has ai-prompt transform
      const customPrompt = matchingRule?.transformType === 'ai-prompt'
        ? matchingRule.transformValue
        : undefined;

      // Get userId for billing checks
      const userId = useSubscriptionStore.getState().userId;

      // Get rename suggestion from AI
      const suggestion = await invoke<RenameSuggestion>('get_rename_suggestion', {
        userId,
        path: event.path,
        filename: event.fileName,
        extension: event.extension,
        size: event.size,
        contentPreview: customPrompt
          ? `${event.contentPreview || ''}\n\n[Custom Rule: ${customPrompt}]`
          : event.contentPreview,
      });

      // Skip if suggestion is the same as original
      if (suggestion.suggestedName === suggestion.originalName) {
        return;
      }

      // Apply the rename
      const result = await invoke<RenameResult>('apply_rename', {
        oldPath: event.path,
        newName: suggestion.suggestedName,
      });

      if (result.success) {
        // O(1) folder lookup
        const folder = selectFolderByPath(event.watchedFolder);
        const folderId = folder?.id || `folder-${Date.now()}`;
        const folderName = folder?.name || event.watchedFolder.split('/').pop() || 'Unknown';

        // Add to history (without full paths for privacy)
        addToHistory({
          originalName: suggestion.originalName,
          newName: suggestion.suggestedName,
          folderId,
          folderName,
        });

        // Emit event for UsageSync to record in Convex
        emit('usage:record-rename', {
          originalName: suggestion.originalName,
          newName: suggestion.suggestedName,
          filePath: event.path,
          fileSize: event.size,
          mimeType: event.extension ? `application/${event.extension}` : undefined,
          aiModel: 'haiku', // Auto-rename uses Haiku
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
            } catch (error) {
              showError('Failed to undo', String(error));
            }
          }
        );
      }
    } catch (error) {
      // Log sanitized error (don't expose file paths)
      console.error('Auto-rename operation failed');
    } finally {
      processingRef.current.delete(event.path);
      removeProcessingFile(event.path);
    }
  }, [addToHistory, addProcessingFile, removeProcessingFile]);

  const processQueue = useCallback(async () => {
    if (isProcessingQueueRef.current || queueRef.current.length === 0) {
      return;
    }

    isProcessingQueueRef.current = true;

    try {
      const event = queueRef.current.shift();
      if (event) {
        // Remove from paths Set (O(1))
        queuePathsRef.current.delete(event.path);

        // Rate limiting: wait if we called API too recently
        const timeSinceLast = Date.now() - lastProcessTimeRef.current;
        if (timeSinceLast < RATE_LIMIT_MS) {
          await new Promise((resolve) => setTimeout(resolve, RATE_LIMIT_MS - timeSinceLast));
        }

        lastProcessTimeRef.current = Date.now();
        await processFile(event);
      }
    } finally {
      isProcessingQueueRef.current = false;

      // Process next item if queue is not empty
      if (queueRef.current.length > 0) {
        // Small delay to prevent blocking
        setTimeout(() => processQueue(), 100);
      }
    }
  }, [processFile]);

  const handleFileCreated = useCallback((event: FileCreatedEvent) => {
    // O(1) checks using Sets
    if (processingRef.current.has(event.path)) {
      return;
    }
    if (queuePathsRef.current.has(event.path)) {
      return;
    }

    // Add to queue (with size limit to prevent memory issues)
    if (queueRef.current.length >= MAX_QUEUE_SIZE) {
      console.warn('Auto-rename queue full, dropping oldest items');
      const droppedCount = Math.floor(MAX_QUEUE_SIZE / 2);
      const droppedEvents = queueRef.current.splice(0, droppedCount);
      // Also remove from paths Set
      droppedEvents.forEach((e) => queuePathsRef.current.delete(e.path));
    }

    // Add to both queue and Set (O(1))
    queueRef.current.push(event);
    queuePathsRef.current.add(event.path);
    processQueue();
  }, [processQueue]);

  useEffect(() => {
    if (!enabled) {
      // Clear queue and paths Set when disabled
      queueRef.current = [];
      queuePathsRef.current.clear();
      return;
    }

    let unlistenFn: (() => void) | null = null;
    let cancelled = false;

    listen<FileCreatedEvent>('sentinel://file-created', (event) => {
      if (!cancelled) {
        handleFileCreated(event.payload);
      }
    })
      .then((fn) => {
        if (cancelled) {
          // Component unmounted before listener was set up
          fn();
        } else {
          unlistenFn = fn;
        }
      })
      .catch((error) => {
        console.error('Failed to setup file watcher listener:', error);
      });

    return () => {
      cancelled = true;
      unlistenFn?.();
    };
  }, [enabled, handleFileCreated]);
}

// Hook to control the watcher
export function useWatcher() {
  const startWatcher = useCallback(async (path?: string) => {
    try {
      await invoke('start_downloads_watcher', { path });
      return true;
    } catch (error) {
      showError('Failed to start watcher', String(error));
      return false;
    }
  }, []);

  const stopWatcher = useCallback(async () => {
    try {
      await invoke('stop_downloads_watcher');
      return true;
    } catch (error) {
      showError('Failed to stop watcher', String(error));
      return false;
    }
  }, []);

  const getStatus = useCallback(async () => {
    return invoke<{ enabled: boolean; watchingPath: string | null }>('get_watcher_status');
  }, []);

  return { startWatcher, stopWatcher, getStatus };
}
