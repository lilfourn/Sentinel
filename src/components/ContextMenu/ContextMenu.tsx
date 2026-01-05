import { useEffect, useRef, useState, useLayoutEffect } from 'react';
import { createPortal } from 'react-dom';
import {
  Sparkles,
  Copy,
  Clipboard,
  Trash2,
  Edit3,
  FileText,
  ExternalLink,
  FolderPlus,
  FilePlus,
  Wand2,
} from 'lucide-react';
import { cn } from '../../lib/utils';

export interface ContextMenuPosition {
  x: number;
  y: number;
}

export interface ContextMenuItem {
  id: string;
  label: string;
  icon?: React.ReactNode;
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  separator?: boolean;
  onClick?: () => void;
}

interface ContextMenuProps {
  position: ContextMenuPosition | null;
  items: ContextMenuItem[];
  onClose: () => void;
}

export function ContextMenu({ position, items, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  // Store both the original position and calculated adjustment together
  const [adjustment, setAdjustment] = useState<{
    forPosition: ContextMenuPosition;
    adjustedPosition: ContextMenuPosition;
  } | null>(null);

  // Close on click outside
  useEffect(() => {
    if (!position) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEscape);

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [position, onClose]);

  // Calculate adjusted position after menu renders
  useLayoutEffect(() => {
    if (!position || !menuRef.current) {
      return;
    }

    const menu = menuRef.current;
    const rect = menu.getBoundingClientRect();

    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;
    const padding = 8;

    let x = position.x;
    let y = position.y;

    // Flip left if would overflow right
    if (x + rect.width > viewportWidth - padding) {
      x = Math.max(padding, position.x - rect.width);
    }

    // Flip up if would overflow bottom
    if (y + rect.height > viewportHeight - padding) {
      y = Math.max(padding, position.y - rect.height);
    }

    setAdjustment({
      forPosition: position,
      adjustedPosition: { x, y },
    });
  }, [position]);

  if (!position) return null;

  // Check if the adjustment was calculated for the current position
  const isAdjustedForCurrentPosition = adjustment &&
    adjustment.forPosition.x === position.x &&
    adjustment.forPosition.y === position.y;

  const finalPos = isAdjustedForCurrentPosition ? adjustment.adjustedPosition : position;
  const isReady = isAdjustedForCurrentPosition;

  // Use portal to render at document body level, bypassing any parent
  // backdrop-filter which creates a new containing block for fixed positioning
  return createPortal(
    <div
      ref={menuRef}
      className={cn(
        'fixed z-[100] min-w-[180px] py-1 whitespace-nowrap rounded-lg',
        'glass-context-menu',
        // Use invisible during measurement phase to prevent flicker
        isReady ? 'animate-in fade-in-0 zoom-in-95 duration-100' : 'invisible'
      )}
      style={{
        left: finalPos.x,
        top: finalPos.y,
      }}
    >
      {items.map((item, index) => {
        if (item.separator) {
          return (
            <div
              key={`sep-${index}`}
              className="h-px my-1 mx-2 bg-black/10 dark:bg-white/10"
            />
          );
        }

        return (
          <button
            key={item.id}
            onClick={() => {
              if (!item.disabled && item.onClick) {
                item.onClick();
                onClose();
              }
            }}
            disabled={item.disabled}
            className={cn(
              'w-full flex items-center gap-3 px-3 py-1.5 text-sm text-left',
              'text-gray-800 dark:text-gray-200',
              'transition-colors',
              item.disabled && 'opacity-50 cursor-not-allowed',
              !item.disabled && !item.danger && 'hover:bg-[color:var(--color-accent)]/15',
              !item.disabled && item.danger && 'hover:bg-red-500/15 text-red-600 dark:text-red-400'
            )}
          >
            {item.icon && (
              <span className="w-4 h-4 flex items-center justify-center">
                {item.icon}
              </span>
            )}
            <span className="flex-1">{item.label}</span>
            {item.shortcut && (
              <span className="text-xs text-gray-400 dark:text-gray-500">
                {item.shortcut}
              </span>
            )}
          </button>
        );
      })}
    </div>,
    document.body
  );
}

// Helper to build context menu items for background (empty space) clicks
export function buildBackgroundContextMenuItems(
  handlers: {
    onNewFolder?: () => void;
    onNewFile?: () => void;
  }
): ContextMenuItem[] {
  return [
    {
      id: 'new-folder',
      label: 'New Folder',
      icon: <FolderPlus size={14} />,
      shortcut: '⇧⌘N',
      onClick: handlers.onNewFolder,
    },
    {
      id: 'new-file',
      label: 'New File',
      icon: <FilePlus size={14} />,
      shortcut: '⌘N',
      onClick: handlers.onNewFile,
    },
  ];
}

// Helper to build context menu items for files/folders
export function buildFileContextMenuItems(
  entry: { name: string; path: string; isDirectory: boolean },
  handlers: {
    onOpen?: () => void;
    onOrganizeWithAI?: () => void;
    onAIRename?: () => void;
    onAIBatchRename?: () => void;
    onRename?: () => void;
    onCopy?: () => void;
    onPaste?: () => void;
    onMoveToTrash?: () => void;
    onGetInfo?: () => void;
  }
): ContextMenuItem[] {
  const items: ContextMenuItem[] = [];

  // Open
  items.push({
    id: 'open',
    label: 'Open',
    icon: <ExternalLink size={14} />,
    onClick: handlers.onOpen,
  });

  // Organize with AI (for folders only)
  if (entry.isDirectory) {
    items.push({
      id: 'organize-ai',
      label: 'Organize with AI',
      icon: <Sparkles size={14} className="text-purple-500" />,
      onClick: handlers.onOrganizeWithAI,
    });
  }

  items.push({ id: 'sep1', label: '', separator: true });

  // Rename
  items.push({
    id: 'rename',
    label: 'Rename',
    icon: <Edit3 size={14} />,
    onClick: handlers.onRename,
  });

  // AI Rename (single file) or AI Batch Rename (folder)
  if (entry.isDirectory && handlers.onAIBatchRename) {
    items.push({
      id: 'ai-batch-rename',
      label: 'AI Rename Files',
      icon: <Wand2 size={14} className="text-orange-500" />,
      onClick: handlers.onAIBatchRename,
    });
  } else if (!entry.isDirectory && handlers.onAIRename) {
    items.push({
      id: 'ai-rename',
      label: 'AI Rename',
      icon: <Wand2 size={14} className="text-orange-500" />,
      onClick: handlers.onAIRename,
    });
  }

  // Copy
  items.push({
    id: 'copy',
    label: 'Copy',
    icon: <Copy size={14} />,
    shortcut: '⌘C',
    onClick: handlers.onCopy,
  });

  // Paste (if in a folder)
  if (entry.isDirectory) {
    items.push({
      id: 'paste',
      label: 'Paste',
      icon: <Clipboard size={14} />,
      shortcut: '⌘V',
      disabled: true, // Enable when clipboard has content
      onClick: handlers.onPaste,
    });
  }

  items.push({ id: 'sep2', label: '', separator: true });

  // Get Info
  items.push({
    id: 'get-info',
    label: 'Get Info',
    icon: <FileText size={14} />,
    shortcut: '⌘I',
    onClick: handlers.onGetInfo,
  });

  items.push({ id: 'sep3', label: '', separator: true });

  // Move to Trash
  items.push({
    id: 'trash',
    label: 'Move to Trash',
    icon: <Trash2 size={14} />,
    shortcut: '⌘⌫',
    danger: true,
    onClick: handlers.onMoveToTrash,
  });

  return items;
}
