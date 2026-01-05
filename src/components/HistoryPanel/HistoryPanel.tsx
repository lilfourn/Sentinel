/**
 * HistoryPanel - Main panel showing organization history with timeline and undo controls.
 */

import { useEffect } from 'react';
import { X, History, Loader2, FolderOpen, RotateCcw, Trash2 } from 'lucide-react';
import { useHistoryStore } from '../../stores/history-store';
import { SessionCard } from './SessionCard';
import { UndoConfirmDialog } from './UndoConfirmDialog';

interface HistoryPanelProps {
  folderPath: string;
  onClose: () => void;
}

export function HistoryPanel({ folderPath, onClose }: HistoryPanelProps) {
  const {
    sessions,
    summary,
    isLoading,
    error,
    isUndoModalOpen,
    loadHistory,
    openUndoModal,
    closeUndoModal,
    deleteHistory,
  } = useHistoryStore();

  // Load history when folder changes
  useEffect(() => {
    if (folderPath) {
      loadHistory(folderPath);
    }
  }, [folderPath, loadHistory]);

  const folderName = folderPath.split('/').pop() || 'Folder';

  const handleDeleteHistory = async () => {
    if (window.confirm('Delete all organization history for this folder? This cannot be undone.')) {
      await deleteHistory(folderPath);
      onClose();
    }
  };

  return (
    <>
      <div className="w-80 h-full flex flex-col glass-sidebar border-l border-white/5 dark:border-white/5">
        {/* Header */}
        <div className="flex items-center justify-between px-3 py-2.5 border-b border-black/5 dark:border-white/5">
          <div className="flex items-center gap-2">
            <div className="w-6 h-6 rounded-lg bg-gradient-to-br from-orange-500 to-amber-500 flex items-center justify-center shadow-sm">
              <History size={13} className="text-white" />
            </div>
            <span className="text-sm font-medium text-gray-800 dark:text-gray-100">History</span>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-md hover:bg-black/5 dark:hover:bg-white/5 text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 transition-colors"
          >
            <X size={14} />
          </button>
        </div>

        {/* Folder info */}
        <div className="px-3 py-2 border-b border-black/5 dark:border-white/5 bg-black/[0.02] dark:bg-white/[0.02]">
          <div className="flex items-center gap-2 text-gray-700 dark:text-gray-300">
            <FolderOpen size={14} />
            <span className="text-sm truncate">{folderName}</span>
          </div>
          {summary && (
            <p className="text-xs text-gray-500 mt-1">
              {summary.sessionCount} sessions · {summary.totalOperations} total operations
            </p>
          )}
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto">
          {isLoading && (
            <div className="flex items-center justify-center py-12">
              <Loader2 size={20} className="animate-spin text-gray-500" />
            </div>
          )}

          {error && (
            <div className="px-3 py-8 text-center">
              <p className="text-sm text-red-500 dark:text-red-400">{error}</p>
            </div>
          )}

          {!isLoading && !error && sessions.length === 0 && (
            <div className="px-3 py-12 text-center">
              <History size={32} className="mx-auto mb-3 text-gray-400 dark:text-gray-600" />
              <p className="text-sm text-gray-600 dark:text-gray-400 mb-1">No organization history</p>
              <p className="text-xs text-gray-500">
                Run AI organization on this folder to create history
              </p>
            </div>
          )}

          {!isLoading && !error && sessions.length > 0 && (
            <div className="p-3 space-y-3">
              {/* Info banner */}
              <div className="rounded-lg border border-orange-500/20 bg-orange-500/5 p-2.5 text-xs text-gray-600 dark:text-gray-400">
                <RotateCcw size={12} className="inline-block mr-1.5 text-orange-500" />
                Click <strong className="text-gray-800 dark:text-gray-200">Undo</strong> to restore files to their state before that organization
              </div>

              {/* Session timeline */}
              <div className="space-y-3">
                {sessions.map((session, index) => (
                  <SessionCard
                    key={session.sessionId}
                    session={session}
                    isFirst={index === 0}
                    onUndo={openUndoModal}
                  />
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Footer actions */}
        {sessions.length > 0 && (
          <div className="px-3 py-2 border-t border-black/5 dark:border-white/5">
            <button
              onClick={handleDeleteHistory}
              className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 text-xs text-red-500 dark:text-red-400 hover:text-red-600 dark:hover:text-red-300 hover:bg-red-500/10 rounded-md transition-colors"
            >
              <Trash2 size={12} />
              Clear all history
            </button>
          </div>
        )}
      </div>

      {/* Undo confirmation dialog */}
      <UndoConfirmDialog isOpen={isUndoModalOpen} onClose={closeUndoModal} />
    </>
  );
}
