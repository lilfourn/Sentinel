import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useExternalFileDrop } from './useExternalFileDrop';
import { getContextStrategy, type ContextItem } from '../stores/chat-store';

/** Pending item during drag (before drop is confirmed) */
export interface PendingDropItem {
  path: string;
  name: string;
  type: 'file' | 'folder' | 'unknown';
  size?: number;
  mimeType?: string;
}

interface UseChatDropZoneOptions {
  /** Callback when context items should be added */
  onContextAdd: (items: Omit<ContextItem, 'id'>[]) => void;
  /** Whether the drop zone is enabled */
  enabled?: boolean;
}

interface UseChatDropZoneReturn {
  /** Whether any drag is currently over the drop zone */
  isDragOver: boolean;
  /** Source of the current drag */
  dragSource: 'internal' | 'external' | null;
  /** Items pending to be dropped (for preview) */
  pendingItems: PendingDropItem[];
  /** React event handlers for the drop zone container */
  handlers: {
    onDragOver: (e: React.DragEvent) => void;
    onDragLeave: (e: React.DragEvent) => void;
    onDrop: (e: React.DragEvent) => void;
  };
}

/** Image file extensions for detecting vision strategy */
const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico']);

/** Get file extension from path */
function getExtension(path: string): string {
  const parts = path.split('.');
  return parts.length > 1 ? parts[parts.length - 1].toLowerCase() : '';
}

/** Check if path is likely an image based on extension */
function isImagePath(path: string): boolean {
  return IMAGE_EXTENSIONS.has(getExtension(path));
}

/** Get filename from path */
function getFileName(path: string): string {
  const parts = path.split('/');
  return parts[parts.length - 1] || path;
}

/** File metadata result from backend */
interface FileMetadata {
  path: string;
  name: string;
  isDirectory: boolean;
  size: number;
  mimeType?: string;
}

/**
 * Hook for managing drag-and-drop in the ChatPanel.
 *
 * Handles both:
 * - Internal drags from file list (via sentinel/* MIME types)
 * - External drags from Finder/Explorer (via Tauri API)
 */
export function useChatDropZone(options: UseChatDropZoneOptions): UseChatDropZoneReturn {
  const { onContextAdd, enabled = true } = options;

  const [isInternalDragOver, setIsInternalDragOver] = useState(false);
  const [pendingItems, setPendingItems] = useState<PendingDropItem[]>([]);
  const [dragSource, setDragSource] = useState<'internal' | 'external' | null>(null);

  // Ref for stable callback
  const onContextAddRef = useRef(onContextAdd);
  // eslint-disable-next-line react-hooks/refs -- Sync ref with latest callback
  onContextAddRef.current = onContextAdd;

  // Handle external file drops via Tauri
  const { isDraggingExternal, pendingPaths } = useExternalFileDrop({
    enabled,
    onEnter: (paths) => {
      setDragSource('external');
      // Create pending items from paths (without full metadata yet)
      const items: PendingDropItem[] = paths.map((path) => ({
        path,
        name: getFileName(path),
        type: 'unknown' as const,
      }));
      setPendingItems(items);
    },
    onLeave: () => {
      if (dragSource === 'external') {
        setDragSource(null);
        setPendingItems([]);
      }
    },
    onDrop: async (paths) => {
      // Fetch metadata for dropped files and add to context
      await handleExternalDrop(paths);
      setDragSource(null);
      setPendingItems([]);
    },
  });

  /** Fetch metadata and add external files to context */
  const handleExternalDrop = useCallback(async (paths: string[]) => {
    const contextItems: Omit<ContextItem, 'id'>[] = [];

    for (const path of paths) {
      try {
        // Try to get metadata from backend
        const metadata = await invoke<FileMetadata>('get_file_metadata', { path });

        const type = metadata.isDirectory ? 'folder' : 'file';
        const mimeType = metadata.mimeType || (isImagePath(path) ? 'image/*' : undefined);
        const strategy = getContextStrategy(type, mimeType);

        contextItems.push({
          type: type === 'folder' ? 'folder' : isImagePath(path) ? 'image' : 'file',
          path: metadata.path,
          name: metadata.name,
          strategy,
          size: metadata.size,
          mimeType,
        });
      } catch (error) {
        console.error(`[useChatDropZone] Failed to get metadata for ${path}:`, error);

        // Fallback: use path-based inference
        const name = getFileName(path);
        const isImage = isImagePath(path);

        contextItems.push({
          type: isImage ? 'image' : 'file',
          path,
          name,
          strategy: isImage ? 'vision' : 'read',
        });
      }
    }

    if (contextItems.length > 0) {
      onContextAddRef.current(contextItems);
    }
  }, []);

  /** Handle internal drag over (from file list with sentinel/* data) */
  const handleDragOver = useCallback(
    (e: React.DragEvent) => {
      if (!enabled) return;

      e.preventDefault();

      // Check for sentinel custom data (internal drag) or Files (external drag)
      const hasInternal = e.dataTransfer.types.includes('sentinel/path');
      const hasExternal = e.dataTransfer.types.includes('Files');

      if (hasInternal) {
        e.dataTransfer.dropEffect = 'link';
        setIsInternalDragOver(true);
        setDragSource('internal');

        // Parse pending items from sentinel data for preview
        if (pendingItems.length === 0) {
          // Try to get all items from sentinel/json first (multi-file drag)
          const jsonData = e.dataTransfer.getData('sentinel/json');
          if (jsonData) {
            try {
              const paths = JSON.parse(jsonData) as string[];
              const items: PendingDropItem[] = paths.map((p) => ({
                path: p,
                name: getFileName(p),
                type: 'unknown' as const,
              }));
              setPendingItems(items);
              return;
            } catch {
              // Fall through to single-file handling
            }
          }

          // Fallback: single file from sentinel/path
          const path = e.dataTransfer.getData('sentinel/path');
          if (path) {
            const type = e.dataTransfer.getData('sentinel/type') as 'file' | 'folder';
            const name = e.dataTransfer.getData('sentinel/name') || getFileName(path);
            const sizeStr = e.dataTransfer.getData('sentinel/size');
            const size = sizeStr ? parseInt(sizeStr, 10) : undefined;
            const mimeType = e.dataTransfer.getData('sentinel/mime') || undefined;

            setPendingItems([{ path, name, type, size, mimeType }]);
          }
        }
      } else if (hasExternal && !isDraggingExternal) {
        // External drag detected via browser event but not yet via Tauri
        e.dataTransfer.dropEffect = 'copy';
      }
    },
    [enabled, isDraggingExternal, pendingItems.length]
  );

  /** Handle drag leave */
  const handleDragLeave = useCallback(
    (e: React.DragEvent) => {
      // Only clear if leaving the container entirely
      if (!e.currentTarget.contains(e.relatedTarget as Node)) {
        if (dragSource === 'internal') {
          setIsInternalDragOver(false);
          setDragSource(null);
          setPendingItems([]);
        }
      }
    },
    [dragSource]
  );

  /** Fetch metadata and add internal multi-file drop to context */
  const handleInternalMultiDrop = useCallback(async (paths: string[]) => {
    const contextItems: Omit<ContextItem, 'id'>[] = [];

    for (const filePath of paths) {
      try {
        // Try to get metadata from backend
        const metadata = await invoke<FileMetadata>('get_file_metadata', { path: filePath });

        const type = metadata.isDirectory ? 'folder' : 'file';
        const mimeType = metadata.mimeType || (isImagePath(filePath) ? 'image/*' : undefined);
        const strategy = getContextStrategy(type, mimeType);

        contextItems.push({
          type: type === 'folder' ? 'folder' : isImagePath(filePath) ? 'image' : 'file',
          path: metadata.path,
          name: metadata.name,
          strategy,
          size: metadata.size,
          mimeType,
        });
      } catch (error) {
        console.error(`[useChatDropZone] Failed to get metadata for ${filePath}:`, error);

        // Fallback: use path-based inference
        const name = getFileName(filePath);
        const isImage = isImagePath(filePath);

        contextItems.push({
          type: isImage ? 'image' : 'file',
          path: filePath,
          name,
          strategy: isImage ? 'vision' : 'read',
        });
      }
    }

    if (contextItems.length > 0) {
      onContextAddRef.current(contextItems);
    }
  }, []);

  /** Handle drop for internal drags */
  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();

      // Try to get all items from sentinel/json first (multi-file drag)
      const jsonData = e.dataTransfer.getData('sentinel/json');
      if (jsonData) {
        try {
          const paths = JSON.parse(jsonData) as string[];
          if (paths.length > 0) {
            // Fetch metadata for all dropped files
            handleInternalMultiDrop(paths);
            // Reset state
            setIsInternalDragOver(false);
            setDragSource(null);
            setPendingItems([]);
            return;
          }
        } catch {
          // Fall through to single-file handling
        }
      }

      // Fallback: Handle single internal sentinel drag
      const path = e.dataTransfer.getData('sentinel/path');
      if (path) {
        const type = e.dataTransfer.getData('sentinel/type') as 'file' | 'folder';
        const name = e.dataTransfer.getData('sentinel/name') || getFileName(path);
        const sizeStr = e.dataTransfer.getData('sentinel/size');
        const size = sizeStr ? parseInt(sizeStr, 10) : undefined;
        const mimeType = e.dataTransfer.getData('sentinel/mime') || undefined;

        const strategy = getContextStrategy(type, mimeType);

        onContextAddRef.current([
          {
            type: type === 'folder' ? 'folder' : mimeType?.startsWith('image/') ? 'image' : 'file',
            path,
            name,
            strategy,
            size,
            mimeType,
          },
        ]);
      }

      // Reset state
      setIsInternalDragOver(false);
      setDragSource(null);
      setPendingItems([]);
    },
    [handleInternalMultiDrop]
  );

  // Determine overall drag state
  const isDragOver = isInternalDragOver || isDraggingExternal;

  // Update pending items for external drags
  const effectivePendingItems =
    dragSource === 'external' && pendingPaths.length > 0
      ? pendingPaths.map((path) => ({
          path,
          name: getFileName(path),
          type: 'unknown' as const,
        }))
      : pendingItems;

  return {
    isDragOver,
    dragSource,
    pendingItems: effectivePendingItems,
    handlers: {
      onDragOver: handleDragOver,
      onDragLeave: handleDragLeave,
      onDrop: handleDrop,
    },
  };
}
