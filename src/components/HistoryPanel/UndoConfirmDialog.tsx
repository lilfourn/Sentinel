/**
 * UndoConfirmDialog - Modal for confirming undo operation with preflight results.
 */

import { X, AlertTriangle, CheckCircle, Loader2, RotateCcw, ShieldCheck } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useHistoryStore } from '../../stores/history-store';
import { ConflictList } from './ConflictList';
import type { ConflictResolution } from '../../types/history';

interface UndoConfirmDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

export function UndoConfirmDialog({ isOpen, onClose }: UndoConfirmDialogProps) {
  const {
    targetSession,
    preflightResult,
    isRunningPreflight,
    isUndoing,
    undoProgress,
    runPreflight,
    executeUndo,
  } = useHistoryStore();

  if (!isOpen || !targetSession) return null;

  const handleExecuteUndo = async (resolution: ConflictResolution) => {
    const result = await executeUndo(resolution);
    if (result.success) {
      onClose();
    }
  };

  const hasConflicts =
    preflightResult &&
    (preflightResult.modifiedFiles.length > 0 ||
      preflightResult.missingFiles.length > 0 ||
      preflightResult.blockingFiles.length > 0);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="w-full max-w-md bg-gray-900 border border-white/10 rounded-xl shadow-2xl overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-white/5">
          <div className="flex items-center gap-2">
            <RotateCcw size={16} className="text-blue-400" />
            <span className="text-sm font-medium text-gray-100">Undo Organization</span>
          </div>
          <button
            onClick={onClose}
            disabled={isUndoing}
            className="p-1 rounded-md hover:bg-white/5 text-gray-500 hover:text-gray-300 transition-colors disabled:opacity-30"
          >
            <X size={14} />
          </button>
        </div>

        {/* Content */}
        <div className="p-4 space-y-4">
          {/* Session info */}
          <div className="rounded-lg border border-white/10 bg-white/[0.02] p-3">
            <p className="text-sm text-gray-200 mb-1">
              {targetSession.userInstruction || 'AI Organization'}
            </p>
            <p className="text-xs text-gray-400">
              {new Date(targetSession.executedAt).toLocaleString()} - {targetSession.filesAffected} files affected
            </p>
          </div>

          {/* Preflight section */}
          {!preflightResult && !isRunningPreflight && (
            <div className="text-center py-4">
              <p className="text-sm text-gray-400 mb-3">
                Run a safety check before undoing
              </p>
              <button
                onClick={runPreflight}
                className="px-4 py-2 rounded-lg bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium transition-colors flex items-center gap-2 mx-auto"
              >
                <ShieldCheck size={14} />
                Check for conflicts
              </button>
            </div>
          )}

          {isRunningPreflight && (
            <div className="flex items-center justify-center py-6">
              <Loader2 size={20} className="animate-spin text-blue-400" />
              <span className="ml-2 text-sm text-gray-400">Checking file integrity...</span>
            </div>
          )}

          {preflightResult && (
            <>
              {/* Preflight status */}
              <div
                className={cn(
                  'rounded-lg border p-3 flex items-center gap-3',
                  preflightResult.canProceed
                    ? 'border-green-500/20 bg-green-500/5'
                    : 'border-red-500/20 bg-red-500/5'
                )}
              >
                {preflightResult.canProceed ? (
                  <>
                    <CheckCircle size={16} className="text-green-400" />
                    <div>
                      <p className="text-sm text-gray-200">Safe to proceed</p>
                      <p className="text-xs text-gray-400">
                        {preflightResult.safeOperations} of {preflightResult.totalOperations} operations can be undone
                      </p>
                    </div>
                  </>
                ) : (
                  <>
                    <AlertTriangle size={16} className="text-red-400" />
                    <div>
                      <p className="text-sm text-gray-200">Cannot proceed safely</p>
                      <p className="text-xs text-gray-400">
                        {preflightResult.conflictedOperations} conflicts detected
                      </p>
                    </div>
                  </>
                )}
              </div>

              {/* Conflict list */}
              {hasConflicts && (
                <ConflictList
                  modifiedFiles={preflightResult.modifiedFiles}
                  missingFiles={preflightResult.missingFiles}
                  blockingFiles={preflightResult.blockingFiles}
                />
              )}

              {/* Undo progress */}
              {isUndoing && undoProgress && (
                <div className="rounded-lg border border-white/10 bg-white/[0.02] p-3">
                  <div className="flex items-center justify-between text-sm mb-2">
                    <span className="text-gray-400">Undoing operations...</span>
                    <span className="text-gray-300">
                      {undoProgress.completed} / {undoProgress.total}
                    </span>
                  </div>
                  <div className="w-full h-1.5 bg-white/5 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-blue-500 transition-all duration-300"
                      style={{
                        width: `${undoProgress.total > 0
                          ? Math.min((undoProgress.completed / undoProgress.total) * 100, 100)
                          : 0}%`,
                      }}
                    />
                  </div>
                </div>
              )}
            </>
          )}
        </div>

        {/* Footer actions */}
        {preflightResult && !isUndoing && (
          <div className="px-4 py-3 border-t border-white/5 flex items-center justify-end gap-2">
            <button
              onClick={onClose}
              className="px-3 py-1.5 text-sm text-gray-400 hover:text-gray-200 transition-colors"
            >
              Cancel
            </button>

            {preflightResult.canProceed && (
              <button
                onClick={() => handleExecuteUndo('skip')}
                className="px-4 py-1.5 rounded-lg bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium transition-colors flex items-center gap-1.5"
              >
                <RotateCcw size={12} />
                Undo Changes
              </button>
            )}

            {!preflightResult.canProceed && hasConflicts && (
              <>
                <button
                  onClick={() => handleExecuteUndo('skip')}
                  className="px-3 py-1.5 rounded-lg bg-amber-600 hover:bg-amber-500 text-white text-sm font-medium transition-colors"
                >
                  Skip conflicts
                </button>
                <button
                  onClick={() => handleExecuteUndo('backup')}
                  className="px-3 py-1.5 rounded-lg bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium transition-colors"
                >
                  Backup & undo
                </button>
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
