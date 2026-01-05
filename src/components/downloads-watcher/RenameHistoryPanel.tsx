import { useState, memo } from "react";
import {
  History,
  Undo2,
  ChevronDown,
  ChevronUp,
  Check,
  X,
  FolderOpen,
} from "lucide-react";
import { cn } from "../../lib/utils";
import {
  useDownloadsWatcherStore,
  type RenameHistoryItem,
} from "../../stores/downloads-watcher-store";
import { showSuccess, showError } from "../../stores/toast-store";

interface RenameHistoryPanelProps {
  maxItems?: number;
  compact?: boolean;
}

export function RenameHistoryPanel({
  maxItems = 20,
  compact = false,
}: RenameHistoryPanelProps) {
  const { history, undoRename, removeFromHistory, clearHistory } =
    useDownloadsWatcherStore();
  const [expanded, setExpanded] = useState(!compact);

  const displayHistory = history.slice(0, maxItems);

  const handleUndo = async (item: RenameHistoryItem) => {
    const success = await undoRename(item.id);
    if (success) {
      showSuccess("Rename undone", `Restored: ${item.originalName}`);
    } else {
      showError("Failed to undo", "The file may have been moved or deleted");
    }
  };

  const formatTimeAgo = (timestamp: number) => {
    const seconds = Math.floor((Date.now() - timestamp) / 1000);

    if (seconds < 60) return "just now";
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
    return `${Math.floor(seconds / 86400)}d ago`;
  };

  if (history.length === 0) {
    return (
      <div className="p-4 text-center text-sm text-gray-500 dark:text-gray-400">
        <History size={24} className="mx-auto mb-2 opacity-50" />
        <p>No rename history yet</p>
        <p className="text-xs mt-1">Files will appear here when auto-renamed</p>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {/* Header */}
      <div className="flex items-center justify-between">
        <button
          onClick={() => setExpanded(!expanded)}
          className="flex items-center gap-2 text-sm font-medium text-gray-700 dark:text-gray-300 hover:text-gray-900 dark:hover:text-gray-100"
        >
          <History size={16} />
          <span>Rename History</span>
          <span className="px-1.5 py-0.5 text-xs rounded-full bg-gray-200 dark:bg-gray-700">
            {history.length}
          </span>
          {expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
        </button>

        {history.length > 0 && (
          <button
            onClick={() => {
              if (confirm("Clear all rename history?")) {
                clearHistory();
              }
            }}
            className="text-xs text-gray-400 hover:text-red-500 dark:hover:text-red-400"
          >
            Clear all
          </button>
        )}
      </div>

      {/* History list */}
      {expanded && (
        <div className="space-y-1 max-h-64 overflow-y-auto">
          {displayHistory.map((item) => (
            <HistoryItem
              key={item.id}
              item={item}
              onUndo={() => handleUndo(item)}
              onRemove={() => removeFromHistory(item.id)}
              formatTimeAgo={formatTimeAgo}
            />
          ))}

          {history.length > maxItems && (
            <p className="text-xs text-center text-gray-400 py-2">
              +{history.length - maxItems} more items
            </p>
          )}
        </div>
      )}
    </div>
  );
}

interface HistoryItemProps {
  item: RenameHistoryItem;
  onUndo: () => void;
  onRemove: () => void;
  formatTimeAgo: (timestamp: number) => string;
}

/** Memoized history item - only re-renders when item data changes */
const HistoryItem = memo(function HistoryItem({
  item,
  onUndo,
  onRemove,
  formatTimeAgo,
}: HistoryItemProps) {
  return (
    <div
      className={cn(
        "group p-2 rounded-lg border transition-colors",
        item.undone
          ? "bg-gray-50 dark:bg-gray-800/50 border-gray-200 dark:border-gray-700 opacity-60"
          : "bg-white dark:bg-[#2a2a2a] border-gray-200 dark:border-gray-700 hover:border-orange-300 dark:hover:border-orange-600"
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1 min-w-0">
          {/* Original -> New name */}
          <div className="flex items-center gap-1.5 text-sm">
            <span className="truncate text-gray-500 dark:text-gray-400 line-through">
              {item.originalName}
            </span>
            <span className="text-gray-400">→</span>
            <span
              className={cn(
                "truncate font-medium",
                item.undone
                  ? "text-gray-400 line-through"
                  : "text-gray-900 dark:text-gray-100"
              )}
            >
              {item.newName}
            </span>
          </div>

          {/* Folder + time */}
          <div className="flex items-center gap-2 mt-1 text-xs text-gray-400">
            <FolderOpen size={10} />
            <span className="truncate">{item.folderName}</span>
            <span>·</span>
            <span>{formatTimeAgo(item.timestamp)}</span>
            {item.undone && (
              <>
                <span>·</span>
                <span className="text-amber-500">Undone</span>
              </>
            )}
          </div>
        </div>

        {/* Actions */}
        <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
          {item.canUndo && !item.undone && (
            <button
              onClick={onUndo}
              className="p-1 rounded hover:bg-orange-100 dark:hover:bg-orange-900/30 text-orange-500"
              title="Undo rename"
            >
              <Undo2 size={14} />
            </button>
          )}
          <button
            onClick={onRemove}
            className="p-1 rounded hover:bg-red-100 dark:hover:bg-red-900/30 text-gray-400 hover:text-red-500"
            title="Remove from history"
          >
            <X size={14} />
          </button>
        </div>
      </div>
    </div>
  );
}, (prev, next) => {
  // Custom comparison - only re-render if item data changed
  return (
    prev.item.id === next.item.id &&
    prev.item.undone === next.item.undone &&
    prev.item.canUndo === next.item.canUndo &&
    prev.item.newName === next.item.newName &&
    prev.item.originalName === next.item.originalName &&
    prev.item.folderName === next.item.folderName
  );
});

// Export a compact version for the sidebar
export function RenameHistoryCompact() {
  const { history, undoRename } = useDownloadsWatcherStore();
  const recentItems = history.slice(0, 3);

  if (recentItems.length === 0) return null;

  return (
    <div className="space-y-1">
      {recentItems.map((item) => (
        <div
          key={item.id}
          className="flex items-center justify-between p-1.5 rounded text-xs bg-gray-50 dark:bg-gray-800/50"
        >
          <div className="flex items-center gap-1.5 min-w-0">
            {item.undone ? (
              <X size={10} className="text-gray-400 flex-shrink-0" />
            ) : (
              <Check size={10} className="text-green-500 flex-shrink-0" />
            )}
            <span className="truncate text-gray-600 dark:text-gray-400">
              {item.newName}
            </span>
          </div>
          {item.canUndo && !item.undone && (
            <button
              onClick={() => undoRename(item.id)}
              className="p-0.5 text-orange-500 hover:text-orange-600"
            >
              <Undo2 size={10} />
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
