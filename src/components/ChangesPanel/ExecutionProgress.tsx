import { Loader2, CheckCircle, XCircle } from 'lucide-react';
import { cn } from '../../lib/utils';

export type ExecutionPhase = 'preparing' | 'executing' | 'complete' | 'failed';

interface ExecutionProgressProps {
  completed: number;
  total: number;
  phase: ExecutionPhase;
  className?: string;
}

/**
 * Clean execution progress component that replaces per-operation thought items.
 * Shows a single progress bar during execution instead of 2000+ individual list items.
 */
export function ExecutionProgress({ completed, total, phase, className }: ExecutionProgressProps) {
  const percentage = total > 0 ? (completed / total) * 100 : 0;
  const isComplete = phase === 'complete';
  const isFailed = phase === 'failed';
  const isExecuting = phase === 'executing';

  return (
    <div
      className={cn(
        'p-3 rounded-lg transition-colors',
        isComplete && 'bg-green-500/10 border border-green-500/20',
        isFailed && 'bg-red-500/10 border border-red-500/20',
        isExecuting && 'bg-orange-500/10 border border-orange-500/20',
        phase === 'preparing' && 'bg-white/5 border border-white/10',
        className
      )}
    >
      {/* Header row */}
      <div className="flex items-center gap-2 mb-2">
        {isExecuting && (
          <Loader2 size={14} className="text-orange-400 animate-spin" />
        )}
        {isComplete && (
          <CheckCircle size={14} className="text-green-400" />
        )}
        {isFailed && (
          <XCircle size={14} className="text-red-400" />
        )}
        {phase === 'preparing' && (
          <Loader2 size={14} className="text-gray-400 animate-spin" />
        )}

        <span
          className={cn(
            'text-xs font-medium',
            isComplete && 'text-green-300',
            isFailed && 'text-red-300',
            isExecuting && 'text-orange-300',
            phase === 'preparing' && 'text-gray-300'
          )}
        >
          {phase === 'preparing' && 'Preparing...'}
          {isExecuting && 'Organizing files...'}
          {isComplete && 'Organization complete'}
          {isFailed && 'Organization failed'}
        </span>
      </div>

      {/* Progress bar */}
      <div className="h-1.5 bg-white/10 rounded-full overflow-hidden">
        <div
          className={cn(
            'h-full rounded-full transition-all duration-300',
            isComplete && 'bg-green-500',
            isFailed && 'bg-red-500',
            isExecuting && 'bg-orange-500',
            phase === 'preparing' && 'bg-gray-500'
          )}
          style={{ width: `${percentage}%` }}
        />
      </div>

      {/* Progress text */}
      <div className="flex items-center justify-between mt-1.5">
        <span className="text-[10px] text-gray-500 tabular-nums">
          {completed} of {total} operations
        </span>
        <span className="text-[10px] text-gray-500 tabular-nums">
          {percentage.toFixed(0)}%
        </span>
      </div>
    </div>
  );
}
