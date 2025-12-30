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
import { cn, formatFileSize, formatDate, getFileType } from '../../lib/utils';
import { FolderIcon } from '../icons/FolderIcon';
import { InlineNameEditor } from './InlineNameEditor';
import { GhostOverlay, getGhostClasses } from '../ghost';
import type { FileEntry } from '../../types/file';
import type { GhostState } from '../../types/ghost';
import '../ghost/GhostAnimations.css';

interface FileRowProps {
  entry: FileEntry;
  isSelected: boolean;
  isFocused: boolean;
  isEditing?: boolean;
  /** Whether this item is currently a drop target */
  isDragTarget?: boolean;
  /** Whether this is a valid drop target (only relevant when isDragTarget is true) */
  isValidDropTarget?: boolean;
  /** Ghost visualization state */
  ghostState?: GhostState;
  /** For source/destination pairs, the linked path */
  linkedPath?: string;
  style?: React.CSSProperties;
  onClick: (e: React.MouseEvent) => void;
  onDoubleClick: () => void;
  onContextMenu?: (e: React.MouseEvent) => void;
  onRenameConfirm?: (newName: string) => void;
  onRenameCancel?: () => void;
  /** Called when user starts dragging this item */
  onDragStart?: (e: React.MouseEvent) => void;
  /** Called when dragging enters this item (for drop target highlighting) */
  onDragEnter?: () => void;
  /** Called when user drops on this item */
  onDrop?: () => void;
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

export function FileRow({
  entry,
  isSelected,
  isFocused,
  isEditing = false,
  isDragTarget = false,
  isValidDropTarget = true,
  ghostState = 'normal',
  linkedPath,
  style,
  onClick,
  onDoubleClick,
  onContextMenu,
  onRenameConfirm,
  onRenameCancel,
  onDragStart,
  onDragEnter,
  onDrop,
}: FileRowProps) {
  const Icon = getFileIcon(entry);
  const ghostClasses = getGhostClasses(ghostState);

  // Handle mouse down for drag initiation
  const handleMouseDown = (e: React.MouseEvent) => {
    // Only initiate drag on left click, not during editing
    if (e.button !== 0 || isEditing) return;

    // Track if this will become a drag
    const startX = e.clientX;
    const startY = e.clientY;
    const threshold = 5; // pixels to move before considering it a drag

    const handleMouseMove = (moveEvent: MouseEvent) => {
      const deltaX = Math.abs(moveEvent.clientX - startX);
      const deltaY = Math.abs(moveEvent.clientY - startY);

      if (deltaX > threshold || deltaY > threshold) {
        // This is a drag, not a click
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);
        onDragStart?.(e);
      }
    };

    const handleMouseUp = () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  };

  // Handle HTML5 drag start for chat panel context
  const handleDragStartHTML5 = (e: React.DragEvent) => {
    // Set sentinel custom MIME types for chat panel
    e.dataTransfer.setData('sentinel/path', entry.path);
    e.dataTransfer.setData('sentinel/type', entry.isDirectory ? 'folder' : 'file');
    e.dataTransfer.setData('sentinel/name', entry.name);
    e.dataTransfer.setData('sentinel/size', String(entry.size || 0));
    if (entry.mimeType) {
      e.dataTransfer.setData('sentinel/mime', entry.mimeType);
    }
    e.dataTransfer.effectAllowed = 'copyLink';
  };

  // Handle mouse enter for drop target detection
  const handleMouseEnter = () => {
    if (onDragEnter && entry.isDirectory) {
      onDragEnter();
    }
  };

  // Handle mouse up for drop
  const handleMouseUp = () => {
    if (onDrop && entry.isDirectory) {
      onDrop();
    }
  };

  return (
    <div
      style={style}
      draggable={!isEditing}
      onDragStart={handleDragStartHTML5}
      onMouseDown={handleMouseDown}
      onMouseEnter={handleMouseEnter}
      onMouseUp={handleMouseUp}
      onClick={isEditing ? undefined : onClick}
      onDoubleClick={isEditing ? undefined : onDoubleClick}
      onContextMenu={isEditing ? undefined : onContextMenu}
      className={cn(
        'group relative flex items-center gap-3 px-4 cursor-default select-none',
        'transition-colors duration-75',
        isSelected && isFocused && 'bg-[color:var(--color-file-selected-focused)]',
        isSelected && !isFocused && 'bg-[color:var(--color-file-selected)]',
        !isSelected && !isEditing && !isDragTarget && ghostState === 'normal' && 'hover:bg-[color:var(--color-file-hover)]',
        // Drop target highlighting
        isDragTarget && isValidDropTarget && 'ring-2 ring-orange-500 bg-orange-500/10',
        isDragTarget && !isValidDropTarget && 'ring-2 ring-red-500 bg-red-500/10',
        // Ghost state styling
        ghostClasses
      )}
    >
      {/* Icon */}
      {entry.isDirectory ? (
        <FolderIcon size={18} className="flex-shrink-0" />
      ) : (
        Icon && <Icon size={18} className="text-gray-400 dark:text-gray-500" />
      )}

      {/* Name - either editable or static */}
      {isEditing && onRenameConfirm && onRenameCancel ? (
        <InlineNameEditor
          initialValue={entry.name}
          onConfirm={onRenameConfirm}
          onCancel={onRenameCancel}
          selectNameOnly={!entry.isDirectory}
          className="flex-1"
        />
      ) : (
        <span
          className={cn(
            'flex-1 truncate text-sm text-gray-800 dark:text-gray-200',
            entry.isHidden && 'text-gray-400 dark:text-gray-500'
          )}
        >
          {entry.name}
        </span>
      )}

      {/* Modified date */}
      <span className="w-36 text-xs text-gray-500 dark:text-gray-500 truncate">
        {formatDate(entry.modifiedAt)}
      </span>

      {/* Size */}
      <span className="w-20 text-xs text-gray-500 dark:text-gray-500 text-right tabular-nums">
        {entry.isFile ? formatFileSize(entry.size) : '—'}
      </span>

      {/* Ghost overlay for state indicators */}
      {ghostState !== 'normal' && (
        <GhostOverlay ghostState={ghostState} linkedPath={linkedPath} />
      )}
    </div>
  );
}
