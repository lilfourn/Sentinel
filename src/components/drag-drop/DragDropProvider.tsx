import { createContext, useContext, type ReactNode } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useDragDrop } from '../../hooks/useDragDrop';
import { useSelectionStore } from '../../stores/selection-store';
import { showError } from '../../stores/toast-store';
import { DROP_INVALID_MESSAGES } from '../../types/drag-drop';
import type { DragState, DropTarget } from '../../types/drag-drop';

interface DragDropContextValue {
  /** Current drag state (null if not dragging) */
  dragState: DragState | null;
  /** Currently hovered drop target */
  dropTarget: DropTarget | null;
  /** Start dragging items */
  startDrag: (items: import('../../types/file').FileEntry[], sourceDirectory: string) => void;
  /** Set current drop target */
  setDropTarget: (path: string | null, isDirectory: boolean) => void;
  /** Execute the drop. Pass targetPath to override state. */
  executeDrop: (targetPath?: string) => Promise<boolean>;
  /** Cancel the drag */
  cancelDrag: () => void;
  /** Set copy mode (Alt key held) */
  setCopyMode: (isCopy: boolean) => void;
  /** Check if currently dragging */
  isDragging: boolean;
  /** Check if current drop target is valid */
  isValidTarget: boolean;
}

const DragDropContext = createContext<DragDropContextValue | null>(null);

interface DragDropProviderProps {
  children: ReactNode;
}

export function DragDropProvider({ children }: DragDropProviderProps) {
  const queryClient = useQueryClient();
  const clearSelection = useSelectionStore((state) => state.clearSelection);

  const dragDrop = useDragDrop({
    onDropComplete: (newPaths, isCopy) => {
      console.log('[DragDropProvider] onDropComplete:', { newPaths, isCopy });
      // Invalidate directory queries to refresh the views
      queryClient.invalidateQueries({ queryKey: ['directory'] });
    },
    onDropError: (error) => {
      console.error('[DragDropProvider] onDropError:', error);

      // Parse error and show user-friendly toast
      let message = 'Failed to complete the operation';

      if (typeof error === 'string') {
        // Check for known error patterns in string errors
        if (error.toLowerCase().includes('cycle')) {
          message = DROP_INVALID_MESSAGES.cycle_descendant;
        } else if (error.toLowerCase().includes('protected')) {
          message = DROP_INVALID_MESSAGES.protected_path;
        } else if (error.toLowerCase().includes('exists')) {
          message = DROP_INVALID_MESSAGES.name_collision;
        } else if (error.toLowerCase().includes('symlink')) {
          message = DROP_INVALID_MESSAGES.symlink_loop;
        } else {
          message = error;
        }
      } else if (error && typeof error === 'object') {
        // Handle structured error objects from Rust backend
        const typedError = error as { type?: string; message?: string };
        if (typedError.type) {
          const typeToMessage: Record<string, string> = {
            CYCLE_DETECTED_SELF: DROP_INVALID_MESSAGES.cycle_self,
            CYCLE_DETECTED_DESCENDANT: DROP_INVALID_MESSAGES.cycle_descendant,
            TARGET_IS_SELECTED: DROP_INVALID_MESSAGES.target_selected,
            NAME_COLLISION: DROP_INVALID_MESSAGES.name_collision,
            PERMISSION_DENIED: DROP_INVALID_MESSAGES.permission_denied,
            PROTECTED_PATH: DROP_INVALID_MESSAGES.protected_path,
            SYMLINK_LOOP: DROP_INVALID_MESSAGES.symlink_loop,
          };
          message = typeToMessage[typedError.type] || typedError.message || message;
        } else if (typedError.message) {
          message = typedError.message;
        }
      }

      showError('Drop Failed', message);
    },
    onDragCancel: () => {
      console.log('[DragDropProvider] onDragCancel');
      // Clear selection when drag is cancelled (dropped on blank space)
      clearSelection();
    },
  });

  const contextValue: DragDropContextValue = {
    dragState: dragDrop.dragState,
    dropTarget: dragDrop.dropTarget,
    startDrag: dragDrop.startDrag,
    setDropTarget: dragDrop.setDropTarget,
    executeDrop: dragDrop.executeDrop,
    cancelDrag: dragDrop.cancelDrag,
    setCopyMode: dragDrop.setCopyMode,
    isDragging: dragDrop.isDragging,
    isValidTarget: dragDrop.isValidTarget,
  };

  // Native HTML5 drag uses setDragImage() instead of a React overlay
  return (
    <DragDropContext.Provider value={contextValue}>
      {children}
    </DragDropContext.Provider>
  );
}

export function useDragDropContext(): DragDropContextValue {
  const context = useContext(DragDropContext);
  if (!context) {
    throw new Error('useDragDropContext must be used within DragDropProvider');
  }
  return context;
}
