import { Loader2, CheckCircle, FileSearch, Brain, FolderTree } from 'lucide-react';
import { cn } from '../../lib/utils';

interface AnalysisProgressBarProps {
  current: number;
  total: number;
  phase: string;
  message: string;
  className?: string;
}

/**
 * Progress bar component for AI analysis phase.
 * Shows scanning, extracting, analyzing, and planning progress
 * instead of individual list items per file.
 */
export function AnalysisProgressBar({
  current,
  total,
  phase,
  message,
  className,
}: AnalysisProgressBarProps) {
  const percentage = total > 0 ? (current / total) * 100 : 0;
  const isComplete = phase === 'complete';
  const isPlanning = phase === 'planning' || phase === 'summarizing';
  const isAnalyzing = phase === 'analyzing' || phase === 'extracting' || phase === 'rendering';
  const isScanning = phase === 'scanning';

  // Get icon based on phase
  const phaseIcon = isComplete ? (
    <CheckCircle size={14} className="text-green-400" />
  ) : isPlanning ? (
    <FolderTree size={14} className="text-purple-400 animate-pulse" />
  ) : isAnalyzing ? (
    <Brain size={14} className="text-blue-400 animate-pulse" />
  ) : isScanning ? (
    <FileSearch size={14} className="text-orange-400 animate-pulse" />
  ) : (
    <Loader2 size={14} className="text-gray-400 animate-spin" />
  );

  // Get phase display name
  const phaseDisplayName = (() => {
    switch (phase) {
      case 'scanning':
        return 'Scanning files';
      case 'extracting':
        return 'Extracting text';
      case 'rendering':
        return 'Rendering PDFs';
      case 'analyzing':
        return 'Analyzing content';
      case 'summarizing':
        return 'Summarizing results';
      case 'planning':
        return 'Creating organization plan';
      case 'complete':
        return 'Analysis complete';
      default:
        return 'Processing';
    }
  })();

  return (
    <div
      className={cn(
        'p-3 rounded-lg transition-colors',
        isComplete && 'bg-green-500/10 border border-green-500/20',
        isPlanning && 'bg-purple-500/10 border border-purple-500/20',
        isAnalyzing && 'bg-blue-500/10 border border-blue-500/20',
        isScanning && 'bg-orange-500/10 border border-orange-500/20',
        !isComplete && !isPlanning && !isAnalyzing && !isScanning && 'bg-white/5 border border-white/10',
        className
      )}
    >
      {/* Header row */}
      <div className="flex items-center gap-2 mb-2">
        {phaseIcon}
        <span
          className={cn(
            'text-xs font-medium',
            isComplete && 'text-green-300',
            isPlanning && 'text-purple-300',
            isAnalyzing && 'text-blue-300',
            isScanning && 'text-orange-300',
            !isComplete && !isPlanning && !isAnalyzing && !isScanning && 'text-gray-300'
          )}
        >
          {phaseDisplayName}
        </span>
      </div>

      {/* Progress bar */}
      <div className="h-1.5 bg-white/10 rounded-full overflow-hidden">
        <div
          className={cn(
            'h-full rounded-full transition-all duration-300',
            isComplete && 'bg-green-500',
            isPlanning && 'bg-purple-500',
            isAnalyzing && 'bg-blue-500',
            isScanning && 'bg-orange-500',
            !isComplete && !isPlanning && !isAnalyzing && !isScanning && 'bg-gray-500'
          )}
          style={{ width: `${Math.min(percentage, 100)}%` }}
        />
      </div>

      {/* Progress text */}
      <div className="flex items-center justify-between mt-1.5">
        <span className="text-[10px] text-gray-500 truncate max-w-[60%]">
          {message}
        </span>
        <span className="text-[10px] text-gray-500 tabular-nums">
          {total > 0 ? `${current}/${total}` : ''} {percentage > 0 ? `(${percentage.toFixed(0)}%)` : ''}
        </span>
      </div>
    </div>
  );
}
