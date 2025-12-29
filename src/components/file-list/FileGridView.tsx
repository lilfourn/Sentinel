import { useCallback, useState, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
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
import { ContextMenu, buildFileContextMenuItems, type ContextMenuPosition } from '../ContextMenu/ContextMenu';
import { FolderIcon } from '../icons/FolderIcon';
import { SelectionOverlay } from './SelectionOverlay';
import { useNavigationStore } from '../../stores/navigation-store';
import { useSelectionStore } from '../../stores/selection-store';
import { useOrganizeStore } from '../../stores/organize-store';
import { showSuccess, showError } from '../../stores/toast-store';
import { useThumbnail } from '../../hooks/useThumbnail';
import { useMarqueeSelection } from '../../hooks/useMarqueeSelection';
import { cn, getFileType, isThumbnailSupported } from '../../lib/utils';
import type { FileEntry } from '../../types/file';

interface FileGridViewProps {
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

// Separate component for grid items to enable hook usage
interface FileGridItemProps {
  entry: FileEntry;
  isSelected: boolean;
  onClick: (e: React.MouseEvent) => void;
  onDoubleClick: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
}

function FileGridItem({
  entry,
  isSelected,
  onClick,
  onDoubleClick,
  onContextMenu,
}: FileGridItemProps) {
  const Icon = getFileIcon(entry);
  const supportsThumbnail = !entry.isDirectory && isThumbnailSupported(entry.extension);
  const { thumbnail, loading } = useThumbnail(
    supportsThumbnail ? entry.path : null,
    entry.extension,
    96
  );

  return (
    <div
      data-path={entry.path}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onContextMenu={onContextMenu}
      className={cn(
        'flex flex-col items-center p-2 rounded-lg cursor-default select-none',
        'transition-colors duration-75',
        isSelected && 'bg-orange-500/20',
        !isSelected && 'hover:bg-gray-500/10'
      )}
    >
      {/* Icon/Thumbnail area */}
      <div className="w-12 h-12 mb-1 flex items-center justify-center flex-shrink-0">
        {entry.isDirectory ? (
          <FolderIcon size={48} />
        ) : thumbnail ? (
          <img
            src={`data:image/png;base64,${thumbnail}`}
            alt={entry.name}
            className="max-w-full max-h-full object-contain rounded"
          />
        ) : loading && supportsThumbnail ? (
          <div className="w-10 h-10 bg-gray-200 dark:bg-gray-700 rounded animate-pulse" />
        ) : (
          Icon && <Icon size={48} className="text-gray-400 dark:text-gray-500" />
        )}
      </div>

      {/* Filename */}
      <span
        className={cn(
          'text-xs text-center line-clamp-2 break-all w-full',
          'text-gray-800 dark:text-gray-200',
          entry.isHidden && 'text-gray-400 dark:text-gray-500'
        )}
        title={entry.name}
      >
        {entry.name}
      </span>
    </div>
  );
}

export function FileGridView({ entries }: FileGridViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const { navigateTo, setQuickLookPath } = useNavigationStore();
  const {
    selectedPaths,
    select,
    selectRange,
    clearSelection,
  } = useSelectionStore();
  const { startOrganize } = useOrganizeStore();

  // Marquee selection
  const {
    isDragging,
    justFinishedDragging,
    selectionRect,
    startDrag,
    updateItemPositions,
  } = useMarqueeSelection(containerRef);

  // Update item positions when entries change or when starting drag
  useEffect(() => {
    if (!gridRef.current) return;

    const items = Array.from(gridRef.current.querySelectorAll('[data-path]')).map((el) => ({
      path: el.getAttribute('data-path')!,
      element: el as HTMLElement,
    }));

    updateItemPositions(items);
  }, [entries, updateItemPositions]);

  // Context menu state
  const [contextMenu, setContextMenu] = useState<{
    position: ContextMenuPosition;
    entry: FileEntry;
  } | null>(null);

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
    (entry: FileEntry) => {
      if (entry.isDirectory) {
        navigateTo(entry.path);
      }
    },
    [navigateTo]
  );

  const handleContainerClick = useCallback((e: React.MouseEvent) => {
    // Don't clear selection if we just finished a marquee drag
    if (justFinishedDragging.current) return;

    // Only clear if clicking directly on container (not bubbled from child)
    if (e.target === e.currentTarget || e.target === gridRef.current) {
      clearSelection();
      setContextMenu(null);
    }
  }, [clearSelection, justFinishedDragging]);

  const handleContainerMouseDown = useCallback((e: React.MouseEvent) => {
    // Start marquee selection if clicking on empty space (container or grid, not items)
    const target = e.target as HTMLElement;
    const isContainer = target === containerRef.current || target === gridRef.current;

    if (isContainer && e.button === 0) {
      // Update positions before starting drag
      if (gridRef.current) {
        const items = Array.from(gridRef.current.querySelectorAll('[data-path]')).map((el) => ({
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

  const handleMoveToTrash = useCallback(async (path: string) => {
    try {
      await invoke('delete_to_trash', { path });
      showSuccess('Moved to Trash', path.split('/').pop() || path);
    } catch (error) {
      showError('Failed to move to Trash', String(error));
    }
  }, []);

  return (
    <div
      ref={containerRef}
      onClick={handleContainerClick}
      onMouseDown={handleContainerMouseDown}
      className="relative h-full overflow-auto p-4 focus:outline-none select-none"
      tabIndex={0}
    >
      {/* Grid of items */}
      <div
        ref={gridRef}
        className="grid grid-cols-4 sm:grid-cols-5 md:grid-cols-6 lg:grid-cols-8 xl:grid-cols-10 gap-2"
      >
        {entries.map((entry) => (
          <FileGridItem
            key={entry.path}
            entry={entry}
            isSelected={selectedPaths.has(entry.path)}
            onClick={(e) => handleClick(entry, e)}
            onDoubleClick={() => handleDoubleClick(entry)}
            onContextMenu={(e) => handleContextMenu(entry, e)}
          />
        ))}
      </div>

      {/* Empty state */}
      {entries.length === 0 && (
        <div className="flex items-center justify-center h-32 text-gray-400">
          This folder is empty
        </div>
      )}

      {/* Selection overlay (marquee rectangle) */}
      {isDragging && <SelectionOverlay rect={selectionRect} />}

      {/* Context Menu */}
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
              onOpen: () => {
                if (contextMenu.entry.isDirectory) {
                  navigateTo(contextMenu.entry.path);
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
    </div>
  );
}
