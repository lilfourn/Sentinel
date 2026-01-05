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
  /** Called when native HTML5 drag starts - parent sets up dataTransfer */
  onDragStart?: (e: React.DragEvent) => void;
  /** Called when dragging enters this item (for drop target highlighting) */
  onDragEnter?: (e: React.DragEvent) => void;
  /** Called when dragging over this item */
  onDragOver?: (e: React.DragEvent) => void;
  /** Called when dragging leaves this item */
  onDragLeave?: (e: React.DragEvent) => void;
  /** Called when user drops on this item */
  onDrop?: (e: React.DragEvent) => void;
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
  onDragOver,
  onDragLeave,
  onDrop,
}: FileRowProps) {
  const Icon = getFileIcon(entry);
  const ghostClasses = getGhostClasses(ghostState);

  // Native HTML5 drag start - delegate to parent for full setup
  const handleDragStart = (e: React.DragEvent) => {
    onDragStart?.(e);
  };

  // Native drag enter - only trigger for directories (drop targets)
  const handleDragEnter = (e: React.DragEvent) => {
    if (entry.isDirectory) {
      onDragEnter?.(e);
    }
  };

  // Native drag over - must prevent default to allow drop
  const handleDragOver = (e: React.DragEvent) => {
    if (entry.isDirectory) {
      e.preventDefault();
      e.stopPropagation();
      onDragOver?.(e);
    }
  };

  // Native drag leave
  const handleDragLeave = (e: React.DragEvent) => {
    if (entry.isDirectory) {
      onDragLeave?.(e);
    }
  };

  // Native drop
  const handleDrop = (e: React.DragEvent) => {
    if (entry.isDirectory) {
      e.preventDefault();
      e.stopPropagation();
      onDrop?.(e);
    }
  };

  return (
    <div
      style={style}
      draggable={!isEditing}
      onDragStart={handleDragStart}
      onDragEnter={handleDragEnter}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
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
