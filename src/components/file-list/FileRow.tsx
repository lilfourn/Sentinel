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
import type { FileEntry } from '../../types/file';

interface FileRowProps {
  entry: FileEntry;
  isSelected: boolean;
  isFocused: boolean;
  style?: React.CSSProperties;
  onClick: (e: React.MouseEvent) => void;
  onDoubleClick: () => void;
  onContextMenu?: (e: React.MouseEvent) => void;
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
  style,
  onClick,
  onDoubleClick,
  onContextMenu,
}: FileRowProps) {
  const Icon = getFileIcon(entry);

  return (
    <div
      style={style}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onContextMenu={onContextMenu}
      className={cn(
        'flex items-center gap-3 px-4 cursor-default select-none',
        'transition-colors duration-75',
        isSelected && isFocused && 'bg-[color:var(--color-file-selected-focused)]',
        isSelected && !isFocused && 'bg-[color:var(--color-file-selected)]',
        !isSelected && 'hover:bg-[color:var(--color-file-hover)]'
      )}
    >
      {/* Icon */}
      {entry.isDirectory ? (
        <FolderIcon size={18} className="flex-shrink-0" />
      ) : (
        Icon && <Icon size={18} className="text-gray-400 dark:text-gray-500" />
      )}

      {/* Name */}
      <span
        className={cn(
          'flex-1 truncate text-sm text-gray-800 dark:text-gray-200',
          entry.isHidden && 'text-gray-400 dark:text-gray-500'
        )}
      >
        {entry.name}
      </span>

      {/* Modified date */}
      <span className="w-36 text-xs text-gray-500 dark:text-gray-500 truncate">
        {formatDate(entry.modifiedAt)}
      </span>

      {/* Size */}
      <span className="w-20 text-xs text-gray-500 dark:text-gray-500 text-right tabular-nums">
        {entry.isFile ? formatFileSize(entry.size) : '—'}
      </span>
    </div>
  );
}
