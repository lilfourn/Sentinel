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
import { useMarqueeSelection } from '../../hooks/useMarqueeSelection';
import { cn, getFileType } from '../../lib/utils';
import type { FileEntry } from '../../types/file';

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

  // Update item positions when entries change
  useEffect(() => {
    if (!columnsRef.current) return;

    const items = Array.from(columnsRef.current.querySelectorAll('[data-path]')).map((el) => ({
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

    if (e.target === e.currentTarget || e.target === columnsRef.current) {
      clearSelection();
      setContextMenu(null);
    }
  }, [clearSelection, justFinishedDragging]);

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
      {/* CSS Columns layout - items flow vertically then wrap to next column */}
      <div ref={columnsRef} className="columns-2 sm:columns-3 md:columns-4 lg:columns-5 xl:columns-6 gap-1">
        {entries.map((entry) => {
          const Icon = getFileIcon(entry);
          const isSelected = selectedPaths.has(entry.path);

          return (
            <div
              key={entry.path}
              data-path={entry.path}
              onClick={(e) => handleClick(entry, e)}
              onDoubleClick={() => handleDoubleClick(entry)}
              onContextMenu={(e) => handleContextMenu(entry, e)}
              className={cn(
                'flex items-center gap-2 px-2 py-1 rounded cursor-default select-none',
                'break-inside-avoid mb-0.5',
                'transition-colors duration-75',
                isSelected && 'bg-orange-500/20',
                !isSelected && 'hover:bg-gray-500/10'
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
