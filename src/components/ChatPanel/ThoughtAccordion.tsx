import { useState } from 'react';
import { ChevronDown, ChevronRight, Loader2, CheckCircle, AlertCircle, Search, FileText, FolderOpen, List, Terminal } from 'lucide-react';
import type { ThoughtStep } from '../../stores/chat-store';

interface ThoughtAccordionProps {
  thoughts: ThoughtStep[];
}

const TOOL_ICONS: Record<string, React.ReactNode> = {
  search_hybrid: <Search size={12} />,
  read_file: <FileText size={12} />,
  inspect_pattern: <FolderOpen size={12} />,
  list_directory: <List size={12} />,
  bash: <Terminal size={12} />,
  grep: <Search size={12} />,
};

// Terminal tools get special styling
const TERMINAL_TOOLS = ['bash', 'grep'];

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

function ThoughtItem({ thought }: { thought: ThoughtStep }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const hasOutput = thought.output && thought.output.length > 0;
  const isTerminal = TERMINAL_TOOLS.includes(thought.tool);

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

        {/* Output (always show for terminal commands when available) */}
        {hasOutput && (
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
}

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
