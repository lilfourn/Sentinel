import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';
import { invoke } from '@tauri-apps/api/core';

/** Merge Tailwind classes with proper precedence */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Format file size in human-readable format */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return '—';

  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const size = bytes / Math.pow(1024, i);

  return `${size.toFixed(i > 0 ? 1 : 0)} ${units[i]}`;
}

/** Format date in relative or absolute format */
export function formatDate(timestamp: number | null): string {
  if (!timestamp) return '—';

  const date = new Date(timestamp);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffDays === 0) {
    return 'Today, ' + date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  } else if (diffDays === 1) {
    return 'Yesterday, ' + date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  } else if (diffDays < 7) {
    return date.toLocaleDateString([], { weekday: 'long' });
  } else {
    return date.toLocaleDateString([], {
      year: 'numeric',
      month: 'short',
      day: 'numeric'
    });
  }
}

/** Format date in absolute format with time (for photo metadata, etc.) */
export function formatAbsoluteDate(timestamp: number | null): string {
  if (!timestamp) return 'Unknown';
  return new Date(timestamp).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/** Get file type category from extension or mime type */
export function getFileType(extension: string | null, mimeType: string | null): string {
  if (!extension && !mimeType) return 'unknown';

  const ext = extension?.toLowerCase();

  // Images
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico', 'heic'].includes(ext || '')) {
    return 'image';
  }

  // Videos
  if (['mp4', 'mov', 'avi', 'mkv', 'webm', 'wmv', 'flv'].includes(ext || '')) {
    return 'video';
  }

  // Audio
  if (['mp3', 'wav', 'flac', 'aac', 'ogg', 'm4a', 'wma'].includes(ext || '')) {
    return 'audio';
  }

  // Documents
  if (['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'odt', 'ods', 'odp'].includes(ext || '')) {
    return 'document';
  }

  // Code
  if (['js', 'ts', 'jsx', 'tsx', 'py', 'rb', 'go', 'rs', 'java', 'c', 'cpp', 'h', 'hpp', 'cs', 'swift', 'kt'].includes(ext || '')) {
    return 'code';
  }

  // Config/data
  if (['json', 'yaml', 'yml', 'toml', 'xml', 'ini', 'env', 'conf', 'config'].includes(ext || '')) {
    return 'config';
  }

  // Text
  if (['txt', 'md', 'markdown', 'rtf', 'log'].includes(ext || '')) {
    return 'text';
  }

  // Archives
  if (['zip', 'tar', 'gz', 'rar', '7z', 'bz2', 'xz'].includes(ext || '')) {
    return 'archive';
  }

  // Executables
  if (['exe', 'app', 'dmg', 'pkg', 'deb', 'rpm', 'msi'].includes(ext || '')) {
    return 'executable';
  }

  return 'unknown';
}

/** Check if file type is previewable */
export function isPreviewable(extension: string | null, mimeType: string | null): boolean {
  const type = getFileType(extension, mimeType);
  return ['image', 'text', 'code', 'config', 'document'].includes(type);
}

/** Check if file type supports thumbnail generation */
export function isThumbnailSupported(extension: string | null): boolean {
  if (!extension) return false;
  const ext = extension.toLowerCase();
  return [
    // Images
    'jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'ico', 'tiff', 'tif',
    // SVG (vector graphics)
    'svg',
    // Videos (requires ffmpeg)
    'mp4', 'mov', 'avi', 'mkv', 'webm', 'wmv', 'flv',
    // PDF
    'pdf'
  ].includes(ext);
}

/** Open a file with the system's default application */
export async function openFile(path: string): Promise<void> {
  await invoke('open_file', { path });
}
