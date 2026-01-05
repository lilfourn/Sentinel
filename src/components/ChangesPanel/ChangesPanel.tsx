import { useRef, useEffect, useState } from 'react';
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
  type ExecutionError,
} from '../../stores/organize-store';
import { ErrorDetailDialog } from './ErrorDetailDialog';
import { OrganizeMethodSelector } from './OrganizeMethodSelector';
import { DynamicStatus } from './DynamicStatus';
import { SimulationControls } from './SimulationControls';
import { ExecutionProgress } from './ExecutionProgress';
import { AnalysisProgressBar } from './AnalysisProgressBar';
import { PlanPreview } from './PlanPreview';
import './ChangesPanel.css';

export function ChangesPanel() {
  const {
    isOpen,
    targetFolder,
    thoughts,
    currentPhase,
    currentPlan,
    isExecuting,
    isAnalyzing,
    executedOps,
    closeOrganizer,
    setUserInstruction,
    submitInstruction,
    awaitingInstruction,
    phase,
    analysisError,
    latestEvent,
    executionProgress,
    acceptPlanParallel,
    rejectPlan,
    analysisProgress,
    executionErrors,
    openPlanEditModal,
  } = useOrganizeStore();

  const scrollRef = useRef<HTMLDivElement>(null);

  // Track whether error detail dialog is shown
  const [showErrorDialog, setShowErrorDialog] = useState(false);

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
  const isComplete = currentPhase === 'complete' || phase === 'complete';
  const hasError = currentPhase === 'error' || phase === 'failed';
  const isWorking = !isComplete && !hasError;

  return (
    <div className="w-96 h-full flex flex-col border-l border-white/5 glass-sidebar">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2.5 border-b border-white/5">
        <div className="flex items-center gap-2">
          <div className="w-6 h-6 rounded-lg bg-gradient-to-br from-orange-500 to-orange-600 flex items-center justify-center shadow-sm">
            <Sparkles size={13} className="text-white" />
          </div>
          <span className="text-sm font-medium text-gray-100">Sentinel</span>
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
              isLatest={index === thoughts.length - 1 && isWorking && !awaitingInstruction}
            />
          ))}

          {/* Analysis Progress Bar - shown during AI analysis phase */}
          {isAnalyzing && analysisProgress && (
            <AnalysisProgressBar
              current={analysisProgress.current}
              total={analysisProgress.total}
              phase={analysisProgress.phase}
              message={analysisProgress.message}
              className="mt-2"
            />
          )}

          {/* Organization Method Selector - shown when awaiting user instruction */}
          {awaitingInstruction && (
            <OrganizeMethodSelector
              onSelect={(instruction) => {
                setUserInstruction(instruction);
                // Small delay to ensure state is set before submit
                setTimeout(submitInstruction, 0);
              }}
              isDisabled={isAnalyzing}
              folderName={folderName}
            />
          )}

          {/* V5: Simulation Controls - shown when plan is ready for approval */}
          {phase === 'simulation' && currentPlan && (
            <div className="mt-3 p-3 rounded-lg bg-white/[0.03] border border-white/10">
              <div className="mb-3">
                <p className="text-xs text-gray-300 font-medium">
                  Ready to organize {currentPlan.operations.length} files
                </p>
                <p className="text-[10px] text-gray-500 mt-1">
                  Review the changes below before applying
                </p>
              </div>

              {/* Plan Preview - shows what will change */}
              <PlanPreview
                plan={currentPlan}
                onEditClick={openPlanEditModal}
                className="mb-4"
              />

              <SimulationControls
                hasConflicts={false}
                conflictCount={0}
                isApplying={false}
                onApply={acceptPlanParallel}
                onCancel={rejectPlan}
                onEdit={openPlanEditModal}
              />
            </div>
          )}

          {/* V5: Execution progress - clean progress bar instead of per-operation list */}
          {executionProgress && (
            <ExecutionProgress
              completed={executionProgress.completed}
              total={executionProgress.total}
              phase={executionProgress.phase}
              className="mt-2"
            />
          )}

          {/* Fallback: Old execution progress for cases without executionProgress state */}
          {isExecuting && currentPlan && !executionProgress && (
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
        errorDetail={analysisError}
        latestEvent={latestEvent}
        executionErrors={executionErrors}
        onViewErrors={() => setShowErrorDialog(true)}
      />

      {/* Error detail dialog */}
      {showErrorDialog && executionErrors.length > 0 && (
        <ErrorDetailDialog
          errors={executionErrors}
          onClose={() => setShowErrorDialog(false)}
        />
      )}
    </div>
  );
}

// Individual activity item with expand/collapse support
function ActivityItem({ thought, isLatest }: { thought: AIThought; isLatest: boolean }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const icon = getThoughtIcon(thought.type);
  const isToolCall = thought.type === 'executing' || thought.content.includes('Running');
  const isThinking = thought.type === 'thinking';
  const hasExpandableContent = thought.expandableDetails && thought.expandableDetails.length > 0;

  // Skip verbose "Processing..." messages
  if (thought.content.includes('Processing... (step')) {
    return null;
  }

  const handleClick = () => {
    if (hasExpandableContent) {
      setIsExpanded(!isExpanded);
    }
  };

  return (
    <div
      className={cn(
        'group rounded-lg p-2 transition-colors',
        isLatest && 'bg-white/[0.03]',
        !isLatest && 'hover:bg-white/[0.02]',
        hasExpandableContent && 'cursor-pointer'
      )}
      onClick={handleClick}
    >
      <div className="flex items-center gap-2">
        {/* Expand/collapse chevron or icon */}
        <div
          className={cn(
            'w-5 h-5 rounded-md flex items-center justify-center flex-shrink-0',
            thought.type === 'complete' && 'bg-green-500/20 text-green-400',
            thought.type === 'error' && 'bg-red-500/20 text-red-400',
            thought.type === 'executing' && 'bg-orange-500/20 text-orange-400',
            thought.type === 'planning' && 'bg-blue-500/20 text-blue-400',
            thought.type === 'naming_conventions' && 'bg-purple-500/20 text-purple-400',
            (thought.type === 'scanning' || thought.type === 'analyzing') && 'bg-gray-500/20 text-gray-400',
            thought.type === 'thinking' && 'bg-gray-500/10 text-gray-500'
          )}
        >
          {hasExpandableContent ? (
            <ChevronRight
              size={11}
              className={cn(
                'transition-transform duration-200',
                isExpanded && 'rotate-90'
              )}
            />
          ) : (
            icon
          )}
        </div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5">
            <p
              className={cn(
                'text-xs leading-relaxed flex-1',
                isThinking ? 'text-gray-500' : 'text-gray-300',
                thought.type === 'error' && 'text-red-400'
              )}
            >
              {formatThoughtContent(thought.content)}
            </p>
            {hasExpandableContent && !isExpanded && (
              <span className="text-[10px] text-gray-500">
                {thought.expandableDetails!.length} details
              </span>
            )}
          </div>

          {/* Tool call details (simple) */}
          {isToolCall && thought.detail && !hasExpandableContent && (
            <div className="mt-1.5 text-[10px] text-gray-500 font-mono bg-black/20 rounded px-1.5 py-1 truncate">
              {thought.detail}
            </div>
          )}

          {/* Expandable details */}
          {hasExpandableContent && isExpanded && (
            <div className="mt-2 space-y-1 pl-1 border-l-2 border-white/10">
              {thought.expandableDetails!.map((detail, idx) => (
                <div key={idx} className="flex items-baseline gap-2 text-[10px]">
                  <span className="text-gray-500 font-medium shrink-0">{detail.label}:</span>
                  <span className="text-gray-400 truncate">{detail.value}</span>
                </div>
              ))}
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
  errorDetail,
  latestEvent,
  executionErrors,
  onViewErrors,
}: {
  isComplete: boolean;
  hasError: boolean;
  totalCount: number;
  currentPhase: ThoughtType;
  errorDetail?: string | null;
  latestEvent: { type: string; detail: string } | null;
  executionErrors: ExecutionError[];
  onViewErrors: () => void;
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
        <div className="flex items-start gap-2">
          <div className="w-5 h-5 rounded-full bg-red-500/20 flex items-center justify-center flex-shrink-0 mt-0.5">
            <XCircle size={12} className="text-red-400" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-xs font-medium text-red-400">Organization failed</p>
            {errorDetail && (
              <p className="text-[10px] text-red-400/70 mt-1 line-clamp-3 break-words">
                {errorDetail}
              </p>
            )}
            {executionErrors.length > 0 && (
              <button
                onClick={onViewErrors}
                className="mt-2 text-[10px] text-red-400 hover:text-red-300 underline transition-colors"
              >
                View all {executionErrors.length} error{executionErrors.length !== 1 ? 's' : ''}
              </button>
            )}
          </div>
        </div>
      </div>
    );
  }

  // Working state - show dynamic status with contextual messages
  return (
    <div className="p-3 border-t border-white/5">
      <DynamicStatus
        eventType={latestEvent?.type || currentPhase}
        detail={latestEvent?.detail}
      />
    </div>
  );
}
