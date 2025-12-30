import { useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { Cloud, X } from 'lucide-react';
import { cn } from '../../lib/utils';

interface ICloudErrorDialogProps {
  isOpen: boolean;
  fileName: string;
  onClose: () => void;
  onUseQuarantine?: () => void;
}

export function ICloudErrorDialog({
  isOpen,
  fileName,
  onClose,
  onUseQuarantine,
}: ICloudErrorDialogProps) {
  // Handle keyboard events
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!isOpen) return;

    if (e.key === 'Escape' || e.key === 'Enter') {
      e.preventDefault();
      onClose();
    }
  }, [isOpen, onClose]);

  useEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  if (!isOpen) return null;

  return createPortal(
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className={cn(
          "bg-white dark:bg-[#2a2a2a] rounded-xl shadow-2xl w-full max-w-md mx-4",
          "border border-gray-200 dark:border-gray-700",
          "animate-in zoom-in-95 fade-in duration-150"
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-start gap-3 p-4">
          <div className="flex-shrink-0 p-2 bg-blue-100 dark:bg-blue-900/30 rounded-full">
            <Cloud size={20} className="text-blue-600 dark:text-blue-400" />
          </div>
          <div className="flex-1">
            <h3 className="text-base font-semibold text-gray-900 dark:text-gray-100">
              iCloud File Not Downloaded
            </h3>
            <p className="mt-1 text-sm text-gray-600 dark:text-gray-400">
              <span className="font-medium">"{fileName}"</span> is stored in iCloud
              and needs to be downloaded before it can be moved to Trash.
            </p>
          </div>
          <button
            onClick={onClose}
            className="flex-shrink-0 p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400"
          >
            <X size={18} />
          </button>
        </div>

        {/* Instructions */}
        <div className="px-4 pb-4 space-y-2">
          <p className="text-sm text-gray-600 dark:text-gray-400">
            To delete this file:
          </p>
          <ol className="text-sm text-gray-600 dark:text-gray-400 list-decimal list-inside space-y-1 ml-1">
            <li>Right-click the file in Finder</li>
            <li>Select "Download Now"</li>
            <li>Wait for download to complete</li>
            <li>Try deleting again in Sentinel</li>
          </ol>
        </div>

        {/* Actions */}
        <div className="flex flex-col gap-2 p-4 border-t border-gray-200 dark:border-gray-700">
          {onUseQuarantine && (
            <button
              onClick={onUseQuarantine}
              className={cn(
                "w-full py-2 px-4 text-sm font-medium rounded-lg transition-colors",
                "bg-orange-500 text-white hover:bg-orange-600"
              )}
            >
              Move to Sentinel Quarantine Instead
            </button>
          )}
          <button
            onClick={onClose}
            className={cn(
              "w-full py-2 px-4 text-sm font-medium rounded-lg transition-colors",
              "text-gray-700 dark:text-gray-300",
              "hover:bg-gray-100 dark:hover:bg-gray-700",
              "border border-gray-300 dark:border-gray-600"
            )}
          >
            OK
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}
