/**
 * Organize Store Module
 *
 * This module provides a facade over the decomposed organize stores:
 * - thinking-store: AI thoughts and thinking stream
 * - recovery-store: WAL recovery and interrupted job handling
 * - session-store: Session lifecycle (open/close/complete)
 * - plan-store: Plan state and editing
 * - execution-store: Execution progress tracking
 *
 * The main organize-store.ts still exists for backward compatibility and
 * delegates to these sub-stores. Eventually, components should migrate
 * to using the sub-stores directly or this facade.
 */

// Re-export sub-stores for direct access
export { useThinkingStore } from './thinking-store';
export type { ThoughtType, ThoughtDetail, AIThought } from './thinking-store';

export { useRecoveryStore } from './recovery-store';
export type { InterruptedJobInfo } from './recovery-store';

export { useSessionStore } from './session-store';

export { usePlanStore } from './plan-store';
export type { OrganizePlan, OrganizeOperation, AnalysisProgressState, EditableOperation } from './plan-store';
export { isValidOrganizePlan, isValidOperation, parseOrganizePlan } from './plan-store';

export { useExecutionStore } from './execution-store';
export type { ExecutionError, ExecutionProgressState } from './execution-store';

// Re-export phase machine (useComputedPhase is in organize-store.ts to avoid circular imports)
export { computePhase, phaseToThoughtType } from './phase-machine';
export type { OrganizePhase } from './phase-machine';
export { useComputedPhase } from '../organize-store';

// NOTE: useOrganizeStore is NOT re-exported here to avoid circular imports.
// Import it directly from '../organize-store' or '../../stores/organize-store'.
