import { createContext, useContext, type ReactNode } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useDragDrop } from '../../hooks/useDragDrop';
import { DragPreview } from './DragPreview';
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
  /** Execute the drop */
  executeDrop: () => Promise<boolean>;
  /** Cancel the drag */
  cancelDrag: () => void;
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

  const dragDrop = useDragDrop({
    onDropComplete: () => {
      // Invalidate directory queries to refresh the views
      queryClient.invalidateQueries({ queryKey: ['directory'] });
    },
    onDropError: (error) => {
      console.error('Drop failed:', error);
    },
  });

  const contextValue: DragDropContextValue = {
    dragState: dragDrop.dragState,
    dropTarget: dragDrop.dropTarget,
    startDrag: dragDrop.startDrag,
    setDropTarget: dragDrop.setDropTarget,
    executeDrop: dragDrop.executeDrop,
    cancelDrag: dragDrop.cancelDrag,
    isDragging: dragDrop.isDragging,
    isValidTarget: dragDrop.isValidTarget,
  };

  return (
    <DragDropContext.Provider value={contextValue}>
      {children}
      {dragDrop.dragState && <DragPreview dragState={dragDrop.dragState} />}
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
