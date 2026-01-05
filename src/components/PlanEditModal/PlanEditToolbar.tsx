import { Search, ChevronDown, ChevronUp } from 'lucide-react';

interface PlanEditToolbarProps {
  searchTerm: string;
  onSearchChange: (term: string) => void;
  onExpandAll: () => void;
  onCollapseAll: () => void;
  enabledCount: number;
  totalCount: number;
}

export function PlanEditToolbar({
  searchTerm,
  onSearchChange,
  onExpandAll,
  onCollapseAll,
  enabledCount,
  totalCount,
}: PlanEditToolbarProps) {
  return (
    <div className="flex items-center gap-3 px-5 py-3 border-b border-white/5 bg-white/[0.01]">
      {/* Search input */}
      <div className="relative flex-1 max-w-sm">
        <Search
          size={14}
          className="absolute left-3 top-1/2 -translate-y-1/2 text-gray-500"
        />
        <input
          type="text"
          value={searchTerm}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder="Search files..."
          className="w-full pl-9 pr-3 py-1.5 text-sm bg-[#1a1a1a] border border-gray-700 rounded-lg text-gray-200 placeholder-gray-500 focus:outline-none focus:border-orange-500/50"
        />
      </div>

      {/* Expand/Collapse buttons */}
      <div className="flex items-center gap-1">
        <button
          onClick={onExpandAll}
          className="flex items-center gap-1 px-2 py-1 text-xs text-gray-400 hover:text-gray-200 hover:bg-white/5 rounded transition-colors"
        >
          <ChevronDown size={12} />
          Expand All
        </button>
        <button
          onClick={onCollapseAll}
          className="flex items-center gap-1 px-2 py-1 text-xs text-gray-400 hover:text-gray-200 hover:bg-white/5 rounded transition-colors"
        >
          <ChevronUp size={12} />
          Collapse All
        </button>
      </div>

      {/* Selection count */}
      <span className="text-xs text-gray-500 tabular-nums">
        {enabledCount}/{totalCount}
      </span>
    </div>
  );
}
