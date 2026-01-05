import { AlertTriangle, Play, X } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { ValidationError } from '../../types/plan-edit';

interface PlanEditFooterProps {
  validationErrors: ValidationError[];
  enabledCount: number;
  hasChanges: boolean;
  onCancel: () => void;
  onApply: () => void;
}

export function PlanEditFooter({
  validationErrors,
  enabledCount,
  hasChanges,
  onCancel,
  onApply,
}: PlanEditFooterProps) {
  const hasErrors = validationErrors.length > 0;

  return (
    <div className="border-t border-white/10 px-5 py-4">
      {/* Validation errors */}
      {hasErrors && (
        <div className="mb-3 p-3 rounded-lg bg-red-500/10 border border-red-500/20">
          <div className="flex items-center gap-2 text-xs text-red-400">
            <AlertTriangle size={14} />
            <span className="font-medium">
              {validationErrors.length} validation{' '}
              {validationErrors.length === 1 ? 'issue' : 'issues'}
            </span>
          </div>
          <ul className="mt-2 space-y-1">
            {validationErrors.slice(0, 3).map((error, i) => (
              <li key={i} className="text-xs text-red-400/80 pl-5">
                {error.message}
              </li>
            ))}
            {validationErrors.length > 3 && (
              <li className="text-xs text-red-400/60 pl-5">
                +{validationErrors.length - 3} more issues
              </li>
            )}
          </ul>
        </div>
      )}

      {/* Action buttons */}
      <div className="flex items-center justify-between">
        <span className="text-xs text-gray-500">
          {enabledCount} operations will be applied
          {hasChanges && (
            <span className="text-orange-400 ml-2">*changes pending</span>
          )}
        </span>

        <div className="flex items-center gap-2">
          <button
            onClick={onCancel}
            className={cn(
              'flex items-center gap-1.5 px-4 py-2 rounded-lg',
              'text-sm font-medium transition-colors',
              'bg-white/5 text-gray-400 hover:bg-white/10 hover:text-gray-300'
            )}
          >
            <X size={14} />
            Cancel
          </button>
          <button
            onClick={onApply}
            disabled={enabledCount === 0}
            className={cn(
              'flex items-center gap-1.5 px-4 py-2 rounded-lg',
              'text-sm font-medium transition-all',
              'bg-gradient-to-r from-orange-600 to-orange-500',
              'text-white shadow-sm',
              'hover:from-orange-500 hover:to-orange-400',
              'disabled:opacity-50 disabled:cursor-not-allowed'
            )}
          >
            <Play size={14} />
            Apply {enabledCount} Changes
          </button>
        </div>
      </div>
    </div>
  );
}
