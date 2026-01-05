import { useEffect, useCallback, useMemo } from 'react';
import { createPortal } from 'react-dom';
import { Sparkles, ArrowRight, X, Loader2, Check, Square, CheckSquare } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { BatchRenameSuggestion } from '../../hooks/useBatchRename';

interface BatchRenameProgress {
  stage: 'scanning' | 'analyzing' | 'complete';
  current: number;
  total: number;
  message: string;
}

interface BatchRenameDialogProps {
  isOpen: boolean;
  isLoading: boolean;
  isApplying: boolean;
  folderName: string;
  suggestions: BatchRenameSuggestion[];
  progress: BatchRenameProgress | null;
  error: string | null;
  onConfirm: () => void;
  onCancel: () => void;
  onToggleSelection: (path: string) => void;
  onSelectAll: (selected: boolean) => void;
}

export function BatchRenameDialog({
  isOpen,
  isLoading,
  isApplying,
  folderName,
  suggestions,
  progress,
  error,
  onConfirm,
  onCancel,
  onToggleSelection,
  onSelectAll,
}: BatchRenameDialogProps) {
  const selectedCount = useMemo(
    () => suggestions.filter((s) => s.selected).length,
    [suggestions]
  );

  const allSelected = selectedCount === suggestions.length && suggestions.length > 0;
  const noneSelected = selectedCount === 0;

  // Handle keyboard events
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!isOpen) return;

      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      } else if (e.key === 'Enter' && selectedCount > 0 && !isLoading && !isApplying && !error) {
        e.preventDefault();
        onConfirm();
      }
    },
    [isOpen, onCancel, onConfirm, selectedCount, isLoading, isApplying, error]
  );

  useEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  if (!isOpen) return null;

  return createPortal(
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div
        className={cn(
          'bg-white dark:bg-[#2a2a2a] rounded-xl shadow-2xl w-full max-w-2xl mx-4',
          'border border-gray-200 dark:border-gray-700',
          'animate-in zoom-in-95 fade-in duration-150',
          'max-h-[80vh] flex flex-col'
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-start gap-3 p-4 border-b border-gray-200 dark:border-gray-700">
          <div className="flex-shrink-0 p-2 bg-orange-100 dark:bg-orange-900/30 rounded-full">
            <Sparkles size={20} className="text-orange-600 dark:text-orange-400" />
          </div>
          <div className="flex-1 min-w-0">
            <h3 className="text-base font-semibold text-gray-900 dark:text-gray-100">
              AI Batch Rename
            </h3>
            <p className="mt-1 text-sm text-gray-600 dark:text-gray-400">
              {isLoading
                ? progress?.message || 'Analyzing files...'
                : error
                  ? 'Failed to generate suggestions'
                  : `${suggestions.length} rename suggestions for "${folderName}"`}
            </p>
          </div>
          <button
            onClick={onCancel}
            className="flex-shrink-0 p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400"
          >
            <X size={18} />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-hidden flex flex-col">
          {isLoading ? (
            <div className="flex flex-col items-center justify-center py-12 px-4">
              <Loader2 size={32} className="animate-spin text-orange-500 mb-4" />
              {progress && (
                <>
                  <p className="text-sm text-gray-600 dark:text-gray-400 mb-2">
                    {progress.message}
                  </p>
                  <div className="w-64 h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-orange-500 transition-all duration-300"
                      style={{
                        width: progress.total > 0 ? `${(progress.current / progress.total) * 100}%` : '0%',
                      }}
                    />
                  </div>
                  <p className="text-xs text-gray-500 mt-1">
                    {progress.current} / {progress.total} files
                  </p>
                </>
              )}
            </div>
          ) : error ? (
            <div className="py-8 px-4">
              <p className="text-sm text-red-600 dark:text-red-400 text-center">{error}</p>
            </div>
          ) : suggestions.length > 0 ? (
            <>
              {/* Select all header */}
              <div className="flex items-center gap-2 px-4 py-2 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-700">
                <button
                  onClick={() => onSelectAll(!allSelected)}
                  className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200"
                >
                  {allSelected ? (
                    <CheckSquare size={16} className="text-orange-500" />
                  ) : (
                    <Square size={16} />
                  )}
                  {allSelected ? 'Deselect all' : 'Select all'}
                </button>
                <span className="text-xs text-gray-500 ml-auto">
                  {selectedCount} of {suggestions.length} selected
                </span>
              </div>

              {/* Suggestions list */}
              <div className="flex-1 overflow-auto">
                {suggestions.map((suggestion) => (
                  <div
                    key={suggestion.path}
                    onClick={() => onToggleSelection(suggestion.path)}
                    className={cn(
                      'flex items-center gap-3 px-4 py-3 cursor-pointer',
                      'border-b border-gray-100 dark:border-gray-800 last:border-b-0',
                      'hover:bg-gray-50 dark:hover:bg-gray-800/50',
                      suggestion.selected && 'bg-orange-50/50 dark:bg-orange-900/10'
                    )}
                  >
                    {/* Checkbox */}
                    <div className="flex-shrink-0">
                      {suggestion.selected ? (
                        <CheckSquare size={18} className="text-orange-500" />
                      ) : (
                        <Square size={18} className="text-gray-400" />
                      )}
                    </div>

                    {/* Names */}
                    <div className="flex-1 min-w-0 flex items-center gap-2">
                      <span className="text-sm text-gray-600 dark:text-gray-400 truncate">
                        {suggestion.originalName}
                      </span>
                      <ArrowRight size={14} className="flex-shrink-0 text-gray-400" />
                      <span
                        className={cn(
                          'text-sm font-medium truncate',
                          suggestion.selected
                            ? 'text-orange-600 dark:text-orange-400'
                            : 'text-gray-700 dark:text-gray-300'
                        )}
                      >
                        {suggestion.suggestedName}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </>
          ) : null}
        </div>

        {/* Actions */}
        <div className="flex gap-2 p-4 border-t border-gray-200 dark:border-gray-700">
          <button
            onClick={onCancel}
            disabled={isApplying}
            className={cn(
              'flex-1 py-2 px-4 text-sm font-medium rounded-lg transition-colors',
              'text-gray-700 dark:text-gray-300',
              'hover:bg-gray-100 dark:hover:bg-gray-700',
              'disabled:opacity-50 disabled:cursor-not-allowed'
            )}
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={isLoading || isApplying || noneSelected || !!error}
            className={cn(
              'flex-1 py-2 px-4 text-sm font-medium rounded-lg transition-colors',
              'bg-orange-500 text-white hover:bg-orange-600',
              'disabled:opacity-50 disabled:cursor-not-allowed',
              'flex items-center justify-center gap-2'
            )}
          >
            {isApplying ? (
              <>
                <Loader2 size={14} className="animate-spin" />
                Renaming...
              </>
            ) : (
              <>
                <Check size={14} />
                Rename {selectedCount} {selectedCount === 1 ? 'file' : 'files'}
              </>
            )}
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}
