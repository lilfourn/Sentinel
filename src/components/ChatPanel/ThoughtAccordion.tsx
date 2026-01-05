import { useState, memo } from 'react';
import { ChevronDown, ChevronRight, Loader2, CheckCircle, AlertCircle, Search, FileText, FolderOpen, List, Terminal, ShieldCheck, ShieldAlert } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import type { ThoughtStep } from '../../stores/chat-store';

/** Parse NEEDS_APPROVAL error format: NEEDS_APPROVAL|command|reason|message */
function parseNeedsApproval(output: string): { command: string; reason: string; message: string } | null {
  if (!output?.startsWith('NEEDS_APPROVAL|')) return null;

  const parts = output.split('|');
  if (parts.length < 4) return null;

  return {
    command: parts[1].replace(/\\\|/g, '|'),
    reason: parts[2].replace(/\\\|/g, '|'),
    message: parts.slice(3).join('|').replace(/\\\|/g, '|'),
  };
}

interface ThoughtAccordionProps {
  thoughts: ThoughtStep[];
}

const TOOL_ICONS: Record<string, React.ReactNode> = {
  search_hybrid: <Search size={12} />,
  read_file: <FileText size={12} />,
  inspect_pattern: <FolderOpen size={12} />,
  list_directory: <List size={12} />,
  bash: <Terminal size={12} />,
  shell: <Terminal size={12} />,
  grep: <Search size={12} />,
};

// Terminal tools get special styling
const TERMINAL_TOOLS = ['bash', 'grep', 'shell'];

function StatusIcon({ status }: { status: ThoughtStep['status'] }) {
  switch (status) {
    case 'pending':
      return <div className="w-3 h-3 rounded-full border border-gray-400" />;
    case 'running':
      return <Loader2 size={12} className="animate-spin text-orange-500" />;
    case 'complete':
      return <CheckCircle size={12} className="text-green-500" />;
    case 'error':
      return <AlertCircle size={12} className="text-red-500" />;
  }
}

/**
 * Memoized thought item - prevents re-renders when parent accordion toggles
 */
const ThoughtItem = memo(function ThoughtItem({ thought }: { thought: ThoughtStep }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const [approvalState, setApprovalState] = useState<'idle' | 'approving' | 'approved' | 'denied'>('idle');
  const hasOutput = thought.output && thought.output.length > 0;
  const isTerminal = TERMINAL_TOOLS.includes(thought.tool);

  // Check if this is a NEEDS_APPROVAL error
  const approvalInfo = thought.status === 'error' ? parseNeedsApproval(thought.output || '') : null;

  const handleApprove = async (asPattern: boolean) => {
    if (!approvalInfo) return;
    setApprovalState('approving');
    try {
      await invoke('allow_shell_command', {
        command: approvalInfo.command,
        asPattern
      });
      setApprovalState('approved');
    } catch (e) {
      console.error('Failed to approve command:', e);
      setApprovalState('denied');
    }
  };

  // Terminal-style display for bash/grep
  if (isTerminal) {
    return (
      <div className="border-l-2 border-green-500/50 pl-2 py-1">
        {/* Command header with $ prompt */}
        <div className="flex items-center gap-2 text-xs">
          <StatusIcon status={thought.status} />
          <span className="text-green-500 font-mono font-bold">$</span>
          <code className="text-gray-300 font-mono truncate flex-1">
            {thought.input.slice(0, 80)}
            {thought.input.length > 80 && '...'}
          </code>
        </div>

        {/* Approval UI for blocked commands */}
        {approvalInfo && (
          <div className="mt-2 ml-4 p-3 bg-amber-900/30 border border-amber-500/40 rounded-lg">
            <div className="flex items-start gap-2 mb-2">
              <ShieldAlert size={14} className="text-amber-400 mt-0.5 flex-shrink-0" />
              <div className="text-xs">
                <p className="text-amber-200 font-medium">{approvalInfo.reason}</p>
                <p className="text-amber-300/70 mt-1">{approvalInfo.message}</p>
              </div>
            </div>

            {approvalState === 'idle' && (
              <div className="flex gap-2 mt-3">
                <button
                  onClick={() => handleApprove(false)}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-white text-xs font-medium rounded transition-colors"
                >
                  <ShieldCheck size={12} />
                  Allow Once
                </button>
                <button
                  onClick={() => handleApprove(true)}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-green-600 hover:bg-green-500 text-white text-xs font-medium rounded transition-colors"
                >
                  <ShieldCheck size={12} />
                  Always Allow
                </button>
              </div>
            )}

            {approvalState === 'approving' && (
              <div className="flex items-center gap-2 mt-3 text-xs text-amber-300">
                <Loader2 size={12} className="animate-spin" />
                Saving permission...
              </div>
            )}

            {approvalState === 'approved' && (
              <div className="flex items-center gap-2 mt-3 text-xs text-green-400">
                <CheckCircle size={12} />
                Permission granted. Re-run the command to continue.
              </div>
            )}
          </div>
        )}

        {/* Output (always show for terminal commands when available, but hide NEEDS_APPROVAL raw format) */}
        {hasOutput && !approvalInfo && (
          <div className="mt-1 ml-4 p-2 bg-gray-950 rounded text-xs font-mono text-gray-300 whitespace-pre-wrap overflow-x-auto max-h-48 overflow-y-auto">
            {thought.output}
          </div>
        )}
      </div>
    );
  }

  // Standard tool display
  return (
    <div className="border-l-2 border-gray-300 dark:border-gray-600 pl-2 py-1">
      <button
        onClick={() => hasOutput && setIsExpanded(!isExpanded)}
        className={`flex items-center gap-2 w-full text-left text-xs ${
          hasOutput ? 'cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-700' : 'cursor-default'
        } rounded px-1 py-0.5`}
        disabled={!hasOutput}
      >
        {hasOutput && (
          isExpanded ? <ChevronDown size={10} /> : <ChevronRight size={10} />
        )}
        <StatusIcon status={thought.status} />
        <span className="text-gray-500 dark:text-gray-400">
          {TOOL_ICONS[thought.tool] || null}
        </span>
        <span className="font-medium text-gray-700 dark:text-gray-300">
          {thought.tool}
        </span>
        <span className="text-gray-500 dark:text-gray-400 truncate flex-1">
          {thought.input.slice(0, 50)}
          {thought.input.length > 50 && '...'}
        </span>
      </button>

      {isExpanded && thought.output && (
        <div className="mt-1 ml-4 p-2 bg-gray-100 dark:bg-gray-800 rounded text-xs font-mono whitespace-pre-wrap max-h-32 overflow-y-auto">
          {thought.output}
        </div>
      )}
    </div>
  );
});

export function ThoughtAccordion({ thoughts }: ThoughtAccordionProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);

  if (thoughts.length === 0) return null;

  return (
    <div className="mt-2 bg-gray-50 dark:bg-gray-800/50 rounded-lg p-2">
      <button
        onClick={() => setIsCollapsed(!isCollapsed)}
        className="flex items-center gap-1 text-xs text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 w-full"
      >
        {isCollapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
        <span>
          {thoughts.length} tool{thoughts.length !== 1 ? 's' : ''} used
        </span>
      </button>

      {!isCollapsed && (
        <div className="mt-2 space-y-1">
          {thoughts.map((thought) => (
            <ThoughtItem key={thought.id} thought={thought} />
          ))}
        </div>
      )}
    </div>
  );
}
