/**
 * Phase State Machine
 *
 * Provides a unified phase computation based on sub-store states.
 * This replaces the dual phase tracking (currentPhase + phase) with
 * a single computed OrganizePhase.
 *
 * Usage:
 * - Use `computePhase()` directly when you have access to all sub-store states
 * - Use `useComputedPhase()` hook in React components for automatic re-renders
 * - The organize-store still manually manages phase for backward compatibility;
 *   new components should prefer useComputedPhase()
 */

import type { ThoughtType } from './thinking-store';

/**
 * Phase state machine for the organize workflow.
 * This provides a higher-level view of where we are in the process.
 */
export type OrganizePhase =
  | 'idle'                // Not organizing anything
  | 'awaiting_instruction' // Waiting for user to provide instructions
  | 'indexing'            // Scanning folder structure
  | 'planning'            // AI is generating the plan
  | 'simulation'          // Plan is ready, showing ghost preview
  | 'review'              // User is reviewing the plan (diff view)
  | 'committing'          // Executing operations
  | 'rolling_back'        // Undoing completed operations
  | 'complete'            // All done
  | 'failed';             // Error occurred

/**
 * Simplified state interfaces for phase computation.
 * These match the essential fields from the sub-stores.
 */
interface PlanPhaseState {
  currentPlan: unknown | null;
  awaitingInstruction: boolean;
  isAnalyzing: boolean;
  analysisProgress: { current: number; total: number } | null;
  analysisError: string | null;
}

interface ExecutionPhaseState {
  isExecuting: boolean;
  executionProgress: { phase: string } | null;
}

interface RecoveryPhaseState {
  rollbackProgress: { completed: number; total: number } | null;
}

/**
 * Compute the current OrganizePhase from sub-store states.
 * This is the single source of truth for workflow phase.
 */
export function computePhase(
  plan: PlanPhaseState,
  exec: ExecutionPhaseState,
  recovery: RecoveryPhaseState
): OrganizePhase {
  // Recovery states take priority
  if (recovery.rollbackProgress) return 'rolling_back';

  // Execution states
  if (exec.isExecuting) return 'committing';
  if (exec.executionProgress?.phase === 'complete') return 'complete';
  if (exec.executionProgress?.phase === 'failed') return 'failed';

  // Analysis states
  if (plan.isAnalyzing) {
    return plan.analysisProgress ? 'indexing' : 'planning';
  }

  // Plan states
  if (plan.currentPlan && !plan.awaitingInstruction) return 'simulation';
  if (plan.awaitingInstruction) return 'awaiting_instruction';

  // Error state
  if (plan.analysisError) return 'failed';

  return 'idle';
}

/**
 * Convert OrganizePhase to ThoughtType for UI display.
 * This maintains backward compatibility with existing thought visualization.
 */
export function phaseToThoughtType(phase: OrganizePhase): ThoughtType {
  const map: Record<OrganizePhase, ThoughtType> = {
    'idle': 'scanning',
    'awaiting_instruction': 'scanning',
    'indexing': 'scanning',
    'planning': 'analyzing',
    'simulation': 'planning',
    'review': 'planning',
    'committing': 'executing',
    'rolling_back': 'executing',
    'complete': 'complete',
    'failed': 'error',
  };
  return map[phase] || 'scanning';
}

// NOTE: useComputedPhase is defined in organize-store.ts to avoid circular imports.
// It is re-exported from the organize/index.ts barrel file.
