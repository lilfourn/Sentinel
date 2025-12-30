import { useState, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { AlertTriangle, X } from 'lucide-react';
import { cn } from '../../lib/utils';

interface ConfirmDialogProps {
  isOpen: boolean;
  title: string;
  message: string;
  itemName?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: 'danger' | 'warning' | 'default';
  showDontAskAgain?: boolean;
  onConfirm: (dontAskAgain: boolean) => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  isOpen,
  title,
  message,
  itemName,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  variant = 'default',
  showDontAskAgain = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const [dontAskAgain, setDontAskAgain] = useState(false);

  // Reset checkbox when dialog closes
  useEffect(() => {
    if (!isOpen) {
      setDontAskAgain(false);
    }
  }, [isOpen]);

  // Handle keyboard events
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!isOpen) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      onConfirm(dontAskAgain);
    }
  }, [isOpen, onCancel, onConfirm, dontAskAgain]);

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
          "bg-white dark:bg-[#2a2a2a] rounded-xl shadow-2xl w-full max-w-sm mx-4",
          "border border-gray-200 dark:border-gray-700",
          "animate-in zoom-in-95 fade-in duration-150"
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-start gap-3 p-4">
          {variant === 'danger' && (
            <div className="flex-shrink-0 p-2 bg-red-100 dark:bg-red-900/30 rounded-full">
              <AlertTriangle size={20} className="text-red-600 dark:text-red-400" />
            </div>
          )}
          {variant === 'warning' && (
            <div className="flex-shrink-0 p-2 bg-orange-100 dark:bg-orange-900/30 rounded-full">
              <AlertTriangle size={20} className="text-orange-600 dark:text-orange-400" />
            </div>
          )}
          <div className="flex-1 min-w-0">
            <h3 className="text-base font-semibold text-gray-900 dark:text-gray-100">
              {title}
            </h3>
            <p className="mt-1 text-sm text-gray-600 dark:text-gray-400">
              {message}
            </p>
            {itemName && (
              <p className="mt-2 text-sm font-medium text-gray-900 dark:text-gray-100 truncate">
                "{itemName}"
              </p>
            )}
          </div>
          <button
            onClick={onCancel}
            className="flex-shrink-0 p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400"
          >
            <X size={18} />
          </button>
        </div>

        {/* Don't ask again checkbox */}
        {showDontAskAgain && (
          <div className="px-4 pb-2">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={dontAskAgain}
                onChange={(e) => setDontAskAgain(e.target.checked)}
                className="w-4 h-4 rounded border-gray-300 text-orange-500 focus:ring-orange-500"
              />
              <span className="text-sm text-gray-600 dark:text-gray-400">
                Don't ask again
              </span>
            </label>
          </div>
        )}

        {/* Actions */}
        <div className="flex gap-2 p-4 border-t border-gray-200 dark:border-gray-700">
          <button
            onClick={onCancel}
            className={cn(
              "flex-1 py-2 px-4 text-sm font-medium rounded-lg transition-colors",
              "text-gray-700 dark:text-gray-300",
              "hover:bg-gray-100 dark:hover:bg-gray-700"
            )}
          >
            {cancelLabel}
          </button>
          <button
            onClick={() => onConfirm(dontAskAgain)}
            className={cn(
              "flex-1 py-2 px-4 text-sm font-medium rounded-lg transition-colors",
              variant === 'danger' && "bg-red-600 text-white hover:bg-red-700",
              variant === 'warning' && "bg-orange-500 text-white hover:bg-orange-600",
              variant === 'default' && "bg-blue-600 text-white hover:bg-blue-700"
            )}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}
