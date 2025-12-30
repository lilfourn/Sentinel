import { useCallback, useState, useRef, useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import {
  File,
  FileText,
  FileImage,
  FileVideo,
  FileAudio,
  FileCode,
  FileArchive,
  FileJson,
  type LucideIcon,
} from 'lucide-react';
import { ContextMenu, buildFileContextMenuItems, buildBackgroundContextMenuItems, type ContextMenuPosition } from '../ContextMenu/ContextMenu';
import { ConfirmDialog } from '../dialogs/ConfirmDialog';
import { ICloudErrorDialog } from '../dialogs/ICloudErrorDialog';
import { useDragDropContext } from '../drag-drop';
import { FolderIcon } from '../icons/FolderIcon';
import { SelectionOverlay } from './SelectionOverlay';
import { GhostOverlay, getGhostClasses } from '../ghost';
import { useNavigationStore } from '../../stores/navigation-store';
import { useSelectionStore } from '../../stores/selection-store';
import { useOrganizeStore } from '../../stores/organize-store';
import { useGhostStore } from '../../stores/ghost-store';
import { useMarqueeSelection } from '../../hooks/useMarqueeSelection';
import { useDelete } from '../../hooks/useDelete';
import { cn, getFileType, openFile } from '../../lib/utils';
import type { FileEntry } from '../../types/file';
import '../ghost/GhostAnimations.css';

interface FileColumnsViewProps {
  entries: FileEntry[];
}

const fileTypeIcons: Record<string, LucideIcon> = {
  image: FileImage,
  video: FileVideo,
  audio: FileAudio,
  code: FileCode,
  config: FileJson,
  text: FileText,
  document: FileText,
  archive: FileArchive,
  unknown: File,
};

function getFileIcon(entry: FileEntry): LucideIcon | null {
  if (entry.isDirectory) return null;
  const fileType = getFileType(entry.extension, entry.mimeType);
  return fileTypeIcons[fileType] || File;
}

export function FileColumnsView({ entries }: FileColumnsViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const columnsRef = useRef<HTMLDivElement>(null);
  const queryClient = useQueryClient();
  const { navigateTo, setQuickLookPath, currentPath, showHidden } = useNavigationStore();
  const {
    selectedPaths,
    select,
    selectRange,
    clearSelection,
    startCreating,
  } = useSelectionStore();
  const { startOrganize } = useOrganizeStore();
  const ghostMap = useGhostStore((state) => state.ghostMap);
  const {
    dropTarget,
    startDrag: startFileDrag,
    setDropTarget,
    executeDrop,
    isDragging: isDragDropActive,
  } = useDragDropContext();

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

  // Marquee selection
  const {
    isDragging,
    justFinishedDragging,
    selectionRect,
    startDrag,
    updateItemPositions,
  } = useMarqueeSelection(containerRef);

  // Update item positions when entries change
  useEffect(() => {
    if (!columnsRef.current) return;

    const items = Array.from(columnsRef.current.querySelectorAll('[data-path]')).map((el) => ({
      path: el.getAttribute('data-path')!,
      element: el as HTMLElement,
    }));

    updateItemPositions(items);
  }, [entries, updateItemPositions]);

  // Context menu state (for file/folder items)
  const [contextMenu, setContextMenu] = useState<{
    position: ContextMenuPosition;
    entry: FileEntry;
  } | null>(null);

  // Background context menu state (for empty space)
  const [backgroundContextMenu, setBackgroundContextMenu] = useState<ContextMenuPosition | null>(null);

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
    if (justFinishedDragging.current) return;

    if (e.target === e.currentTarget || e.target === columnsRef.current) {
      clearSelection();
      setContextMenu(null);
      setBackgroundContextMenu(null);
    }
  }, [clearSelection, justFinishedDragging]);

  // Handle right-click on empty space (background)
  // File items call e.stopPropagation(), so only background clicks reach here
  const handleBackgroundContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setContextMenu(null); // Close file context menu if open
    setBackgroundContextMenu({ x: e.clientX, y: e.clientY });
  }, []);

  const handleContainerMouseDown = useCallback((e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    const isContainer = target === containerRef.current || target === columnsRef.current;

    if (isContainer && e.button === 0) {
      if (columnsRef.current) {
        const items = Array.from(columnsRef.current.querySelectorAll('[data-path]')).map((el) => ({
          path: el.getAttribute('data-path')!,
          element: el as HTMLElement,
        }));
        updateItemPositions(items);
      }
      startDrag(e);
    }
  }, [startDrag, updateItemPositions]);

  const handleContextMenu = useCallback(
    (entry: FileEntry, e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();

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

  // Handle drag start on an item
  const handleDragStart = useCallback(
    (entry: FileEntry, e: React.MouseEvent) => {
      e.preventDefault();
      const itemsToDrag = selectedPaths.has(entry.path)
        ? entries.filter((e) => selectedPaths.has(e.path))
        : [entry];

      startFileDrag(itemsToDrag, currentPath);
    },
    [selectedPaths, entries, currentPath, startFileDrag]
  );

  // Handle drag entering a directory
  const handleDragEnter = useCallback(
    (entry: FileEntry) => {
      if (entry.isDirectory && isDragDropActive) {
        setDropTarget(entry.path, true);
      }
    },
    [isDragDropActive, setDropTarget]
  );

  // Handle dropping on a directory
  const handleDrop = useCallback(
    async (entry: FileEntry) => {
      if (entry.isDirectory && isDragDropActive) {
        setDropTarget(entry.path, true);
        await executeDrop();
      }
    },
    [isDragDropActive, setDropTarget, executeDrop]
  );

  // Clear drop target when mouse leaves
  const handleMouseLeave = useCallback(() => {
    if (isDragDropActive) {
      setDropTarget(null, false);
    }
  }, [isDragDropActive, setDropTarget]);

  return (
    <div
      ref={containerRef}
      onClick={handleContainerClick}
      onMouseDown={handleContainerMouseDown}
      onMouseLeave={handleMouseLeave}
      onContextMenu={handleBackgroundContextMenu}
      className="relative h-full overflow-auto p-4 focus:outline-none select-none"
      tabIndex={0}
    >
      {/* CSS Columns layout - items flow vertically then wrap to next column */}
      <div ref={columnsRef} className="columns-2 sm:columns-3 md:columns-4 lg:columns-5 xl:columns-6 gap-1">
        {entries.map((entry) => {
          const Icon = getFileIcon(entry);
          const isSelected = selectedPaths.has(entry.path);
          const isDragTarget = dropTarget?.path === entry.path;
          const isValidDropTarget = isDragTarget ? dropTarget.isValid : true;
          const ghostInfo = ghostMap.get(entry.path);
          const ghostState = ghostInfo?.state || 'normal';
          const linkedPath = ghostInfo?.linkedPath;
          const ghostClasses = getGhostClasses(ghostState);

          // Drag initiation handler
          const handleMouseDown = (e: React.MouseEvent) => {
            if (e.button !== 0) return;

            const startX = e.clientX;
            const startY = e.clientY;
            const threshold = 5;

            const onMouseMove = (moveEvent: MouseEvent) => {
              const deltaX = Math.abs(moveEvent.clientX - startX);
              const deltaY = Math.abs(moveEvent.clientY - startY);

              if (deltaX > threshold || deltaY > threshold) {
                document.removeEventListener('mousemove', onMouseMove);
                document.removeEventListener('mouseup', onMouseUp);
                handleDragStart(entry, e);
              }
            };

            const onMouseUp = () => {
              document.removeEventListener('mousemove', onMouseMove);
              document.removeEventListener('mouseup', onMouseUp);
            };

            document.addEventListener('mousemove', onMouseMove);
            document.addEventListener('mouseup', onMouseUp);
          };

          return (
            <div
              key={entry.path}
              data-path={entry.path}
              onMouseDown={handleMouseDown}
              onMouseEnter={() => entry.isDirectory && handleDragEnter(entry)}
              onMouseUp={() => entry.isDirectory && handleDrop(entry)}
              onClick={(e) => handleClick(entry, e)}
              onDoubleClick={() => handleDoubleClick(entry)}
              onContextMenu={(e) => handleContextMenu(entry, e)}
              className={cn(
                'group relative flex items-center gap-2 px-2 py-1 rounded cursor-default select-none',
                'break-inside-avoid mb-0.5',
                'transition-colors duration-75',
                isSelected && 'bg-orange-500/20',
                !isSelected && !isDragTarget && ghostState === 'normal' && 'hover:bg-gray-500/10',
                isDragTarget && isValidDropTarget && 'ring-2 ring-orange-500 bg-orange-500/10',
                isDragTarget && !isValidDropTarget && 'ring-2 ring-red-500 bg-red-500/10',
                ghostClasses
              )}
            >
              {entry.isDirectory ? (
                <FolderIcon size={16} className="flex-shrink-0" />
              ) : (
                Icon && <Icon size={16} className="flex-shrink-0 text-gray-400 dark:text-gray-500" />
              )}
              <span
                className={cn(
                  'text-sm truncate',
                  'text-gray-800 dark:text-gray-200',
                  entry.isHidden && 'text-gray-400 dark:text-gray-500'
                )}
                title={entry.name}
              >
                {entry.name}
              </span>
              {ghostState !== 'normal' && (
                <GhostOverlay ghostState={ghostState} linkedPath={linkedPath} />
              )}
            </div>
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
      {isDragging && <SelectionOverlay rect={selectionRect} />}

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
