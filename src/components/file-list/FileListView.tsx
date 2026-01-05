import { useRef, useCallback, useEffect, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { invoke } from '@tauri-apps/api/core';
import { useQueryClient } from '@tanstack/react-query';
import { FileRow } from './FileRow';
import { NewItemRow } from './NewItemRow';
import { SelectionOverlay } from './SelectionOverlay';
import { ContextMenu, buildFileContextMenuItems, buildBackgroundContextMenuItems, type ContextMenuPosition } from '../ContextMenu/ContextMenu';
import { ConfirmDialog } from '../dialogs/ConfirmDialog';
import { ICloudErrorDialog } from '../dialogs/ICloudErrorDialog';
import { useDragDropContext } from '../drag-drop';
import { useNavigationStore } from '../../stores/navigation-store';
import { useSelectionStore } from '../../stores/selection-store';
import { useOrganizeStore } from '../../stores/organize-store';
import { useDelete } from '../../hooks/useDelete';
import { useNativeDrag } from '../../hooks/useNativeDrag';
import { createDragGhost } from '../../lib/drag-visuals';
import { openFile } from '../../lib/utils';
import type { FileEntry } from '../../types/file';
import type { SelectionRect } from '../../hooks/useMarqueeSelection';

// Re-export for type compatibility
export type { FileEntry };

interface FileListViewProps {
  entries: FileEntry[];
}

const ROW_HEIGHT = 28;
const HEADER_HEIGHT = 30;

export function FileListView({ entries }: FileListViewProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const queryClient = useQueryClient();
  const { navigateTo, setQuickLookPath, currentPath, showHidden } = useNavigationStore();
  const {
    selectedPaths,
    focusedPath,
    select,
    selectRange,
    clearSelection,
    selectMultiple,
    editingPath,
    creatingType,
    creatingInPath,
    startEditing,
    stopEditing,
    startCreating,
    stopCreating,
  } = useSelectionStore();
  const { startOrganize } = useOrganizeStore();
  const {
    dropTarget,
    startDrag,
    setDropTarget,
    executeDrop,
    isDragging: isDragDropActive,
  } = useDragDropContext();

  // Native drag hooks for spring loading and anti-flicker
  const {
    handleDragEnter: nativeDragEnter,
    handleDragLeave: nativeDragLeave,
    handleDrop: nativeDragDrop,
    dragCounter,
  } = useNativeDrag();

  // Check if we should show the new item row in this directory
  const isCreatingHere = creatingType !== null && creatingInPath === currentPath;

  // Marquee selection state
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const [dragCurrent, setDragCurrent] = useState({ x: 0, y: 0 });
  const [dragModifiers, setDragModifiers] = useState({ meta: false, shift: false });
  const justFinishedDraggingRef = useRef(false);

  // Context menu state (for file/folder items)
  const [contextMenu, setContextMenu] = useState<{
    position: ContextMenuPosition;
    entry: FileEntry;
  } | null>(null);

  // Background context menu state (for empty space)
  const [backgroundContextMenu, setBackgroundContextMenu] = useState<ContextMenuPosition | null>(null);

  // Total count includes the new item row if we're creating
  const totalCount = entries.length + (isCreatingHere ? 1 : 0);

  const virtualizer = useVirtualizer({
    count: totalCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  // Refresh directory after changes
  const refreshDirectory = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: ['directory', currentPath, showHidden] });
  }, [queryClient, currentPath, showHidden]);

  // Delete with confirmation and iCloud error handling
  const {
    requestDelete,
    confirmDelete,
    cancelDelete,
    closeICloudError,
    useQuarantineFallback,
    showConfirmation,
    showICloudError,
    pendingName,
    iCloudFileName,
  } = useDelete(refreshDirectory);

  // Handle rename confirmation
  const handleRenameConfirm = useCallback(
    async (oldPath: string, newName: string) => {
      const parentDir = oldPath.substring(0, oldPath.lastIndexOf('/'));
      const newPath = `${parentDir}/${newName}`;

      try {
        await invoke('rename_file', { oldPath, newPath });
        stopEditing();
        refreshDirectory();
        // Select the renamed item
        select(newPath, false);
      } catch (error) {
        console.error('Rename failed:', error);
      }
    },
    [stopEditing, refreshDirectory, select]
  );

  // Handle creating a new file or folder
  const handleCreateConfirm = useCallback(
    async (name: string) => {
      if (!creatingType || !creatingInPath) return;

      const newPath = `${creatingInPath}/${name}`;

      try {
        if (creatingType === 'folder') {
          await invoke('create_directory', { path: newPath });
        } else {
          await invoke('create_file', { path: newPath });
        }
        stopCreating();
        refreshDirectory();
        // Select the new item after a brief delay to let the directory refresh
        setTimeout(() => select(newPath, false), 100);
      } catch (error) {
        console.error(`Failed to create ${creatingType}:`, error);
      }
    },
    [creatingType, creatingInPath, stopCreating, refreshDirectory, select]
  );

  // Handle cancel
  const handleCreateCancel = useCallback(() => {
    stopCreating();
  }, [stopCreating]);

  const handleRenameCancel = useCallback(() => {
    stopEditing();
  }, [stopEditing]);

  // Calculate selection rectangle
  const getSelectionRect = useCallback((): SelectionRect | null => {
    if (!isDragging || !parentRef.current) return null;

    const container = parentRef.current;
    const containerRect = container.getBoundingClientRect();
    const scrollTop = container.scrollTop;

    const startX = dragStart.x - containerRect.left;
    const startY = dragStart.y - containerRect.top + scrollTop - HEADER_HEIGHT;
    const currentX = dragCurrent.x - containerRect.left;
    const currentY = dragCurrent.y - containerRect.top + scrollTop - HEADER_HEIGHT;

    return {
      x: Math.min(startX, currentX),
      y: Math.min(startY, currentY),
      width: Math.abs(currentX - startX),
      height: Math.abs(currentY - startY),
    };
  }, [isDragging, dragStart, dragCurrent]);

  // Get paths of items that intersect with selection rectangle (mathematical approach for virtualized list)
  const getIntersectingPaths = useCallback((): string[] => {
    const rect = getSelectionRect();
    if (!rect) return [];

    const paths: string[] = [];
    const minRowIndex = Math.floor(rect.y / ROW_HEIGHT);
    const maxRowIndex = Math.ceil((rect.y + rect.height) / ROW_HEIGHT);

    for (let i = Math.max(0, minRowIndex); i < Math.min(entries.length, maxRowIndex); i++) {
      paths.push(entries[i].path);
    }

    return paths;
  }, [getSelectionRect, entries]);

  const handleClick = useCallback(
    (entry: FileEntry, e: React.MouseEvent) => {
      e.stopPropagation();

      if (e.shiftKey) {
        selectRange(entry.path, entries);
      } else if (e.metaKey || e.ctrlKey) {
        select(entry.path, true);
      } else {
        select(entry.path, false);
      }

      // Update quick look path if active
      setQuickLookPath(entry.path);
    },
    [entries, select, selectRange, setQuickLookPath]
  );

  const handleDoubleClick = useCallback(
    async (entry: FileEntry) => {
      if (entry.isDirectory) {
        navigateTo(entry.path);
      } else {
        await openFile(entry.path);
      }
    },
    [navigateTo]
  );

  const handleContainerClick = useCallback((e: React.MouseEvent) => {
    // Don't clear selection if we just finished a marquee drag
    if (justFinishedDraggingRef.current) return;

    if (e.target === e.currentTarget) {
      clearSelection();
      setContextMenu(null);
      setBackgroundContextMenu(null);
    }
  }, [clearSelection]);

  // Handle right-click on empty space (background)
  // File items call e.stopPropagation(), so only background clicks reach here
  const handleBackgroundContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setContextMenu(null); // Close file context menu if open
    setBackgroundContextMenu({ x: e.clientX, y: e.clientY });
  }, []);

  const handleContainerMouseDown = useCallback((e: React.MouseEvent) => {
    // Only start drag on left button, on container background
    const target = e.target as HTMLElement;
    const isListArea = target === parentRef.current ||
      target.classList.contains('virtual-list-inner');

    if (isListArea && e.button === 0) {
      setDragModifiers({
        meta: e.metaKey || e.ctrlKey,
        shift: e.shiftKey,
      });
      setDragStart({ x: e.clientX, y: e.clientY });
      setDragCurrent({ x: e.clientX, y: e.clientY });
      setIsDragging(true);

      if (!e.metaKey && !e.ctrlKey && !e.shiftKey) {
        clearSelection();
      }
    }
  }, [clearSelection]);

  // Global mouse event handlers for drag
  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      setDragCurrent({ x: e.clientX, y: e.clientY });
    };

    const handleMouseUp = () => {
      const paths = getIntersectingPaths();
      if (paths.length > 0) {
        selectMultiple(paths, dragModifiers.meta || dragModifiers.shift);
      }

      // Mark that we just finished dragging - prevents click handler from clearing selection
      justFinishedDraggingRef.current = true;
      setTimeout(() => {
        justFinishedDraggingRef.current = false;
      }, 0);

      setIsDragging(false);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging, getIntersectingPaths, selectMultiple, dragModifiers]);

  const handleContextMenu = useCallback(
    (entry: FileEntry, e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();

      // Select the item if not already selected
      if (!selectedPaths.has(entry.path)) {
        select(entry.path, false);
      }

      setContextMenu({
        position: { x: e.clientX, y: e.clientY },
        entry,
      });
    },
    [selectedPaths, select]
  );

  // Wrapper for context menu usage
  const handleMoveToTrash = useCallback((path: string) => {
    requestDelete(path);
  }, [requestDelete]);

  // Handle native HTML5 drag start on an item
  const handleDragStart = useCallback(
    (entry: FileEntry, e: React.DragEvent) => {
      // Reset drag counter to fix stale state from cancelled/interrupted drags
      dragCounter.current = 0;

      // Get all selected items, or just this one if not selected
      const itemsToDrag = selectedPaths.has(entry.path)
        ? entries.filter((ent) => selectedPaths.has(ent.path))
        : [entry];

      // 1. Set sentinel/* data for chat panel compatibility
      e.dataTransfer.setData('sentinel/path', entry.path);
      e.dataTransfer.setData('sentinel/type', entry.isDirectory ? 'folder' : 'file');
      e.dataTransfer.setData('sentinel/name', entry.name);
      e.dataTransfer.setData('sentinel/size', String(entry.size || 0));
      if (entry.mimeType) {
        e.dataTransfer.setData('sentinel/mime', entry.mimeType);
      }

      // 2. Set text/uri-list for external app support (Finder, VS Code, etc.)
      const uriList = itemsToDrag.map((f) => `file://${f.path}`).join('\r\n');
      e.dataTransfer.setData('text/uri-list', uriList);
      e.dataTransfer.setData('text/plain', uriList);

      // 3. Set internal app data (for multi-file operations)
      e.dataTransfer.setData('sentinel/json', JSON.stringify(itemsToDrag.map((f) => f.path)));
      e.dataTransfer.effectAllowed = 'copyMove';

      // 4. Create native drag image (stacked cards)
      const ghost = createDragGhost(itemsToDrag);
      document.body.appendChild(ghost);
      e.dataTransfer.setDragImage(ghost, 16, 16);
      // Cleanup after browser captures the image
      requestAnimationFrame(() => ghost.remove());

      // 5. Start drag in context (for validation and execution)
      startDrag(itemsToDrag, currentPath);
    },
    [selectedPaths, entries, currentPath, startDrag]
  );

  // Handle native drag entering a directory (for drop target highlighting)
  const handleDragEnter = useCallback(
    (entry: FileEntry, e: React.DragEvent) => {
      e.preventDefault();
      nativeDragEnter(entry.path, entry.isDirectory);

      if (entry.isDirectory && dragCounter.current === 1) {
        setDropTarget(entry.path, true);
      }
    },
    [nativeDragEnter, dragCounter, setDropTarget]
  );

  // Handle native drag over (required to allow drop)
  const handleDragOver = useCallback(
    (_entry: FileEntry, e: React.DragEvent) => {
      e.preventDefault();
      // Set copy/move based on Alt key
      e.dataTransfer.dropEffect = e.altKey ? 'copy' : 'move';
    },
    []
  );

  // Handle native drag leave
  const handleDragLeave = useCallback(
    (entry: FileEntry, _e: React.DragEvent) => {
      nativeDragLeave();

      if (entry.isDirectory && dragCounter.current === 0) {
        setDropTarget(null, false);
      }
    },
    [nativeDragLeave, dragCounter, setDropTarget]
  );

  // Handle dropping on a directory
  const handleDrop = useCallback(
    async (entry: FileEntry, e: React.DragEvent) => {
      e.preventDefault();
      nativeDragDrop();

      if (entry.isDirectory) {
        // Pass target path directly to avoid async state timing issues
        await executeDrop(entry.path);
        setDropTarget(null, false);
      }
    },
    [nativeDragDrop, executeDrop, setDropTarget]
  );

  // Clear drop target when mouse leaves the list area
  const handleMouseLeave = useCallback(() => {
    if (isDragDropActive) {
      setDropTarget(null, false);
    }
  }, [isDragDropActive, setDropTarget]);

  // Keyboard navigation and shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't handle keys if we're editing
      if (editingPath || isCreatingHere) {
        if (e.key === 'Escape') {
          e.preventDefault();
          stopEditing();
          stopCreating();
        }
        return;
      }

      const currentIndex = entries.findIndex((entry) => entry.path === focusedPath);

      // New Folder: Cmd+Shift+N
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === 'N') {
        e.preventDefault();
        useSelectionStore.getState().startCreating('folder', currentPath);
        return;
      }

      // New File: Cmd+N (without shift)
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === 'n') {
        e.preventDefault();
        useSelectionStore.getState().startCreating('file', currentPath);
        return;
      }

      if (!entries.length) return;

      switch (e.key) {
        case 'ArrowDown': {
          e.preventDefault();
          const nextIndex = Math.min(currentIndex + 1, entries.length - 1);
          const nextEntry = entries[nextIndex];
          if (nextEntry) {
            if (e.shiftKey) {
              selectRange(nextEntry.path, entries);
            } else {
              select(nextEntry.path, false);
            }
            virtualizer.scrollToIndex(nextIndex, { align: 'auto' });
          }
          break;
        }
        case 'ArrowUp': {
          e.preventDefault();
          const prevIndex = Math.max(currentIndex - 1, 0);
          const prevEntry = entries[prevIndex];
          if (prevEntry) {
            if (e.shiftKey) {
              selectRange(prevEntry.path, entries);
            } else {
              select(prevEntry.path, false);
            }
            virtualizer.scrollToIndex(prevIndex, { align: 'auto' });
          }
          break;
        }
        case 'Enter': {
          e.preventDefault();
          const currentEntry = entries[currentIndex];
          if (!currentEntry) break;

          // If it's a folder and only one selected, navigate into it
          if (currentEntry.isDirectory && selectedPaths.size === 1) {
            navigateTo(currentEntry.path);
          } else if (selectedPaths.size === 1) {
            // Single file selected - start renaming
            startEditing(currentEntry.path);
          }
          break;
        }
        case 'F2': {
          // F2 to rename (Windows/Linux convention)
          e.preventDefault();
          if (selectedPaths.size === 1 && focusedPath) {
            startEditing(focusedPath);
          }
          break;
        }
        case 'Backspace':
        case 'Delete': {
          // Cmd+Backspace to delete
          if (e.metaKey || e.ctrlKey) {
            e.preventDefault();
            const selectedEntries = entries.filter((entry) => selectedPaths.has(entry.path));
            selectedEntries.forEach((entry) => handleMoveToTrash(entry.path));
          }
          break;
        }
        case 'a':
          if (e.metaKey || e.ctrlKey) {
            e.preventDefault();
            const allPaths = entries.map((entry) => entry.path);
            selectMultiple(allPaths, false);
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [entries, focusedPath, select, selectRange, selectMultiple, navigateTo, virtualizer, editingPath, isCreatingHere, selectedPaths, startEditing, stopEditing, stopCreating, currentPath, handleMoveToTrash]);

  const selectionRect = getSelectionRect();

  return (
    <div
      ref={parentRef}
      onClick={handleContainerClick}
      onMouseDown={handleContainerMouseDown}
      onMouseLeave={handleMouseLeave}
      onContextMenu={handleBackgroundContextMenu}
      className="relative h-full overflow-auto focus:outline-none select-none"
      tabIndex={0}
    >
      {/* Header */}
      <div className="sticky top-0 z-10 flex items-center gap-3 px-4 py-1.5 glass-file-header border-b border-gray-200/20 text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">
        <span className="w-[18px]" /> {/* Icon space */}
        <span className="flex-1">Name</span>
        <span className="w-36">Date Modified</span>
        <span className="w-20 text-right">Size</span>
      </div>

      {/* Virtualized list */}
      <div
        className="virtual-list-inner"
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          // If creating and this is the first row, show the new item row
          if (isCreatingHere && virtualRow.index === 0) {
            return (
              <NewItemRow
                key="__new_item__"
                type={creatingType!}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  height: `${virtualRow.size}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
                onConfirm={handleCreateConfirm}
                onCancel={handleCreateCancel}
              />
            );
          }

          // Adjust index if we have a new item row
          const entryIndex = isCreatingHere ? virtualRow.index - 1 : virtualRow.index;
          const entry = entries[entryIndex];

          if (!entry) return null;

          return (
            <FileRow
              key={entry.path}
              entry={entry}
              isSelected={selectedPaths.has(entry.path)}
              isFocused={focusedPath === entry.path}
              isEditing={editingPath === entry.path}
              isDragTarget={dropTarget?.path === entry.path}
              isValidDropTarget={dropTarget?.path === entry.path ? dropTarget.isValid : true}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
              }}
              onClick={(e) => handleClick(entry, e)}
              onDoubleClick={() => handleDoubleClick(entry)}
              onContextMenu={(e) => handleContextMenu(entry, e)}
              onRenameConfirm={(newName) => handleRenameConfirm(entry.path, newName)}
              onRenameCancel={handleRenameCancel}
              onDragStart={(e) => handleDragStart(entry, e)}
              onDragEnter={(e) => handleDragEnter(entry, e)}
              onDragOver={(e) => handleDragOver(entry, e)}
              onDragLeave={(e) => handleDragLeave(entry, e)}
              onDrop={(e) => handleDrop(entry, e)}
            />
          );
        })}
      </div>

      {/* Empty state */}
      {entries.length === 0 && (
        <div className="flex items-center justify-center h-32 text-gray-400">
          This folder is empty
        </div>
      )}

      {/* Selection overlay (marquee rectangle) */}
      {isDragging && selectionRect && (
        <SelectionOverlay
          rect={{
            ...selectionRect,
            y: selectionRect.y + HEADER_HEIGHT, // Adjust for header
          }}
        />
      )}

      {/* Context Menu (for files/folders) */}
      {contextMenu && (
        <ContextMenu
          position={contextMenu.position}
          items={buildFileContextMenuItems(
            {
              name: contextMenu.entry.name,
              path: contextMenu.entry.path,
              isDirectory: contextMenu.entry.isDirectory,
            },
            {
              onOpen: async () => {
                if (contextMenu.entry.isDirectory) {
                  navigateTo(contextMenu.entry.path);
                } else {
                  await openFile(contextMenu.entry.path);
                }
              },
              onOrganizeWithAI: () => {
                startOrganize(contextMenu.entry.path);
              },
              onRename: () => {
                startEditing(contextMenu.entry.path);
              },
              onMoveToTrash: () => {
                handleMoveToTrash(contextMenu.entry.path);
              },
            }
          )}
          onClose={() => setContextMenu(null)}
        />
      )}

      {/* Background Context Menu (for empty space) */}
      {backgroundContextMenu && (
        <ContextMenu
          position={backgroundContextMenu}
          items={buildBackgroundContextMenuItems({
            onNewFolder: () => {
              startCreating('folder', currentPath);
            },
            onNewFile: () => {
              startCreating('file', currentPath);
            },
          })}
          onClose={() => setBackgroundContextMenu(null)}
        />
      )}

      {/* Delete Confirmation Dialog */}
      <ConfirmDialog
        isOpen={showConfirmation}
        title="Move to Trash?"
        message="This item will be moved to the Trash."
        itemName={pendingName || undefined}
        confirmLabel="Move to Trash"
        variant="danger"
        showDontAskAgain={true}
        onConfirm={confirmDelete}
        onCancel={cancelDelete}
      />

      {/* iCloud Error Dialog */}
      <ICloudErrorDialog
        isOpen={showICloudError}
        fileName={iCloudFileName || ''}
        onClose={closeICloudError}
        onUseQuarantine={useQuarantineFallback}
      />
    </div>
  );
}
