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
          ? 'border-gray-600/30 bg-gray-800/20 opacity-60'
          : isFirst
            ? 'border-blue-500/30 bg-blue-500/5'
            : 'border-white/10 bg-white/[0.02] hover:bg-white/[0.04]'
      )}
    >
      {/* Timeline connector */}
      {!isFirst && (
        <div className="absolute -top-3 left-6 w-px h-3 bg-white/10" />
      )}

      {/* Header */}
      <div className="flex items-start justify-between gap-2 mb-2">
        <div className="flex items-center gap-2">
          <div
            className={cn(
              'w-5 h-5 rounded-full flex items-center justify-center',
              session.undone
                ? 'bg-gray-600/30'
                : isFirst
                  ? 'bg-blue-500/20'
                  : 'bg-white/10'
            )}
          >
            {session.undone ? (
              <Undo2 size={10} className="text-gray-400" />
            ) : (
              <RotateCcw size={10} className={isFirst ? 'text-blue-400' : 'text-gray-400'} />
            )}
          </div>
          <div className="flex items-center gap-1.5 text-xs text-gray-400">
            <Clock size={10} />
            <span>{timeAgo}</span>
          </div>
        </div>

        {/* Undo button */}
        {!session.undone && (
          <button
            onClick={() => onUndo(session.sessionId)}
            className="px-2 py-1 text-xs rounded-md bg-white/5 hover:bg-white/10 text-gray-300 hover:text-white transition-colors flex items-center gap-1"
          >
            <RotateCcw size={10} />
            <span>Undo</span>
          </button>
        )}

        {session.undone && (
          <span className="flex items-center gap-1 text-xs text-gray-500">
            <CheckCircle2 size={10} />
            <span>Undone</span>
          </span>
        )}
      </div>

      {/* Instruction */}
      <p className="text-sm text-gray-200 mb-1.5 line-clamp-2">
        {session.userInstruction || 'AI Organization'}
      </p>

      {/* Plan description */}
      <p className="text-xs text-gray-400 line-clamp-2 mb-2">
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

      {/* First session indicator */}
      {isFirst && !session.undone && (
        <div className="absolute -right-1 -top-1 px-1.5 py-0.5 text-[10px] font-medium text-blue-400 bg-blue-500/20 rounded-md">
          Latest
        </div>
      )}
    </div>
  );
}
