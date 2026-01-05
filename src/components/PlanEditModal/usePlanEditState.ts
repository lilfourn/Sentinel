import { useState, useMemo, useCallback } from 'react';
import type { EditableOperation, OperationGroup, PlanEditStats } from '../../types/plan-edit';
import { buildOperationGroups, filterOperationsBySearch } from './planEditUtils';

interface UsePlanEditStateReturn {
  /** Operation groups for display */
  groups: OperationGroup[];
  /** Filtered groups based on search */
  filteredGroups: OperationGroup[];
  /** Set of expanded folder paths */
  expandedPaths: Set<string>;
  /** Toggle expansion of a group */
  toggleExpand: (path: string) => void;
  /** Expand all groups */
  expandAll: () => void;
  /** Collapse all groups */
  collapseAll: () => void;
  /** Current search term */
  searchTerm: string;
  /** Update search term */
  setSearchTerm: (term: string) => void;
  /** Statistics for header display */
  stats: PlanEditStats;
}

/**
 * Hook to manage plan edit modal state.
 * Handles grouping, expansion, and search.
 */
export function usePlanEditState(operations: EditableOperation[]): UsePlanEditStateReturn {
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [searchTerm, setSearchTerm] = useState('');

  // Build groups from operations
  const groups = useMemo(() => buildOperationGroups(operations), [operations]);

  // Calculate stats
  const stats = useMemo<PlanEditStats>(() => {
    const enabled = operations.filter((op) => op.enabled).length;
    const modified = operations.filter((op) => op.isModified).length;
    return {
      total: operations.length,
      enabled,
      disabled: operations.length - enabled,
      modified,
    };
  }, [operations]);

  // Filter groups based on search
  const filteredGroups = useMemo(() => {
    if (!searchTerm.trim()) return groups;

    // Filter operations in each group and rebuild
    return groups
      .map((group) => ({
        ...group,
        operations: filterOperationsBySearch(group.operations, searchTerm),
      }))
      .filter((group) => group.operations.length > 0);
  }, [groups, searchTerm]);

  // Toggle expansion of a single group
  const toggleExpand = useCallback((path: string) => {
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  // Expand all groups
  const expandAll = useCallback(() => {
    setExpandedPaths(new Set(groups.map((g) => g.targetFolder)));
  }, [groups]);

  // Collapse all groups
  const collapseAll = useCallback(() => {
    setExpandedPaths(new Set());
  }, []);

  return {
    groups,
    filteredGroups,
    expandedPaths,
    toggleExpand,
    expandAll,
    collapseAll,
    searchTerm,
    setSearchTerm,
    stats,
  };
}
