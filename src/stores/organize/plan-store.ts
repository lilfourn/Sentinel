import { create } from 'zustand';
import type { EditableOperation } from '../../types/plan-edit';

// Re-export for convenience
export type { EditableOperation } from '../../types/plan-edit';

export interface OrganizeOperation {
  opId: string;
  type: 'create_folder' | 'move' | 'rename' | 'trash' | 'copy';
  source?: string;
  destination?: string;
  path?: string;
  newName?: string;
  riskLevel: 'low' | 'medium' | 'high';
}

export interface OrganizePlan {
  planId: string;
  description: string;
  operations: OrganizeOperation[];
  targetFolder: string;
  simplificationRecommended?: boolean;
}

/**
 * Valid operation types for organize operations.
 */
const VALID_OP_TYPES = ['create_folder', 'move', 'rename', 'trash', 'copy'] as const;

/**
 * Runtime type guard for OrganizeOperation.
 * Validates that a backend operation matches the expected shape.
 */
export function isValidOperation(op: unknown): op is OrganizeOperation {
  if (typeof op !== 'object' || op === null) return false;
  const o = op as Record<string, unknown>;
  return (
    typeof o.opId === 'string' &&
    typeof o.type === 'string' &&
    VALID_OP_TYPES.includes(o.type as typeof VALID_OP_TYPES[number]) &&
    (o.source === undefined || typeof o.source === 'string') &&
    (o.destination === undefined || typeof o.destination === 'string') &&
    (o.path === undefined || typeof o.path === 'string') &&
    (o.newName === undefined || typeof o.newName === 'string')
    // Note: riskLevel is added by frontend, not validated from backend
  );
}

/**
 * Runtime type guard for OrganizePlan.
 * Validates that a backend plan matches the expected shape.
 */
export function isValidOrganizePlan(plan: unknown): plan is Omit<OrganizePlan, 'operations'> & { operations: unknown[] } {
  if (typeof plan !== 'object' || plan === null) return false;
  const p = plan as Record<string, unknown>;
  return (
    typeof p.planId === 'string' &&
    typeof p.description === 'string' &&
    typeof p.targetFolder === 'string' &&
    Array.isArray(p.operations)
  );
}

/**
 * Validates and parses a backend plan result.
 * Returns the plan with validated operations, or throws a descriptive error.
 */
export function parseOrganizePlan(value: unknown): OrganizePlan {
  if (!isValidOrganizePlan(value)) {
    const received = JSON.stringify(value, null, 2);
    throw new Error(
      `Invalid OrganizePlan from backend. ` +
      `Expected {planId: string, description: string, targetFolder: string, operations: array}. ` +
      `Received: ${received.slice(0, 300)}${received.length > 300 ? '...' : ''}`
    );
  }

  // Validate each operation
  const validOperations: OrganizeOperation[] = [];
  const invalidOps: number[] = [];

  for (let i = 0; i < value.operations.length; i++) {
    const op = value.operations[i];
    if (isValidOperation(op)) {
      // Add default riskLevel if not present
      validOperations.push({
        ...op,
        riskLevel: (op as OrganizeOperation).riskLevel || 'medium',
      });
    } else {
      invalidOps.push(i);
    }
  }

  if (invalidOps.length > 0) {
    console.warn(`[Plan] Skipped ${invalidOps.length} invalid operations at indices:`, invalidOps);
  }

  // Safely validate simplificationRecommended field
  const rawSimplification = (value as Record<string, unknown>).simplificationRecommended;
  const simplificationRecommended = typeof rawSimplification === 'boolean' ? rawSimplification : undefined;

  return {
    planId: value.planId,
    description: value.description,
    targetFolder: value.targetFolder,
    operations: validOperations,
    simplificationRecommended,
  };
}

// Analysis progress for the progress bar during AI analysis
export interface AnalysisProgressState {
  current: number;
  total: number;
  phase: string;
  message: string;
}

interface PlanState {
  currentPlan: OrganizePlan | null;
  userInstruction: string;
  awaitingInstruction: boolean;
  isAnalyzing: boolean;
  analysisError: string | null;
  analysisProgress: AnalysisProgressState | null;
  awaitingSimplificationChoice: boolean;
  editableOperations: EditableOperation[];
  isPlanEditModalOpen: boolean;
  _analysisCleanup: (() => void) | null;
}

interface PlanActions {
  setPlan: (plan: OrganizePlan | null) => void;
  setUserInstruction: (instruction: string) => void;
  setAwaitingInstruction: (awaiting: boolean) => void;
  setAnalyzing: (analyzing: boolean) => void;
  setAnalysisError: (error: string | null) => void;
  setAnalysisProgress: (progress: AnalysisProgressState | null) => void;
  setAwaitingSimplificationChoice: (awaiting: boolean) => void;
  setAnalysisCleanup: (cleanup: (() => void) | null) => void;

  // Plan edit modal actions
  openPlanEditModal: () => void;
  closePlanEditModal: () => void;
  setEditableOperations: (ops: EditableOperation[]) => void;
  toggleOperation: (opId: string) => void;
  toggleOperationGroup: (targetFolder: string) => void;
  updateOperationDestination: (opId: string, newDestination: string) => void;
  updateOperationNewName: (opId: string, newName: string) => void;

  // Reset
  resetPlan: () => void;
}

export const usePlanStore = create<PlanState & PlanActions>((set, get) => ({
  // Initial state
  currentPlan: null,
  userInstruction: '',
  awaitingInstruction: false,
  isAnalyzing: false,
  analysisError: null,
  analysisProgress: null,
  awaitingSimplificationChoice: false,
  editableOperations: [],
  isPlanEditModalOpen: false,
  _analysisCleanup: null,

  // Actions
  setPlan: (plan) => set({ currentPlan: plan }),
  setUserInstruction: (instruction) => set({ userInstruction: instruction }),
  setAwaitingInstruction: (awaiting) => set({ awaitingInstruction: awaiting }),
  setAnalyzing: (analyzing) => set({ isAnalyzing: analyzing }),
  setAnalysisError: (error) => set({ analysisError: error }),
  setAnalysisProgress: (progress) => set({ analysisProgress: progress }),
  setAwaitingSimplificationChoice: (awaiting) => set({ awaitingSimplificationChoice: awaiting }),
  setAnalysisCleanup: (cleanup) => set({ _analysisCleanup: cleanup }),

  // Plan edit modal actions
  openPlanEditModal: () => {
    const { currentPlan } = get();
    if (!currentPlan) return;

    // Create editable copies of all operations
    const editableOps: EditableOperation[] = currentPlan.operations.map((op) => ({
      ...op,
      enabled: true,
      isModified: false,
      originalDestination: op.destination,
      originalNewName: op.newName,
    }));

    set({
      isPlanEditModalOpen: true,
      editableOperations: editableOps,
    });
  },

  closePlanEditModal: () => {
    set({
      isPlanEditModalOpen: false,
      editableOperations: [],
    });
  },

  setEditableOperations: (ops) => set({ editableOperations: ops }),

  toggleOperation: (opId: string) => {
    set((state) => ({
      editableOperations: state.editableOperations.map((op) =>
        op.opId === opId ? { ...op, enabled: !op.enabled, isModified: true } : op
      ),
    }));
  },

  toggleOperationGroup: (targetFolder: string) => {
    const { editableOperations } = get();

    // Find operations in this group
    const groupOps = editableOperations.filter((op) => {
      if (op.type === 'create_folder') {
        return op.path === targetFolder;
      }
      if (op.type === 'move' || op.type === 'copy') {
        const destFolder = op.destination?.split('/').slice(0, -1).join('/');
        return destFolder === targetFolder;
      }
      if (op.type === 'rename') {
        const parentFolder = op.path?.split('/').slice(0, -1).join('/');
        return parentFolder === targetFolder;
      }
      return false;
    });

    // Determine new state (if any enabled, disable all; otherwise enable all)
    const anyEnabled = groupOps.some((op) => op.enabled);
    const newEnabled = !anyEnabled;

    set((state) => ({
      editableOperations: state.editableOperations.map((op) => {
        const inGroup = groupOps.find((g) => g.opId === op.opId);
        if (inGroup) {
          return { ...op, enabled: newEnabled, isModified: true };
        }
        return op;
      }),
    }));
  },

  updateOperationDestination: (opId: string, newDestination: string) => {
    set((state) => ({
      editableOperations: state.editableOperations.map((op) =>
        op.opId === opId
          ? { ...op, destination: newDestination, isModified: true }
          : op
      ),
    }));
  },

  updateOperationNewName: (opId: string, newName: string) => {
    set((state) => ({
      editableOperations: state.editableOperations.map((op) =>
        op.opId === opId ? { ...op, newName, isModified: true } : op
      ),
    }));
  },

  resetPlan: () => {
    // Clean up any active listeners
    const { _analysisCleanup } = get();
    _analysisCleanup?.();

    set({
      currentPlan: null,
      userInstruction: '',
      awaitingInstruction: false,
      isAnalyzing: false,
      analysisError: null,
      analysisProgress: null,
      awaitingSimplificationChoice: false,
      editableOperations: [],
      isPlanEditModalOpen: false,
      _analysisCleanup: null,
    });
  },
}));
