/**
 * SessionCard - Displays a single organization session with undo button.
 */

import { RotateCcw, Clock, FileStack, CheckCircle2, Undo2 } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { SessionSummary } from '../../types/history';

interface SessionCardProps {
  session: SessionSummary;
  isFirst: boolean;
  onUndo: (sessionId: string) => void;
}

function formatRelativeTime(dateString: string): string {
  const date = new Date(dateString);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return 'Just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (diffDays < 7) return `${diffDays}d ago`;

  return date.toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
  });
}

export function SessionCard({ session, isFirst, onUndo }: SessionCardProps) {
  const timeAgo = formatRelativeTime(session.executedAt);

  return (
    <div
      className={cn(
        'relative rounded-lg border p-3 transition-all',
        session.undone
          ? 'border-gray-300/30 dark:border-gray-600/30 bg-gray-100/50 dark:bg-gray-800/20 opacity-60'
          : isFirst
            ? 'border-orange-500/30 bg-orange-500/5'
            : 'border-black/5 dark:border-white/10 bg-black/[0.02] dark:bg-white/[0.02] hover:bg-black/[0.04] dark:hover:bg-white/[0.04]'
      )}
    >
      {/* Timeline connector */}
      {!isFirst && (
        <div className="absolute -top-3 left-6 w-px h-3 bg-black/10 dark:bg-white/10" />
      )}

      {/* Header */}
      <div className="flex items-start justify-between gap-2 mb-2">
        <div className="flex items-center gap-2">
          <div
            className={cn(
              'w-5 h-5 rounded-full flex items-center justify-center flex-shrink-0',
              session.undone
                ? 'bg-gray-400/30 dark:bg-gray-600/30'
                : isFirst
                  ? 'bg-orange-500/20'
                  : 'bg-black/5 dark:bg-white/10'
            )}
          >
            {session.undone ? (
              <Undo2 size={10} className="text-gray-400" />
            ) : (
              <RotateCcw size={10} className={isFirst ? 'text-orange-500' : 'text-gray-400'} />
            )}
          </div>
          <div className="flex items-center gap-1.5 text-xs text-gray-500 dark:text-gray-400">
            <Clock size={10} />
            <span>{timeAgo}</span>
          </div>
          {/* Latest indicator - inline with time */}
          {isFirst && !session.undone && (
            <span className="px-1.5 py-0.5 text-[10px] font-medium text-orange-600 dark:text-orange-400 bg-orange-500/15 rounded">
              Latest
            </span>
          )}
        </div>

        {/* Undo button */}
        {!session.undone && (
          <button
            onClick={() => onUndo(session.sessionId)}
            className="px-2 py-1 text-xs rounded-md bg-orange-500/10 hover:bg-orange-500/20 text-orange-600 dark:text-orange-400 hover:text-orange-700 dark:hover:text-orange-300 transition-colors flex items-center gap-1 flex-shrink-0"
          >
            <RotateCcw size={10} />
            <span>Undo</span>
          </button>
        )}

        {session.undone && (
          <span className="flex items-center gap-1 text-xs text-gray-500 flex-shrink-0">
            <CheckCircle2 size={10} />
            <span>Undone</span>
          </span>
        )}
      </div>

      {/* Instruction */}
      <p className="text-sm text-gray-800 dark:text-gray-200 mb-1.5 line-clamp-2">
        {session.userInstruction || 'AI Organization'}
      </p>

      {/* Plan description */}
      <p className="text-xs text-gray-500 dark:text-gray-400 line-clamp-2 mb-2">
        {session.planDescription}
      </p>

      {/* Stats */}
      <div className="flex items-center gap-3 text-xs text-gray-500">
        <span className="flex items-center gap-1">
          <FileStack size={10} />
          {session.filesAffected} files
        </span>
        <span className="flex items-center gap-1">
          {session.operationCount} operations
        </span>
      </div>
    </div>
  );
}
