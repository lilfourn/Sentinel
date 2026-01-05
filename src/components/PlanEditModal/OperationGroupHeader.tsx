import { ChevronDown, ChevronRight, Folder, FolderPlus } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useOrganizeStore } from '../../stores/organize-store';
import type { OperationGroup } from '../../types/plan-edit';

interface OperationGroupHeaderProps {
  group: OperationGroup;
  isExpanded: boolean;
  onToggleExpand: () => void;
}

export function OperationGroupHeader({
  group,
  isExpanded,
  onToggleExpand,
}: OperationGroupHeaderProps) {
  const toggleOperationGroup = useOrganizeStore((s) => s.toggleOperationGroup);

  // Check if this group includes a folder creation
  const hasCreateFolder = group.operations.some((op) => op.type === 'create_folder');

  return (
    <div
      className={cn(
        'flex items-center gap-2 px-3 py-2 bg-white/[0.03]',
        'hover:bg-white/[0.05] transition-colors'
      )}
    >
      {/* Checkbox for group toggle */}
      <label
        className="flex items-center cursor-pointer"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          type="checkbox"
          checked={group.allEnabled}
          ref={(el) => {
            if (el) el.indeterminate = group.partialEnabled;
          }}
          onChange={() => toggleOperationGroup(group.targetFolder)}
          className="w-4 h-4 rounded border-gray-600 text-orange-500 focus:ring-orange-500 focus:ring-offset-0 bg-gray-800 cursor-pointer"
        />
      </label>

      {/* Expand/collapse toggle */}
      <button
        onClick={onToggleExpand}
        className="p-0.5 rounded hover:bg-white/10"
      >
        {isExpanded ? (
          <ChevronDown size={14} className="text-gray-500" />
        ) : (
          <ChevronRight size={14} className="text-gray-500" />
        )}
      </button>

      {/* Folder icon */}
      {hasCreateFolder ? (
        <FolderPlus size={16} className="text-orange-400 flex-shrink-0" />
      ) : (
        <Folder size={16} className="text-orange-500/70 flex-shrink-0" />
      )}

      {/* Folder name */}
      <button
        onClick={onToggleExpand}
        className={cn(
          'flex-1 text-left text-sm font-medium truncate',
          group.allEnabled ? 'text-gray-200' : 'text-gray-500'
        )}
      >
        {group.displayName}
      </button>

      {/* Count badge */}
      <span className="text-xs text-gray-500 tabular-nums">
        {group.enabledCount}/{group.totalCount}
      </span>
    </div>
  );
}
