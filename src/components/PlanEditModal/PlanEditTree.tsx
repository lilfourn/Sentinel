import { memo } from 'react';
import { OperationGroupHeader } from './OperationGroupHeader';
import { OperationEditRow } from './OperationEditRow';
import type { OperationGroup } from '../../types/plan-edit';

interface PlanEditTreeProps {
  groups: OperationGroup[];
  expandedPaths: Set<string>;
  onToggleExpand: (path: string) => void;
  searchTerm: string;
}

export const PlanEditTree = memo(function PlanEditTree({
  groups,
  expandedPaths,
  onToggleExpand,
  searchTerm,
}: PlanEditTreeProps) {
  if (groups.length === 0) {
    return (
      <div className="text-center py-12 text-gray-500">
        {searchTerm ? 'No operations match your search' : 'No operations in plan'}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {groups.map((group) => (
        <div
          key={group.groupId}
          className="rounded-lg bg-white/[0.02] border border-white/5 overflow-hidden"
        >
          <OperationGroupHeader
            group={group}
            isExpanded={expandedPaths.has(group.targetFolder)}
            onToggleExpand={() => onToggleExpand(group.targetFolder)}
          />

          {expandedPaths.has(group.targetFolder) && (
            <div className="divide-y divide-white/5">
              {group.operations.map((op) => (
                <OperationEditRow
                  key={op.opId}
                  operation={op}
                  searchTerm={searchTerm}
                />
              ))}
            </div>
          )}
        </div>
      ))}
    </div>
  );
});
