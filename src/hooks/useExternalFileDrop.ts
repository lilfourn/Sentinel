import { useState, useEffect, useRef } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

interface UseExternalFileDropOptions {
  /** Callback when files are dropped */
  onDrop: (paths: string[]) => void;
  /** Callback when drag enters the window */
  onEnter?: (paths: string[]) => void;
  /** Callback when drag leaves the window */
  onLeave?: () => void;
  /** Whether to enable the listener (default: true) */
  enabled?: boolean;
}

interface UseExternalFileDropReturn {
  /** Whether an external drag is currently over the window */
  isDraggingExternal: boolean;
  /** File paths currently being dragged (available during drag) */
  pendingPaths: string[];
  /** Current drag position (if available) */
  position: { x: number; y: number } | null;
}

/**
 * Hook to handle external file drops from Finder/Explorer using Tauri v2 API.
 *
 * This captures files dragged from outside the app (native file manager).
 * For internal app drags (from file list), use the sentinel/* MIME types instead.
 */
export function useExternalFileDrop(
  options: UseExternalFileDropOptions
): UseExternalFileDropReturn {
  const { onDrop, onEnter, onLeave, enabled = true } = options;

  const [isDraggingExternal, setIsDraggingExternal] = useState(false);
  const [pendingPaths, setPendingPaths] = useState<string[]>([]);
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null);

  // Use refs to avoid stale closures in the event handler
  const onDropRef = useRef(onDrop);
  const onEnterRef = useRef(onEnter);
  const onLeaveRef = useRef(onLeave);

  useEffect(() => {
    onDropRef.current = onDrop;
    onEnterRef.current = onEnter;
    onLeaveRef.current = onLeave;
  }, [onDrop, onEnter, onLeave]);

  useEffect(() => {
    if (!enabled) {
      // eslint-disable-next-line react-hooks/set-state-in-effect -- Reset state when disabled
      setIsDraggingExternal(false);
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setPendingPaths([]);
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setPosition(null);
      return;
    }

    let unlisten: (() => void) | undefined;

    const setupListener = async () => {
      try {
        const currentWindow = getCurrentWindow();

        unlisten = await currentWindow.onDragDropEvent((event) => {
          // event.payload contains the actual DragDropEvent data
          const payload = event.payload;

          switch (payload.type) {
            case 'enter':
              // Drag entered the window
              setIsDraggingExternal(true);
              if (payload.paths) {
                setPendingPaths(payload.paths);
                onEnterRef.current?.(payload.paths);
              }
              break;

            case 'over':
              // Drag is moving over the window
              if (payload.position) {
                setPosition({
                  x: payload.position.x,
                  y: payload.position.y,
                });
              }
              break;

            case 'drop':
              // Files were dropped
              setIsDraggingExternal(false);
              if (payload.paths && payload.paths.length > 0) {
                onDropRef.current(payload.paths);
              }
              setPendingPaths([]);
              setPosition(null);
              break;

            case 'leave':
              // Drag left the window without dropping
              setIsDraggingExternal(false);
              setPendingPaths([]);
              setPosition(null);
              onLeaveRef.current?.();
              break;
          }
        });
      } catch (error) {
        console.error('[useExternalFileDrop] Failed to setup listener:', error);
      }
    };

    setupListener();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [enabled]);

  return {
    isDraggingExternal,
    pendingPaths,
    position,
  };
}
