import { useRef, useEffect } from 'react';
import {
  X,
  CheckCircle,
  XCircle,
  Sparkles,
  Terminal,
  FileSearch,
  Lightbulb,
  ChevronRight,
  Loader2,
  FolderOpen,
  FileText,
  FileType,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import {
  useOrganizeStore,
  type AIThought,
  type ThoughtType,
} from '../../stores/organize-store';
import { ConventionSelector } from './ConventionSelector';
import './ChangesPanel.css';

export function ChangesPanel() {
  const {
    isOpen,
    targetFolder,
    thoughts,
    currentPhase,
    currentPlan,
    isExecuting,
    executedOps,
    closeOrganizer,
    awaitingConventionSelection,
    suggestedConventions,
    selectConvention,
    skipConventionSelection,
  } = useOrganizeStore();

  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when new thoughts arrive
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [thoughts]);

  if (!isOpen) return null;

  const folderName = targetFolder?.split('/').pop() || 'Folder';
  const completedCount = executedOps.length;
  const totalCount = currentPlan?.operations.length || 0;
  const isComplete = currentPhase === 'complete';
  const hasError = currentPhase === 'error';
  const isWorking = !isComplete && !hasError;

  return (
    <div className="w-96 h-full flex flex-col border-l border-white/5 glass-sidebar">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2.5 border-b border-white/5">
        <div className="flex items-center gap-2">
          <div className="w-6 h-6 rounded-lg bg-gradient-to-br from-orange-500 to-orange-600 flex items-center justify-center shadow-sm">
            <Sparkles size={13} className="text-white" />
          </div>
          <span className="text-sm font-medium text-gray-100">AI Organizer</span>
        </div>
        <button
          onClick={closeOrganizer}
          disabled={isExecuting}
          className="p-1.5 rounded-md hover:bg-white/5 text-gray-500 hover:text-gray-300 transition-colors disabled:opacity-30"
        >
          <X size={14} />
        </button>
      </div>

      {/* Target folder context */}
      <div className="px-3 py-2 border-b border-white/5 bg-white/[0.02]">
        <div className="flex items-center gap-2">
          <FolderOpen size={14} className="text-orange-500/70" />
          <span className="text-xs text-gray-400 truncate">{folderName}</span>
        </div>
      </div>

      {/* Activity feed */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto">
        <div className="p-2 space-y-1">
          {thoughts.map((thought, index) => (
            <ActivityItem
              key={thought.id}
              thought={thought}
              isLatest={index === thoughts.length - 1 && isWorking && !awaitingConventionSelection}
            />
          ))}

          {/* Naming Convention Selection */}
          {awaitingConventionSelection && suggestedConventions && (
            <ConventionSelector
              conventions={suggestedConventions}
              onSelect={selectConvention}
              onSkip={skipConventionSelection}
            />
          )}

          {/* Execution progress */}
          {isExecuting && currentPlan && (
            <div className="mt-2 p-2 rounded-lg bg-orange-500/10 border border-orange-500/20">
              <div className="flex items-center gap-2 mb-2">
                <Loader2 size={12} className="text-orange-400 animate-spin" />
                <span className="text-xs font-medium text-orange-300">
                  Applying changes
                </span>
              </div>
              <div className="flex items-center gap-2">
                <div className="flex-1 h-1 bg-white/10 rounded-full overflow-hidden">
                  <div
                    className="h-full bg-orange-500 rounded-full transition-all duration-300"
                    style={{ width: `${(completedCount / totalCount) * 100}%` }}
                  />
                </div>
                <span className="text-[10px] text-gray-400 tabular-nums">
                  {completedCount}/{totalCount}
                </span>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Status footer */}
      <StatusFooter
        isComplete={isComplete}
        hasError={hasError}
        totalCount={totalCount}
        currentPhase={currentPhase}
      />
    </div>
  );
}

// Individual activity item
function ActivityItem({ thought, isLatest }: { thought: AIThought; isLatest: boolean }) {
  const icon = getThoughtIcon(thought.type);
  const isToolCall = thought.type === 'executing' || thought.content.includes('Running');
  const isThinking = thought.type === 'thinking';

  // Skip verbose "Processing..." messages
  if (thought.content.includes('Processing... (step')) {
    return null;
  }

  return (
    <div
      className={cn(
        'group rounded-lg p-2 transition-colors',
        isLatest && 'bg-white/[0.03]',
        !isLatest && 'hover:bg-white/[0.02]'
      )}
    >
      <div className="flex items-start gap-2">
        {/* Icon */}
        <div
          className={cn(
            'mt-0.5 w-5 h-5 rounded-md flex items-center justify-center flex-shrink-0',
            thought.type === 'complete' && 'bg-green-500/20 text-green-400',
            thought.type === 'error' && 'bg-red-500/20 text-red-400',
            thought.type === 'executing' && 'bg-orange-500/20 text-orange-400',
            thought.type === 'planning' && 'bg-blue-500/20 text-blue-400',
            thought.type === 'naming_conventions' && 'bg-purple-500/20 text-purple-400',
            (thought.type === 'scanning' || thought.type === 'analyzing') && 'bg-gray-500/20 text-gray-400',
            thought.type === 'thinking' && 'bg-gray-500/10 text-gray-500'
          )}
        >
          {isLatest && thought.type !== 'complete' && thought.type !== 'error' ? (
            <Loader2 size={11} className="animate-spin" />
          ) : (
            icon
          )}
        </div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          <p
            className={cn(
              'text-xs leading-relaxed',
              isThinking ? 'text-gray-500' : 'text-gray-300',
              thought.type === 'error' && 'text-red-400'
            )}
          >
            {formatThoughtContent(thought.content)}
          </p>

          {/* Tool call details */}
          {isToolCall && thought.detail && (
            <div className="mt-1.5 text-[10px] text-gray-500 font-mono bg-black/20 rounded px-1.5 py-1 truncate">
              {thought.detail}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// Format thought content to be more readable
function formatThoughtContent(content: string): string {
  // Clean up common patterns
  if (content.startsWith('Running ')) {
    return content.replace('Running ', '→ ');
  }
  if (content.includes('Exploring folder')) {
    return '🔍 Exploring folder structure';
  }
  if (content.includes('Finalizing')) {
    return '✨ Finalizing organization plan';
  }
  // Truncate long AI responses
  if (content.length > 150) {
    return content.slice(0, 150) + '...';
  }
  return content;
}

// Get icon for thought type
function getThoughtIcon(type: ThoughtType) {
  switch (type) {
    case 'scanning':
      return <FileSearch size={11} />;
    case 'analyzing':
      return <FileText size={11} />;
    case 'naming_conventions':
      return <FileType size={11} />;
    case 'thinking':
      return <Lightbulb size={11} />;
    case 'planning':
      return <Lightbulb size={11} />;
    case 'executing':
      return <Terminal size={11} />;
    case 'complete':
      return <CheckCircle size={11} />;
    case 'error':
      return <XCircle size={11} />;
    default:
      return <ChevronRight size={11} />;
  }
}

// Status footer
function StatusFooter({
  isComplete,
  hasError,
  totalCount,
  currentPhase,
}: {
  isComplete: boolean;
  hasError: boolean;
  totalCount: number;
  currentPhase: ThoughtType;
}) {
  if (isComplete) {
    const message = totalCount === 0 ? 'Already organized' : 'Complete';
    const subMessage = totalCount === 0
      ? 'No changes needed'
      : `${totalCount} ${totalCount === 1 ? 'change' : 'changes'} applied`;

    return (
      <div className="p-3 border-t border-white/5 bg-green-500/5">
        <div className="flex items-center gap-2">
          <div className="w-5 h-5 rounded-full bg-green-500/20 flex items-center justify-center">
            <CheckCircle size={12} className="text-green-400" />
          </div>
          <div>
            <p className="text-xs font-medium text-green-400">{message}</p>
            <p className="text-[10px] text-gray-500">{subMessage}</p>
          </div>
        </div>
      </div>
    );
  }

  if (hasError) {
    return (
      <div className="p-3 border-t border-white/5 bg-red-500/5">
        <div className="flex items-center gap-2">
          <div className="w-5 h-5 rounded-full bg-red-500/20 flex items-center justify-center">
            <XCircle size={12} className="text-red-400" />
          </div>
          <p className="text-xs font-medium text-red-400">Organization failed</p>
        </div>
      </div>
    );
  }

  // Working state - show current phase
  const phaseLabels: Record<ThoughtType, string> = {
    scanning: 'Scanning folder...',
    analyzing: 'Analyzing contents...',
    naming_conventions: 'Choose naming style...',
    thinking: 'Processing...',
    planning: 'Creating plan...',
    executing: 'Applying changes...',
    complete: 'Complete',
    error: 'Error',
  };

  return (
    <div className="p-3 border-t border-white/5">
      <div className="flex items-center gap-2">
        <Loader2 size={12} className="text-orange-400 animate-spin" />
        <p className="text-xs text-gray-400">{phaseLabels[currentPhase]}</p>
      </div>
    </div>
  );
}
