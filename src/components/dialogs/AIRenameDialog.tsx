import { useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { Sparkles, ArrowRight, X, RefreshCw, Loader2 } from 'lucide-react';
import { cn } from '../../lib/utils';

interface AIRenameDialogProps {
  isOpen: boolean;
  isLoading: boolean;
  originalName: string;
  suggestedName: string | null;
  error: string | null;
  onConfirm: () => void;
  onCancel: () => void;
  onRetry?: () => void;
}

export function AIRenameDialog({
  isOpen,
  isLoading,
  originalName,
  suggestedName,
  error,
  onConfirm,
  onCancel,
  onRetry,
}: AIRenameDialogProps) {
  // Handle keyboard events
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!isOpen) return;

      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      } else if (e.key === 'Enter' && suggestedName && !isLoading && !error) {
        e.preventDefault();
        onConfirm();
      }
    },
    [isOpen, onCancel, onConfirm, suggestedName, isLoading, error]
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
          'bg-white dark:bg-[#2a2a2a] rounded-xl shadow-2xl w-full max-w-md mx-4',
          'border border-gray-200 dark:border-gray-700',
          'animate-in zoom-in-95 fade-in duration-150'
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-start gap-3 p-4">
          <div className="flex-shrink-0 p-2 bg-orange-100 dark:bg-orange-900/30 rounded-full">
            <Sparkles size={20} className="text-orange-600 dark:text-orange-400" />
          </div>
          <div className="flex-1 min-w-0">
            <h3 className="text-base font-semibold text-gray-900 dark:text-gray-100">
              AI Rename
            </h3>
            <p className="mt-1 text-sm text-gray-600 dark:text-gray-400">
              {isLoading
                ? 'Generating suggestion...'
                : error
                  ? 'Failed to generate suggestion'
                  : 'Review the suggested name'}
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
        <div className="px-4 pb-4">
          {isLoading ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 size={24} className="animate-spin text-orange-500" />
            </div>
          ) : error ? (
            <div className="py-4">
              <p className="text-sm text-red-600 dark:text-red-400 text-center">{error}</p>
            </div>
          ) : suggestedName ? (
            <div className="space-y-3">
              {/* Original name */}
              <div className="flex items-center gap-3">
                <span className="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide w-16">
                  From
                </span>
                <div className="flex-1 px-3 py-2 bg-gray-100 dark:bg-gray-800 rounded-lg">
                  <span className="text-sm text-gray-700 dark:text-gray-300 break-all">
                    {originalName}
                  </span>
                </div>
              </div>

              {/* Arrow */}
              <div className="flex justify-center">
                <ArrowRight size={20} className="text-gray-400 rotate-90" />
              </div>

              {/* Suggested name */}
              <div className="flex items-center gap-3">
                <span className="text-xs font-medium text-orange-500 uppercase tracking-wide w-16">
                  To
                </span>
                <div className="flex-1 px-3 py-2 bg-orange-50 dark:bg-orange-900/20 rounded-lg border border-orange-200 dark:border-orange-800">
                  <span className="text-sm text-orange-700 dark:text-orange-300 font-medium break-all">
                    {suggestedName}
                  </span>
                </div>
              </div>
            </div>
          ) : null}
        </div>

        {/* Actions */}
        <div className="flex gap-2 p-4 border-t border-gray-200 dark:border-gray-700">
          <button
            onClick={onCancel}
            className={cn(
              'flex-1 py-2 px-4 text-sm font-medium rounded-lg transition-colors',
              'text-gray-700 dark:text-gray-300',
              'hover:bg-gray-100 dark:hover:bg-gray-700'
            )}
          >
            Cancel
          </button>
          {error && onRetry ? (
            <button
              onClick={onRetry}
              className={cn(
                'flex-1 py-2 px-4 text-sm font-medium rounded-lg transition-colors',
                'bg-orange-500 text-white hover:bg-orange-600',
                'flex items-center justify-center gap-2'
              )}
            >
              <RefreshCw size={14} />
              Retry
            </button>
          ) : (
            <button
              onClick={onConfirm}
              disabled={isLoading || !suggestedName}
              className={cn(
                'flex-1 py-2 px-4 text-sm font-medium rounded-lg transition-colors',
                'bg-orange-500 text-white hover:bg-orange-600',
                'disabled:opacity-50 disabled:cursor-not-allowed'
              )}
            >
              Apply
            </button>
          )}
        </div>
      </div>
    </div>,
    document.body
  );
}
