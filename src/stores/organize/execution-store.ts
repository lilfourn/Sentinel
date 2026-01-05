import { create } from 'zustand';
import type { OperationStatus } from '../../types/ghost';

// Detailed execution error for UI display
export interface ExecutionError {
  message: string;
  operationType?: string;
  source?: string;
  destination?: string;
}

// Execution progress tracked from backend events
export interface ExecutionProgressState {
  completed: number;
  total: number;
  phase: 'preparing' | 'executing' | 'complete' | 'failed';
}

interface ExecutionState {
  isExecuting: boolean;
  executedOps: string[];
  failedOp: string | null;
  currentOpIndex: number;
  executionProgress: ExecutionProgressState | null;
  executionErrors: ExecutionError[];
  operationStatuses: Map<string, OperationStatus>;
}

interface ExecutionActions {
  setExecuting: (executing: boolean) => void;
  markOpExecuted: (opId: string) => void;
  markOpFailed: (opId: string) => void;
  setCurrentOpIndex: (index: number) => void;
  resetExecution: () => void;
  setExecutionProgress: (progress: ExecutionProgressState | null) => void;
  setOperationStatus: (opId: string, status: OperationStatus) => void;
  initializeOperationStatuses: (opIds: string[]) => void;
  setExecutionErrors: (errors: ExecutionError[]) => void;
  clearExecution: () => void;
}

export const useExecutionStore = create<ExecutionState & ExecutionActions>((set) => ({
  // Initial state
  isExecuting: false,
  executedOps: [],
  failedOp: null,
  currentOpIndex: -1,
  executionProgress: null,
  executionErrors: [],
  operationStatuses: new Map(),

  // Actions
  setExecuting: (executing) => set({ isExecuting: executing }),

  markOpExecuted: (opId) =>
    set((state) => ({
      executedOps: [...state.executedOps, opId],
    })),

  markOpFailed: (opId) =>
    set({ failedOp: opId, isExecuting: false }),

  setCurrentOpIndex: (index) => set({ currentOpIndex: index }),

  resetExecution: () =>
    set({
      isExecuting: false,
      executedOps: [],
      failedOp: null,
      currentOpIndex: -1,
    }),

  setExecutionProgress: (progress) => set({ executionProgress: progress }),

  setOperationStatus: (opId, status) => {
    set((state) => {
      const newStatuses = new Map(state.operationStatuses);
      newStatuses.set(opId, status);
      return { operationStatuses: newStatuses };
    });
  },

  initializeOperationStatuses: (opIds) => {
    const statuses = new Map<string, OperationStatus>();
    for (const opId of opIds) {
      statuses.set(opId, 'pending');
    }
    set({ operationStatuses: statuses });
  },

  setExecutionErrors: (errors) => set({ executionErrors: errors }),

  clearExecution: () =>
    set({
      isExecuting: false,
      executedOps: [],
      failedOp: null,
      currentOpIndex: -1,
      executionProgress: null,
      executionErrors: [],
      operationStatuses: new Map(),
    }),
}));
