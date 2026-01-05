import { Play, X, Edit, AlertTriangle } from 'lucide-react';
import { cn } from '../../lib/utils';

interface SimulationControlsProps {
  /** Whether there are conflicts that prevent applying */
  hasConflicts: boolean;
  /** Number of conflicts */
  conflictCount: number;
  /** Whether the system is currently applying changes */
  isApplying: boolean;
  /** Callback when user clicks Apply */
  onApply: () => void;
  /** Callback when user clicks Cancel */
  onCancel: () => void;
  /** Callback when user clicks Edit Plan */
  onEdit?: () => void;
  /** Optional additional class names */
  className?: string;
}

/**
 * Control buttons for the simulation phase.
 * Shows Edit Plan, Cancel, and Apply Changes buttons.
 * Apply is disabled if there are conflicts.
 */
export function SimulationControls({
  hasConflicts,
  conflictCount,
  isApplying,
  onApply,
  onCancel,
  onEdit,
  className,
}: SimulationControlsProps) {
  return (
    <div className={cn('flex flex-col gap-2', className)}>
      {/* Conflict warning */}
      {hasConflicts && (
        <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20">
          <AlertTriangle size={14} className="text-red-400 flex-shrink-0" />
          <span className="text-xs text-red-400">
            {conflictCount} {conflictCount === 1 ? 'conflict' : 'conflicts'} found
          </span>
        </div>
      )}

      {/* Button row - all buttons equal width */}
      <div className="grid grid-cols-3 gap-2">
        {/* Edit Plan button (optional) */}
        {onEdit ? (
          <button
            onClick={onEdit}
            disabled={isApplying}
            className={cn(
              'flex items-center justify-center gap-1.5 px-3 py-2 rounded-md',
              'text-xs font-medium transition-colors',
              'bg-white/5 text-gray-300 hover:bg-white/10',
              'disabled:opacity-50 disabled:cursor-not-allowed'
            )}
          >
            <Edit size={12} />
            Edit Plan
          </button>
        ) : (
          <div /> /* Empty grid cell */
        )}

        {/* Cancel button */}
        <button
          onClick={onCancel}
          disabled={isApplying}
          className={cn(
            'flex items-center justify-center gap-1.5 px-3 py-2 rounded-md',
            'text-xs font-medium transition-colors',
            'bg-white/5 text-gray-400 hover:bg-white/10 hover:text-gray-300',
            'disabled:opacity-50 disabled:cursor-not-allowed'
          )}
        >
          <X size={12} />
          Cancel
        </button>

        {/* Apply button */}
        <button
          onClick={onApply}
          disabled={hasConflicts || isApplying}
          className={cn(
            'flex items-center justify-center gap-1.5 px-3 py-2 rounded-md',
            'text-xs font-medium transition-all',
            'bg-gradient-to-r from-green-600 to-green-500',
            'text-white shadow-sm',
            'hover:from-green-500 hover:to-green-400',
            'disabled:opacity-50 disabled:cursor-not-allowed disabled:from-gray-600 disabled:to-gray-500'
          )}
        >
          <Play size={12} />
          {isApplying ? 'Applying...' : 'Apply Changes'}
        </button>
      </div>
    </div>
  );
}

/**
 * Compact inline version for tighter layouts.
 */
export function SimulationControlsInline({
  hasConflicts,
  isApplying,
  onApply,
  onCancel,
}: Omit<SimulationControlsProps, 'conflictCount' | 'onEdit' | 'className'>) {
  return (
    <div className="flex items-center gap-2">
      <button
        onClick={onCancel}
        disabled={isApplying}
        className="px-2 py-1 text-xs text-gray-400 hover:text-gray-300 rounded hover:bg-white/5"
      >
        Cancel
      </button>
      <button
        onClick={onApply}
        disabled={hasConflicts || isApplying}
        className={cn(
          'px-3 py-1 text-xs font-medium rounded',
          'bg-green-600 text-white hover:bg-green-500',
          'disabled:opacity-50 disabled:cursor-not-allowed'
        )}
      >
        {isApplying ? 'Applying...' : 'Apply'}
      </button>
    </div>
  );
}
