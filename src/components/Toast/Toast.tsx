import { useEffect, useState } from 'react';
import { X, Check, AlertCircle, Undo2 } from 'lucide-react';
import { cn } from '../../lib/utils';

export interface ToastProps {
  id: string;
  type: 'success' | 'error' | 'info' | 'rename';
  title: string;
  message?: string;
  duration?: number;
  onDismiss: (id: string) => void;
  onUndo?: () => void;
  undoTimeout?: number;
}

export function Toast({
  id,
  type,
  title,
  message,
  duration = 2500,
  onDismiss,
  onUndo,
  undoTimeout = 6000,
}: ToastProps) {
  const [progress, setProgress] = useState(100);
  const [isHovered, setIsHovered] = useState(false);

  // Auto-dismiss timer
  useEffect(() => {
    if (isHovered) return;

    const timeout = type === 'rename' ? undoTimeout : duration;
    const interval = 50;
    const step = (interval / timeout) * 100;

    const timer = setInterval(() => {
      setProgress((prev) => {
        const next = prev - step;
        if (next <= 0) {
          clearInterval(timer);
          onDismiss(id);
          return 0;
        }
        return next;
      });
    }, interval);

    return () => clearInterval(timer);
  }, [id, type, duration, undoTimeout, isHovered, onDismiss]);

  const icons = {
    success: <Check size={18} className="text-green-500" />,
    error: <AlertCircle size={18} className="text-red-500" />,
    info: <AlertCircle size={18} className="text-orange-500" />,
    rename: <Check size={18} className="text-green-500" />,
  };

  return (
    <div
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      className={cn(
        'relative overflow-hidden rounded-xl shadow-lg',
        'bg-white/90 dark:bg-[#2a2a2a]/90 backdrop-blur-[20px]',
        'border border-black/5 dark:border-white/10',
        'min-w-72 max-w-md'
      )}
    >
      <div className="p-4 flex items-start gap-3">
        {/* Icon */}
        <div className="flex-shrink-0 mt-0.5">{icons[type]}</div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium text-gray-900 dark:text-gray-100">
            {title}
          </p>
          {message && (
            <p className="mt-1 text-sm text-gray-500 dark:text-gray-400 truncate">
              {message}
            </p>
          )}

          {/* Undo button for rename toasts */}
          {type === 'rename' && onUndo && (
            <button
              onClick={() => {
                onUndo();
                onDismiss(id);
              }}
              className="mt-2 flex items-center gap-1 text-sm text-orange-500 dark:text-orange-400 hover:text-orange-600 dark:hover:text-orange-300 font-medium"
            >
              <Undo2 size={14} />
              Undo
            </button>
          )}
        </div>

        {/* Close button */}
        <button
          onClick={() => onDismiss(id)}
          className="flex-shrink-0 p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
        >
          <X size={16} />
        </button>
      </div>

      {/* Progress bar */}
      <div className="h-1 bg-gray-100 dark:bg-gray-700">
        <div
          className={cn(
            'h-full transition-all duration-50',
            type === 'error' && 'bg-red-500',
            type === 'success' && 'bg-green-500',
            type === 'info' && 'bg-orange-500',
            type === 'rename' && 'bg-green-500'
          )}
          style={{ width: `${progress}%` }}
        />
      </div>
    </div>
  );
}
