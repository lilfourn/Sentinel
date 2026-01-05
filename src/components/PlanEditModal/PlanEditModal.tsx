import { useEffect, useMemo, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { cn } from '../../lib/utils';
import { useOrganizeStore } from '../../stores/organize-store';
import { PlanEditHeader } from './PlanEditHeader';
import { PlanEditToolbar } from './PlanEditToolbar';
import { PlanEditTree } from './PlanEditTree';
import { PlanEditFooter } from './PlanEditFooter';
import { usePlanEditState } from './usePlanEditState';
import { validatePlanEdits } from './planEditUtils';

export function PlanEditModal() {
  const {
    isPlanEditModalOpen,
    editableOperations,
    closePlanEditModal,
    applyPlanEdits,
    currentPlan,
  } = useOrganizeStore();

  const {
    filteredGroups,
    expandedPaths,
    toggleExpand,
    expandAll,
    collapseAll,
    searchTerm,
    setSearchTerm,
    stats,
  } = usePlanEditState(editableOperations);

  // Validate current edits
  const validationErrors = useMemo(
    () => validatePlanEdits(editableOperations),
    [editableOperations]
  );

  // Handle escape key
  useEffect(() => {
    if (!isPlanEditModalOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        closePlanEditModal();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isPlanEditModalOpen, closePlanEditModal]);

  // Handle apply
  const handleApply = useCallback(() => {
    applyPlanEdits();
  }, [applyPlanEdits]);

  // Handle backdrop click
  const handleBackdropClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget) {
        closePlanEditModal();
      }
    },
    [closePlanEditModal]
  );

  if (!isPlanEditModalOpen || !currentPlan) return null;

  return createPortal(
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60"
      onClick={handleBackdropClick}
      role="dialog"
      aria-modal="true"
      aria-labelledby="plan-edit-title"
    >
      <div
        className={cn(
          'bg-[#2a2a2a] rounded-xl shadow-2xl backdrop-blur-xl',
          'w-[90vw] max-w-4xl h-[85vh] max-h-[800px]',
          'flex flex-col',
          'border border-gray-700',
          'animate-in zoom-in-95 fade-in duration-200'
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <PlanEditHeader stats={stats} onClose={closePlanEditModal} />

        {/* Toolbar */}
        <PlanEditToolbar
          searchTerm={searchTerm}
          onSearchChange={setSearchTerm}
          onExpandAll={expandAll}
          onCollapseAll={collapseAll}
          enabledCount={stats.enabled}
          totalCount={stats.total}
        />

        {/* Tree content - scrollable */}
        <div className="flex-1 overflow-y-auto p-4">
          <PlanEditTree
            groups={filteredGroups}
            expandedPaths={expandedPaths}
            onToggleExpand={toggleExpand}
            searchTerm={searchTerm}
          />
        </div>

        {/* Footer with validation and actions */}
        <PlanEditFooter
          validationErrors={validationErrors}
          enabledCount={stats.enabled}
          hasChanges={stats.modified > 0}
          onCancel={closePlanEditModal}
          onApply={handleApply}
        />
      </div>
    </div>,
    document.body
  );
}
