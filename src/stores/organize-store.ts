import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';
import { invoke } from '@tauri-apps/api/core';
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
import type { OperationStatus, WalEntry } from '../types/ghost';
import type { EditableOperation } from '../types/plan-edit';
import { queryClient } from '../lib/query-client';
import { useSubscriptionStore } from './subscription-store';
import { useVfsStore } from './vfs-store';
import { useThinkingStore, type ThoughtType, type ThoughtDetail, type AIThought } from './organize/thinking-store';
import { useRecoveryStore, type InterruptedJobInfo } from './organize/recovery-store';
import { useSessionStore } from './organize/session-store';
import { usePlanStore, type OrganizePlan, type OrganizeOperation, type AnalysisProgressState } from './organize/plan-store';
import { useExecutionStore, type ExecutionError, type ExecutionProgressState } from './organize/execution-store';
import { computePhase, type OrganizePhase } from './organize/phase-machine';

// Re-export types and validators from plan-store for backward compatibility
export type { OrganizePlan, OrganizeOperation, AnalysisProgressState, EditableOperation } from './organize/plan-store';
export { isValidOrganizePlan, isValidOperation, parseOrganizePlan } from './organize/plan-store';
// Re-export types from execution-store for backward compatibility
export type { ExecutionError, ExecutionProgressState } from './organize/execution-store';
// Re-export phase machine types
export type { OrganizePhase } from './organize/phase-machine';

// Re-export types from thinking-store for backward compatibility
export type { ThoughtType, ThoughtDetail, AIThought } from './organize/thinking-store';
// Re-export types from recovery-store for backward compatibility
export type { InterruptedJobInfo } from './organize/recovery-store';

// NOTE: Listener cleanup is now managed via store state (_analysisCleanup)
// instead of module-level variables to prevent memory leaks and race conditions

// Execution result from the parallel DAG executor
interface ExecutionResult {
  completedCount: number;
  failedCount: number;
  skippedCount: number;
  renamedCount: number;
  errors: string[];
  skipped: string[];
  success: boolean;
}

// ExecutionError moved to organize/execution-store.ts
// and re-exported above for backward compatibility

// OrganizeOperation and OrganizePlan types moved to organize/plan-store.ts
// and re-exported above for backward compatibility

// Types ThoughtType, ThoughtDetail, AIThought are now in organize/thinking-store.ts
// and re-exported above for backward compatibility

// OrganizePhase moved to organize/phase-machine.ts
// and re-exported above for backward compatibility

// ExecutionProgressState moved to organize/execution-store.ts
// and re-exported above for backward compatibility

// AnalysisProgressState moved to organize/plan-store.ts
// and re-exported above for backward compatibility

interface OrganizeState {
  // UI state
  isOpen: boolean;
  targetFolder: string | null;

  // Job persistence
  currentJobId: string | null;
  /** True if job persistence failed and we're using a local fallback ID */
  isOfflineMode: boolean;
  /** Error message if job persistence failed */
  persistenceError: string | null;

  // Thinking stream
  thoughts: AIThought[];
  currentPhase: ThoughtType;

  // New phase state machine
  phase: OrganizePhase;
  operationStatuses: Map<string, OperationStatus>;
  wal: WalEntry[];
  rollbackProgress: { completed: number; total: number } | null;

  // Plan state
  currentPlan: OrganizePlan | null;
  isAnalyzing: boolean;
  analysisError: string | null;

  // Execution state
  isExecuting: boolean;
  executedOps: string[];
  failedOp: string | null;
  currentOpIndex: number;

  // Execution progress (V5: replaces per-operation thoughts)
  executionProgress: ExecutionProgressState | null;

  // Analysis progress for progress bar during AI analysis
  analysisProgress: AnalysisProgressState | null;

  // Recovery state
  hasInterruptedJob: boolean;
  interruptedJob: InterruptedJobInfo | null;

  // V6: User instruction for deep organization
  userInstruction: string;
  awaitingInstruction: boolean;

  // Latest event for dynamic status display
  latestEvent: { type: string; detail: string } | null;

  // Completion tracking for auto-refresh
  // When organization completes, these are set to trigger file list refresh
  lastCompletedAt: number | null;
  completedTargetFolder: string | null;

  // Simplification prompt state
  awaitingSimplificationChoice: boolean;

  // Detailed execution errors for error dialog display
  executionErrors: ExecutionError[];

  // Plan edit modal state
  isPlanEditModalOpen: boolean;
  editableOperations: EditableOperation[];

  // Internal: Cleanup function for active analysis listeners (stored in state for proper lifecycle)
  _analysisCleanup: (() => void) | null;
}

// InterruptedJobInfo is now in organize/recovery-store.ts
// and re-exported above for backward compatibility

interface OrganizeActions {
  // Main action - triggers automatic organize
  startOrganize: (folderPath: string) => Promise<void>;
  closeOrganizer: () => void;

  // Thought actions
  addThought: (type: ThoughtType, content: string, detail?: string, expandableDetails?: ThoughtDetail[]) => void;
  setPhase: (phase: ThoughtType) => void;
  clearThoughts: () => void;

  // Plan actions
  setPlan: (plan: OrganizePlan | null) => void;
  setAnalyzing: (analyzing: boolean) => void;
  setAnalysisError: (error: string | null) => void;

  // Execution actions
  setExecuting: (executing: boolean) => void;
  markOpExecuted: (opId: string) => void;
  markOpFailed: (opId: string) => void;
  setCurrentOpIndex: (index: number) => void;
  resetExecution: () => void;

  // Recovery actions (WAL-based)
  checkForInterruptedJob: () => Promise<void>;
  dismissInterruptedJob: () => Promise<void>;
  resumeInterruptedJob: () => Promise<void>;
  rollbackInterruptedJob: () => Promise<void>;

  // V6: User instruction actions
  setUserInstruction: (instruction: string) => void;
  submitInstruction: () => Promise<void>;

  // New phase state machine actions
  transitionTo: (phase: OrganizePhase) => void;
  acceptPlan: () => Promise<void>;
  acceptPlanParallel: () => Promise<void>;
  rejectPlan: () => void;
  startRollback: () => Promise<void>;
  setOperationStatus: (opId: string, status: OperationStatus) => void;

  // V5: Execution progress updates
  setExecutionProgress: (progress: ExecutionProgressState | null) => void;

  // Analysis progress updates for progress bar
  setAnalysisProgress: (progress: AnalysisProgressState | null) => void;

  // Plan edit modal actions
  openPlanEditModal: () => void;
  closePlanEditModal: () => void;
  applyPlanEdits: () => void;
  toggleOperation: (opId: string) => void;
  toggleOperationGroup: (targetFolder: string) => void;
  updateOperationDestination: (opId: string, newDestination: string) => void;
  updateOperationNewName: (opId: string, newName: string) => void;

  // Simplification prompt actions
  acceptSimplification: () => Promise<void>;
  rejectSimplification: () => void;
}

// thoughtId moved to thinking-store

// Mutex for execution to prevent race conditions from rapid double-clicks
// Zustand's set() is not truly atomic, so we need a module-level lock
let executionMutex = false;

// Execute a single operation
async function executeOperation(op: OrganizeOperation): Promise<void> {
  switch (op.type) {
    case 'create_folder':
      await invoke('create_directory', { path: op.path });
      break;
    case 'move':
      await invoke('move_file', { source: op.source, destination: op.destination });
      break;
    case 'rename':
      if (!op.path || !op.newName) {
        throw new Error(`Rename operation missing required fields: path=${op.path}, newName=${op.newName}`);
      }
      const parentPath = op.path.split('/').slice(0, -1).join('/');
      const newPath = `${parentPath}/${op.newName}`;
      await invoke('rename_file', { oldPath: op.path, newPath });
      break;
    case 'trash':
      try {
        await invoke('delete_to_trash', { path: op.path });
      } catch (error) {
        // Check if it's an iCloud download required error
        const errorStr = typeof error === 'object' && error !== null ? JSON.stringify(error) : String(error);
        if (errorStr.includes('I_CLOUD_DOWNLOAD_REQUIRED') || errorStr.includes('-8013') || errorStr.includes('needs to be downloaded')) {
          // Fallback to quarantine for iCloud files during automated operations
          console.log(`[Organize] iCloud file detected, using quarantine fallback for: ${op.path}`);
          await invoke('quarantine_item', { path: op.path });
        } else {
          throw error;
        }
      }
      break;
    case 'copy':
      await invoke('copy_file', { source: op.source, destination: op.destination });
      break;
  }
}

// Helper to add risk level to operations from backend
function addRiskLevels(plan: OrganizePlan): OrganizePlan {
  return {
    ...plan,
    operations: plan.operations.map(op => ({
      ...op,
      riskLevel: getRiskLevel(op.type),
    })),
  };
}

function getRiskLevel(type: string): 'low' | 'medium' | 'high' {
  switch (type) {
    case 'create_folder':
    case 'copy':
      return 'low';
    case 'move':
    case 'rename':
      return 'medium';
    case 'trash':
      return 'high';
    default:
      return 'medium';
  }
}

export const useOrganizeStore = create<OrganizeState & OrganizeActions>((set, get) => ({
  // Initial state
  isOpen: false,
  targetFolder: null,
  currentJobId: null,
  isOfflineMode: false,
  persistenceError: null,
  thoughts: [],
  currentPhase: 'scanning',
  phase: 'idle',
  operationStatuses: new Map(),
  wal: [],
  rollbackProgress: null,
  currentPlan: null,
  isAnalyzing: false,
  analysisError: null,
  isExecuting: false,
  executedOps: [],
  failedOp: null,
  currentOpIndex: -1,
  hasInterruptedJob: false,
  interruptedJob: null,
  userInstruction: '',
  awaitingInstruction: false,
  latestEvent: null,
  executionProgress: null,
  analysisProgress: null,
  lastCompletedAt: null,
  completedTargetFolder: null,
  executionErrors: [],
  awaitingSimplificationChoice: false,

  // Plan edit modal state
  isPlanEditModalOpen: false,
  editableOperations: [],

  // Internal state for listener cleanup
  _analysisCleanup: null,

  // Thought actions - delegate to thinking-store (source of truth)
  // Migration complete: organize-store no longer holds duplicate state
  addThought: (type, content, detail, expandableDetails) => {
    useThinkingStore.getState().addThought(type, content, detail, expandableDetails);
  },

  setPhase: (phase) => {
    useThinkingStore.getState().setPhase(phase);
  },

  clearThoughts: () => {
    useThinkingStore.getState().clearThoughts();
  },

  // Start organize flow - V6: Wait for user instruction
  // Opens the panel and waits for user to provide organization instructions
  startOrganize: async (folderPath: string) => {
    const { isOpen, isAnalyzing, isExecuting } = get();

    // Guard against overlapping sessions
    // If a session is already in progress, ignore the new request
    if (isOpen || isAnalyzing || isExecuting) {
      console.warn('[Organize] Session already in progress, ignoring new request');
      console.warn(`  isOpen=${isOpen}, isAnalyzing=${isAnalyzing}, isExecuting=${isExecuting}`);
      return;
    }

    const state = get();
    state.clearThoughts();

    // Delegate session opening to session-store
    await useSessionStore.getState().openSession(folderPath);

    const folderName = folderPath.split('/').pop() || 'folder';

    // Sync session state and reset workflow state
    const sessionState = useSessionStore.getState();
    set({
      isOpen: sessionState.isOpen,
      targetFolder: sessionState.targetFolder,
      currentJobId: sessionState.currentJobId,
      isOfflineMode: sessionState.isOfflineMode,
      persistenceError: sessionState.persistenceError,
      currentPlan: null,
      isAnalyzing: false,
      analysisError: null,
      isExecuting: false,
      executedOps: [],
      failedOp: null,
      currentOpIndex: -1,
      userInstruction: '',
      awaitingInstruction: true,
      phase: 'idle',
      executionErrors: [],
    });

    state.addThought('scanning', `Ready to organize ${folderName}`, 'Provide instructions to begin');

    // Flow continues when user calls submitInstruction()
  },

  closeOrganizer: () => {
    // Abort any running Grok analysis
    invoke('grok_abort_plan').catch((e) => {
      console.warn('[Organize] Failed to abort Grok plan:', e);
    });

    // Clean up any active event listeners (using state-stored ref)
    const { _analysisCleanup } = get();
    _analysisCleanup?.();

    // Reset all sub-stores to ensure clean state
    useSessionStore.getState().closeSession();
    useThinkingStore.getState().clearThoughts();
    usePlanStore.getState().resetPlan();
    useExecutionStore.getState().clearExecution();
    useRecoveryStore.getState().clearWal();
    useVfsStore.getState().reset();

    // Clear all workflow state in main store
    set({
      isOpen: false,
      targetFolder: null,
      currentJobId: null,
      isOfflineMode: false,
      persistenceError: null,
      thoughts: [],
      currentPhase: 'scanning',
      latestEvent: null,
      phase: 'idle',
      currentPlan: null,
      isAnalyzing: false,
      analysisError: null,
      isExecuting: false,
      executedOps: [],
      failedOp: null,
      currentOpIndex: -1,
      executionProgress: null,
      analysisProgress: null,
      userInstruction: '',
      awaitingInstruction: false,
      executionErrors: [],
      _analysisCleanup: null,
    });
  },

  setPlan: (plan) => set({ currentPlan: plan }),
  setAnalyzing: (analyzing) => set({ isAnalyzing: analyzing }),
  setAnalysisError: (error) => set({ analysisError: error }),

  // Execution actions - delegate to execution-store
  setExecuting: (executing) => {
    useExecutionStore.getState().setExecuting(executing);
    set({ isExecuting: executing });
  },
  markOpExecuted: (opId) => {
    useExecutionStore.getState().markOpExecuted(opId);
    set({ executedOps: useExecutionStore.getState().executedOps });
  },
  markOpFailed: (opId) => {
    useExecutionStore.getState().markOpFailed(opId);
    const execState = useExecutionStore.getState();
    set({ failedOp: execState.failedOp, isExecuting: execState.isExecuting });
  },
  setCurrentOpIndex: (index) => {
    useExecutionStore.getState().setCurrentOpIndex(index);
    set({ currentOpIndex: index });
  },
  resetExecution: () => {
    useExecutionStore.getState().resetExecution();
    set({
      isExecuting: false,
      executedOps: [],
      failedOp: null,
      currentOpIndex: -1,
    });
  },

  // Recovery actions - delegate to recovery-store
  // During migration, sync state for backward compatibility
  checkForInterruptedJob: async () => {
    await useRecoveryStore.getState().checkForInterruptedJob();
    // Sync state for backward compatibility
    const recoveryState = useRecoveryStore.getState();
    set({
      hasInterruptedJob: recoveryState.hasInterruptedJob,
      interruptedJob: recoveryState.interruptedJob,
    });
  },

  dismissInterruptedJob: async () => {
    await useRecoveryStore.getState().dismissInterruptedJob();
    set({
      hasInterruptedJob: false,
      interruptedJob: null,
    });
  },

  resumeInterruptedJob: async () => {
    const { interruptedJob } = get();
    if (!interruptedJob) return;

    set({ isExecuting: true });

    await useRecoveryStore.getState().resumeInterruptedJob((phase) => {
      if (phase === 'committing') {
        set({ phase: 'committing' });
      } else if (phase === 'complete') {
        set({ phase: 'complete', isExecuting: false, interruptedJob: null });
      } else if (phase === 'failed') {
        set({ phase: 'failed', isExecuting: false });
      }
    });

    // Sync thinking state
    const thinkingState = useThinkingStore.getState();
    set({
      thoughts: thinkingState.thoughts,
      currentPhase: thinkingState.currentPhase,
      latestEvent: thinkingState.latestEvent,
    });
  },

  rollbackInterruptedJob: async () => {
    const { interruptedJob } = get();
    if (!interruptedJob) return;

    set({ rollbackProgress: { completed: 0, total: interruptedJob.completedOps } });

    await useRecoveryStore.getState().rollbackInterruptedJob((phase) => {
      if (phase === 'rolling_back') {
        set({ phase: 'rolling_back' });
      } else if (phase === 'idle') {
        set({ phase: 'idle', rollbackProgress: null, interruptedJob: null });
      } else if (phase === 'failed') {
        set({ phase: 'failed', rollbackProgress: null });
      }
    });

    // Sync thinking state
    const thinkingState = useThinkingStore.getState();
    set({
      thoughts: thinkingState.thoughts,
      currentPhase: thinkingState.currentPhase,
      latestEvent: thinkingState.latestEvent,
    });
  },

  // V6: Set user instruction
  setUserInstruction: (instruction: string) => {
    set({ userInstruction: instruction });
  },

  // V6: Submit instruction and start plan generation
  submitInstruction: async () => {
    const { targetFolder, userInstruction } = get();

    if (!targetFolder) return;
    if (!userInstruction.trim()) {
      get().addThought('error', 'Please provide organization instructions', 'Instructions are required');
      return;
    }

    set({
      awaitingInstruction: false,
      isAnalyzing: true,
      phase: 'indexing',
    });

    const folderName = targetFolder.split('/').pop() || 'folder';
    get().addThought('scanning', `Analyzing ${folderName}...`, 'Using your instructions to design organization');

    try {
      // Clean up any existing listeners first (using state-stored ref)
      const existingCleanup = get()._analysisCleanup;
      existingCleanup?.();

      // Set up event listeners for streaming from Rust
      let unlistenThought: UnlistenFn | null = null;
      let unlistenProgress: UnlistenFn | null = null;
      let listenersActive = true;

      // Cleanup function stored in state for proper lifecycle management
      const cleanupListeners = () => {
        if (!listenersActive) return;
        listenersActive = false;
        set({ _analysisCleanup: null });
        unlistenThought?.();
        unlistenProgress?.();
      };

      try {
        // Listen for ai-thought events (phase transitions)
        unlistenThought = await listen<{ type: string; content: string; detail?: string; expandableDetails?: ThoughtDetail[] }>('ai-thought', (event) => {
          const { type, content, detail, expandableDetails } = event.payload;
          get().addThought(type as ThoughtType, content, detail, expandableDetails);
        });

        // Listen for analysis-progress events (progress bar updates)
        unlistenProgress = await listen<AnalysisProgressState>('analysis-progress', (event) => {
          get().setAnalysisProgress(event.payload);
        });

        // Store cleanup in state (not module-level) for proper React lifecycle
        set({ _analysisCleanup: cleanupListeners });
      } catch (e) {
        // Event listener failed - log warning but continue
        // Progress updates may not display but analysis can still proceed
        console.warn('[Organize] Event listener setup failed, progress updates may not display:', e);
      }

      // V6 Hybrid Pipeline: GPT-5-nano exploration + Claude planning
      // OpenAI key is expected from OPENAI_API_KEY environment variable
      // Get userId for billing checks
      const userId = useSubscriptionStore.getState().userId;
      get().addThought('analyzing', 'Using V6 Hybrid pipeline', 'GPT-5-nano explore → Claude plan');
      const rawPlan = await invoke<OrganizePlan>('generate_organize_plan_hybrid', {
        userId,
        folderPath: targetFolder,
        userRequest: userInstruction,
      });

      // Clean up listeners (analysis complete)
      cleanupListeners();

      // Clear analysis progress since we're done analyzing
      get().setAnalysisProgress(null);

      // Defensive validation of plan structure
      if (!rawPlan) {
        throw new Error('No plan returned from AI agent');
      }
      if (!rawPlan.operations) {
        throw new Error('Plan missing operations array');
      }
      if (!Array.isArray(rawPlan.operations)) {
        throw new Error('Operations is not an array');
      }

      // Add risk levels to operations (backend doesn't include them)
      const plan = addRiskLevels(rawPlan);

      // Handle "already organized" case (0 operations)
      if (plan.operations.length === 0) {
        // Check if simplification might help
        if (plan.simplificationRecommended === true) {
          get().addThought('planning', 'No content changes needed', 'Would you like to simplify the folder structure?');
          set({
            currentPlan: plan,
            isAnalyzing: false,
            awaitingSimplificationChoice: true,
            phase: 'idle',
          });
          return;
        }

        // Truly organized
        get().addThought('complete', 'Folder is already well organized!', plan.description || 'No changes needed');
        set({
          currentPlan: plan,
          isAnalyzing: false,
          isExecuting: false,
          phase: 'complete',
        });

        // Mark job as complete since nothing to do
        const jobId = get().currentJobId;
        if (jobId && !jobId.startsWith('local-')) {
          invoke('complete_organize_job', { jobId }).catch(console.error);
          setTimeout(() => invoke('clear_organize_job').catch(console.error), 1000);
        }
        return;
      }

      get().addThought('planning', `Plan ready: ${plan.operations.length} operations`, plan.description);

      // Persist the plan to job state
      const currentJobId = get().currentJobId;
      if (currentJobId && !currentJobId.startsWith('local-')) {
        try {
          await invoke('set_job_plan', {
            jobId: currentJobId,
            planId: plan.planId,
            description: plan.description,
            operations: plan.operations,
            targetFolder: plan.targetFolder,
          });
        } catch (e) {
          console.error('[Organize] Failed to persist plan:', e);
        }
      }

      // V6: Set phase to simulation for user approval
      set({
        currentPlan: plan,
        isAnalyzing: false,
        phase: 'simulation',
      });

    } catch (error) {
      // Clean up listeners on error
      const cleanup = get()._analysisCleanup;
      cleanup?.();

      // Reset VFS state to prevent stale ghost previews
      useVfsStore.getState().reset();

      get().addThought('error', 'Organization failed', String(error));

      // Persist error
      const jobId = get().currentJobId;
      if (jobId && !jobId.startsWith('local-')) {
        invoke('fail_organize_job', { jobId, error: String(error) }).catch(console.error);
      }

      set({
        isAnalyzing: false,
        analysisError: String(error),
        phase: 'failed',
        _analysisCleanup: null,
      });
    }
  },

  // New phase state machine actions
  transitionTo: (phase: OrganizePhase) => {
    set({ phase });
  },

  // Sequential execution (kept for fallback/compatibility)
  // V5: Uses progress tracking instead of per-operation thoughts
  acceptPlan: async () => {
    // Prevent concurrent execution from double-clicks
    if (executionMutex) {
      console.warn('[Organize] acceptPlan blocked by mutex - execution already in progress');
      return;
    }
    executionMutex = true;

    const { currentPlan, phase } = get();
    if (!currentPlan || (phase !== 'simulation' && phase !== 'review')) {
      executionMutex = false;
      return;
    }

    const totalOps = currentPlan.operations.length;

    // Transition to committing phase
    set({ phase: 'committing', isExecuting: true });

    // Initialize operation statuses
    const operationStatuses = new Map<string, OperationStatus>();
    for (const op of currentPlan.operations) {
      operationStatuses.set(op.opId, 'pending');
    }
    set({ operationStatuses });

    // V5: Initialize progress state
    get().setExecutionProgress({
      completed: 0,
      total: totalOps,
      phase: 'executing',
    });

    // V5: Single thought for execution start
    get().addThought('executing', 'Organizing files...',
      `${totalOps} operations queued`);

    // Execute operations sequentially
    for (let i = 0; i < currentPlan.operations.length; i++) {
      const op = currentPlan.operations[i];
      get().setCurrentOpIndex(i);
      get().setOperationStatus(op.opId, 'executing');

      // V5: Update progress instead of adding per-operation thoughts
      get().setExecutionProgress({
        completed: i,
        total: totalOps,
        phase: 'executing',
      });

      try {
        await executeOperation(op);
        get().markOpExecuted(op.opId);
        get().setOperationStatus(op.opId, 'completed');

        // Add to WAL
        set((state) => ({
          wal: [...state.wal, {
            operationId: op.opId,
            type: op.type,
            source: op.source,
            destination: op.destination,
            path: op.path,
            newName: op.newName,
            timestamp: Date.now(),
            status: 'completed' as OperationStatus,
          }],
        }));

        // Persist progress
        const jobId = get().currentJobId;
        if (jobId && !jobId.startsWith('local-')) {
          invoke('complete_job_operation', { jobId, opId: op.opId, currentIndex: i }).catch(console.error);
        }

        await new Promise(resolve => setTimeout(resolve, 50));
      } catch (error) {
        const opName = getOperationDescription(op);
        get().addThought('error', `Failed: ${opName}`, String(error));
        get().markOpFailed(op.opId);
        get().setOperationStatus(op.opId, 'failed');
        get().setExecutionProgress({
          completed: i,
          total: totalOps,
          phase: 'failed',
        });
        set({ phase: 'failed', isExecuting: false });
        executionMutex = false;
        return;
      }
    }

    // Complete
    get().setExecutionProgress({
      completed: totalOps,
      total: totalOps,
      phase: 'complete',
    });
    get().addThought('complete', 'Organization complete!',
      `${totalOps} files organized`);
    set({
      phase: 'complete',
      isExecuting: false,
      currentOpIndex: -1,
    });
    executionMutex = false;
  },

  // Execute plan using parallel DAG-based execution
  // V5: Uses event-based progress tracking instead of per-operation thoughts
  acceptPlanParallel: async () => {
    // True mutex guard - prevents race conditions from rapid double-clicks
    // Zustand's set() is not truly atomic, so we use a module-level lock
    if (executionMutex) {
      console.warn('[Organize] Execution mutex held, ignoring duplicate request');
      return;
    }
    executionMutex = true;

    try {
      const { currentPlan, phase, isExecuting } = get();

      // Guard against invalid states
      if (isExecuting) {
        console.warn('[Organize] Execution already in progress');
        return;
      }
      if (!currentPlan || (phase !== 'simulation' && phase !== 'review')) {
        return;
      }

      // Set isExecuting in store
      set({ isExecuting: true });

      // VFS-WAL sync validation: ensure the plan being executed matches what was previewed
      // Check both plan ID and plan hash for stronger validation
      const vfsState = useVfsStore.getState();
      const simulatedPlanId = vfsState.getSimulatedPlanId();
      const simulatedPlanHash = vfsState.getPlanHash();

      // Validate plan ID matches
      if (simulatedPlanId && simulatedPlanId !== currentPlan.planId) {
        console.error('[Organize] VFS-WAL sync error: plan ID mismatch');
        console.error(`  Simulated planId: ${simulatedPlanId}`);
        console.error(`  Current planId: ${currentPlan.planId}`);
        get().addThought('error', 'Plan out of sync',
          'The plan was modified after preview. Please re-simulate before executing.');
        set({ phase: 'failed', isExecuting: false, analysisError: 'Plan was modified after preview - please re-simulate' });
        return; // Mutex released in finally
      }

      // Validate plan hash if available (provides content-level validation)
      // The plan hash is computed by the backend during VFS validation and ensures
      // the exact operations match what was simulated
      if (simulatedPlanHash) {
        // Compute a simple operations hash for comparison
        const opsString = JSON.stringify(currentPlan.operations.map(op => ({
          opId: op.opId,
          type: op.type,
          source: op.source,
          destination: op.destination,
          path: op.path,
          newName: op.newName,
        })));
        const currentOpsHash = btoa(opsString).slice(0, 32); // Simple hash for comparison

        console.debug('[Organize] VFS-WAL sync: validating plan hash');
        console.debug(`  Simulated hash: ${simulatedPlanHash.slice(0, 16)}...`);
        console.debug(`  Current ops hash: ${currentOpsHash}...`);
        // Note: The hashes may not match exactly because backend uses a different algorithm
        // This is a defense-in-depth check - the plan ID check above is the primary validation
      }

      const totalOps = currentPlan.operations.length;

      // Transition to committing phase (isExecuting already set above)
      set({ phase: 'committing' });

      // Initialize operation statuses
      const operationStatuses = new Map<string, OperationStatus>();
      for (const op of currentPlan.operations) {
        operationStatuses.set(op.opId, 'pending');
      }
      set({ operationStatuses });

      // V5: Initialize progress state (replaces per-operation thoughts)
      get().setExecutionProgress({
        completed: 0,
        total: totalOps,
        phase: 'executing',
      });

      // V5: Single thought for execution start (no per-operation thoughts)
      get().addThought('executing', 'Organizing files...',
        `${totalOps} operations queued for parallel execution`);

      // Set up event listener for progress updates from backend
      let unlisten: UnlistenFn | null = null;
      let unlistenOpComplete: UnlistenFn | null = null;

      // V7: Debounced directory refresh for hot reload
      // Collect affected directories and batch refresh every 150ms
      const pendingDirs = new Set<string>();
      let debounceTimer: ReturnType<typeof setTimeout> | null = null;

      const flushPendingRefresh = () => {
        if (pendingDirs.size > 0) {
          console.log('[organize-store] Hot reload: refreshing', pendingDirs.size, 'directories');
          for (const dir of pendingDirs) {
            queryClient.invalidateQueries({
              queryKey: ['directory', dir],
              exact: false, // Match any showHidden value
            });
          }
          pendingDirs.clear();
        }
      };

      try {
        unlisten = await listen<{ completed: number; total: number }>('execution-progress', (event) => {
          get().setExecutionProgress({
            completed: event.payload.completed,
            total: event.payload.total,
            phase: 'executing',
          });
        });

        // V7: Listen for per-operation completion events for hot reload
        unlistenOpComplete = await listen<{ affectedDirs: string[] }>('execution-op-complete', (event) => {
          const dirs = event.payload.affectedDirs;
          if (dirs && dirs.length > 0) {
            // Add to pending set
            for (const dir of dirs) {
              pendingDirs.add(dir);
            }

            // Debounce: reset timer and flush after 150ms of no new events
            if (debounceTimer) clearTimeout(debounceTimer);
            debounceTimer = setTimeout(flushPendingRefresh, 150);
          }
        });
      } catch (e) {
        // Event listener failed - log warning but continue
        // Real-time progress and hot reload may not work but execution can still proceed
        console.warn('[Organize] Execution event listener setup failed:', e);
      }

      try {
        // Convert plan to backend format (strip frontend-only fields like riskLevel)
        // Note: backend expects 'type' not 'opType' per serde rename
        const backendPlan = {
          planId: currentPlan.planId,
          description: currentPlan.description,
          targetFolder: currentPlan.targetFolder,
          operations: currentPlan.operations.map(op => ({
            opId: op.opId,
            type: op.type,  // Backend expects 'type' (serde rename)
            source: op.source,
            destination: op.destination,
            path: op.path,
            newName: op.newName,
          })),
        };

        // Execute using parallel DAG executor with auto-rename conflict policy
        // Pass originalFolder for post-execution cleanup of empty directories
        // Pass userInstruction for history tracking (multi-level undo)
        const result = await invoke<ExecutionResult>('execute_plan_parallel', {
          plan: backendPlan,
          conflictPolicy: 'auto_rename', // Auto-rename duplicates like file_1.pdf, file_2.pdf
          originalFolder: get().targetFolder, // Original folder for empty directory cleanup
          userInstruction: get().userInstruction || undefined, // For history tracking
        });

        // Clean up listeners and flush any pending refreshes
        if (unlisten) unlisten();
        if (unlistenOpComplete) unlistenOpComplete();
        if (debounceTimer) clearTimeout(debounceTimer);
        flushPendingRefresh(); // Final flush for any remaining directories

        if (result.success) {
          // Mark all operations as completed
          for (const op of currentPlan.operations) {
            get().setOperationStatus(op.opId, 'completed');
            get().markOpExecuted(op.opId);
          }

          // V5: Update progress to complete state
          const processedCount = result.completedCount + result.skippedCount + result.renamedCount;
          get().setExecutionProgress({
            completed: processedCount,
            total: totalOps,
            phase: 'complete',
          });

          // V6: Enhanced completion message with rename/skip info
          let completionDetail = `${result.completedCount} files organized`;
          if (result.renamedCount > 0) {
            completionDetail += `, ${result.renamedCount} renamed to avoid conflicts`;
          }
          if (result.skippedCount > 0) {
            completionDetail += `, ${result.skippedCount} skipped`;
          }
          get().addThought('complete', 'Organization complete!', completionDetail);

          // Get the target folder before clearing state
          const completedFolder = currentPlan.targetFolder;

          // Mark completion in session store
          useSessionStore.getState().markComplete(completedFolder);
          const sessionState = useSessionStore.getState();

          set({
            phase: 'complete',
            isExecuting: false,
            currentOpIndex: -1,
            // Sync completion tracking from session store
            lastCompletedAt: sessionState.lastCompletedAt,
            completedTargetFolder: sessionState.completedTargetFolder,
          });

          // Mark job as complete
          const jobId = get().currentJobId;
          if (jobId && !jobId.startsWith('local-')) {
            invoke('complete_organize_job', { jobId }).catch(console.error);
            setTimeout(() => invoke('clear_organize_job').catch(console.error), 1000);
          }

          // Invalidate VFS state after successful execution to prevent stale data
          invoke('vfs_clear').catch(console.error);
          useVfsStore.getState().reset();

          // Emit event for UsageSync to record in Convex
          emit('usage:record-organize', {
            folderPath: currentPlan.targetFolder,
            folderName: currentPlan.targetFolder.split('/').pop() || 'folder',
            operationCount: currentPlan.operations.length,
            operations: currentPlan.operations.map(op => ({
              type: op.type,
              sourcePath: op.source || op.path || '',
              destPath: op.destination,
            })),
            summary: currentPlan.description,
          });
        } else {
          // Partial failure - V6: include renamed/skipped in progress
          const processedCount = result.completedCount + result.skippedCount + result.renamedCount;
          get().setExecutionProgress({
            completed: processedCount,
            total: totalOps,
            phase: 'failed',
          });

          // V6: Enhanced error message with all outcomes
          let errorDetail = `${result.completedCount} succeeded, ${result.failedCount} failed`;
          if (result.renamedCount > 0) {
            errorDetail += `, ${result.renamedCount} renamed`;
          }
          if (result.skippedCount > 0) {
            errorDetail += `, ${result.skippedCount} skipped`;
          }
          get().addThought('error', 'Some operations failed', errorDetail);

          // V7: Store all execution errors for detailed error dialog
          const executionErrors: ExecutionError[] = result.errors.map((msg) => ({
            message: msg,
          }));

          set({
            phase: 'failed',
            isExecuting: false,
            executionErrors,
          });

          // Persist failure (keep up to 10 errors for logging)
          const jobId = get().currentJobId;
          if (jobId && !jobId.startsWith('local-')) {
            invoke('fail_organize_job', {
              jobId,
              error: `${result.failedCount} operations failed: ${result.errors.slice(0, 10).join('; ')}`
            }).catch(console.error);
          }
        }
      } catch (error) {
        // Clean up listeners and flush any pending refreshes
        if (unlisten) unlisten();
        if (unlistenOpComplete) unlistenOpComplete();
        if (debounceTimer) clearTimeout(debounceTimer);
        flushPendingRefresh(); // Final flush for any remaining directories

        get().setExecutionProgress({
          completed: 0,
          total: totalOps,
          phase: 'failed',
        });

        get().addThought('error', 'Execution failed', String(error));
        set({ phase: 'failed', isExecuting: false });

        // Persist failure
        const jobId = get().currentJobId;
        if (jobId && !jobId.startsWith('local-')) {
          invoke('fail_organize_job', { jobId, error: String(error) }).catch(console.error);
        }
      }
    } finally {
      // Always release the mutex
      executionMutex = false;
    }
  },

  rejectPlan: () => {
    // Reset to idle state, discarding the plan
    set({
      phase: 'idle',
      currentPlan: null,
      isOpen: false,
      operationStatuses: new Map(),
      wal: [],
    });
  },

  startRollback: async () => {
    const { wal, executedOps } = get();
    if (executedOps.length === 0) {
      return;
    }

    set({
      phase: 'rolling_back',
      rollbackProgress: { completed: 0, total: executedOps.length },
    });

    // Track rollback results for detailed reporting
    let successCount = 0;
    let failedCount = 0;
    const failedOperations: { type: string; error: string }[] = [];

    // Rollback in reverse order
    const reversedWal = [...wal].reverse().filter(entry => entry.status === 'completed');

    for (let i = 0; i < reversedWal.length; i++) {
      const entry = reversedWal[i];
      get().addThought('executing', `Undoing: ${entry.type}`, `Rollback ${i + 1} of ${reversedWal.length}`);

      try {
        // Reverse the operation
        switch (entry.type) {
          case 'move':
            if (entry.source && entry.destination) {
              await invoke('move_file', { source: entry.destination, destination: entry.source });
            }
            break;
          case 'rename':
            if (entry.path && entry.newName) {
              const parentPath = entry.path.split('/').slice(0, -1).join('/');
              const oldPath = `${parentPath}/${entry.newName}`;
              await invoke('rename_file', { oldPath, newPath: entry.path });
            }
            break;
          case 'create_folder':
            if (entry.path) {
              try {
                await invoke('delete_to_trash', { path: entry.path });
              } catch (error) {
                // Handle iCloud errors during rollback
                const errorStr = typeof error === 'object' && error !== null ? JSON.stringify(error) : String(error);
                if (errorStr.includes('I_CLOUD_DOWNLOAD_REQUIRED') || errorStr.includes('-8013') || errorStr.includes('needs to be downloaded')) {
                  await invoke('quarantine_item', { path: entry.path });
                } else {
                  throw error;
                }
              }
            }
            break;
          // copy and trash are harder to reverse safely
        }

        successCount++;
        set((state) => ({
          rollbackProgress: state.rollbackProgress
            ? { ...state.rollbackProgress, completed: i + 1 }
            : null,
        }));
      } catch (error) {
        failedCount++;
        const errorStr = String(error);
        failedOperations.push({ type: entry.type, error: errorStr });
        get().addThought('error', `Rollback failed: ${entry.type}`, errorStr);
        // Continue with next operation
      }
    }

    // Report detailed rollback results
    if (failedCount === 0) {
      get().addThought('complete', 'Rollback complete', `Reversed all ${successCount} operations successfully`);
      set({
        phase: 'idle',
        rollbackProgress: null,
        executedOps: [],
        wal: [],
      });
    } else {
      // Partial failure - report detailed results
      const failedSummary = failedOperations
        .slice(0, 5)
        .map(f => `${f.type}: ${f.error.slice(0, 50)}`)
        .join('; ');
      const moreText = failedOperations.length > 5 ? ` (+${failedOperations.length - 5} more)` : '';

      get().addThought(
        'error',
        `Rollback partially complete`,
        `${successCount}/${reversedWal.length} reversed, ${failedCount} failed: ${failedSummary}${moreText}`
      );

      set({
        phase: 'failed',
        rollbackProgress: null,
        // Keep executedOps and wal so user knows what wasn't rolled back
        executionErrors: failedOperations.map(f => ({
          message: f.error,
          operationType: f.type,
        })),
      });
    }
  },

  setOperationStatus: (opId: string, status: OperationStatus) => {
    useExecutionStore.getState().setOperationStatus(opId, status);
    set({ operationStatuses: new Map(useExecutionStore.getState().operationStatuses) });
  },

  // V5: Execution progress updates - delegate to execution-store
  setExecutionProgress: (progress: ExecutionProgressState | null) => {
    useExecutionStore.getState().setExecutionProgress(progress);
    set({ executionProgress: progress });
  },

  // Analysis progress updates for progress bar
  setAnalysisProgress: (progress: AnalysisProgressState | null) => {
    set({ analysisProgress: progress });
  },

  // Plan edit modal actions - delegate to plan-store
  openPlanEditModal: () => {
    // Sync currentPlan to plan-store before calling openPlanEditModal
    // so plan-store.openPlanEditModal() can read the current plan
    const { currentPlan } = get();
    if (!currentPlan) return;
    usePlanStore.getState().setPlan(currentPlan);
    usePlanStore.getState().openPlanEditModal();
    const planState = usePlanStore.getState();
    set({
      isPlanEditModalOpen: planState.isPlanEditModalOpen,
      editableOperations: planState.editableOperations,
    });
  },

  closePlanEditModal: () => {
    usePlanStore.getState().closePlanEditModal();
    set({
      isPlanEditModalOpen: false,
      editableOperations: [],
    });
  },

  applyPlanEdits: () => {
    const { currentPlan, editableOperations } = get();
    if (!currentPlan) return;

    // Filter to only enabled operations and strip editing metadata
    const filteredOps: OrganizeOperation[] = editableOperations
      .filter((op) => op.enabled)
      .map(({ enabled, isModified, originalDestination, originalNewName, ...op }) => op);

    // Generate new planId since plan was modified
    // This ensures VFS-WAL sync validation will catch any edits
    const newPlanId = `plan-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;

    // Update the plan with filtered operations and new planId
    const updatedPlan: OrganizePlan = {
      ...currentPlan,
      planId: newPlanId,
      operations: filteredOps,
    };

    // IMPORTANT: Update VFS BEFORE main store to prevent stale state reads
    // Components subscribing to phase changes might read VFS state immediately
    useVfsStore.getState().applyPlan(updatedPlan);
    usePlanStore.getState().setPlan(updatedPlan);

    // Set phase back to simulation to force re-preview
    // This is done AFTER VFS update so components see consistent state
    set({
      currentPlan: updatedPlan,
      isPlanEditModalOpen: false,
      editableOperations: [],
      phase: 'simulation', // Force re-simulation for safety
    });

    get().addThought('planning', 'Plan updated',
      `${filteredOps.length} operations ready for preview`);
  },

  toggleOperation: (opId: string) => {
    usePlanStore.getState().toggleOperation(opId);
    set({ editableOperations: usePlanStore.getState().editableOperations });
  },

  toggleOperationGroup: (targetFolder: string) => {
    usePlanStore.getState().toggleOperationGroup(targetFolder);
    set({ editableOperations: usePlanStore.getState().editableOperations });
  },

  updateOperationDestination: (opId: string, newDestination: string) => {
    usePlanStore.getState().updateOperationDestination(opId, newDestination);
    set({ editableOperations: usePlanStore.getState().editableOperations });
  },

  updateOperationNewName: (opId: string, newName: string) => {
    usePlanStore.getState().updateOperationNewName(opId, newName);
    set({ editableOperations: usePlanStore.getState().editableOperations });
  },

  // Simplification prompt actions
  acceptSimplification: async () => {
    const { targetFolder, isAnalyzing, awaitingSimplificationChoice, _analysisCleanup } = get();

    // Guard against double-invocation (e.g., rapid double-click)
    if (!targetFolder || isAnalyzing || !awaitingSimplificationChoice) return;

    // Clean up any existing listeners first
    _analysisCleanup?.();

    set({ awaitingSimplificationChoice: false, isAnalyzing: true, phase: 'planning' });
    get().addThought('analyzing', 'Analyzing folder structure for simplification...');

    // Set up listener cleanup with state-based pattern
    let unlistenThought: UnlistenFn | null = null;
    let listenersActive = true;

    const cleanupListeners = () => {
      if (!listenersActive) return;
      listenersActive = false;
      set({ _analysisCleanup: null });
      unlistenThought?.();
    };

    try {
      // Set up event listeners for AI thoughts
      unlistenThought = await listen<{
        type: string;
        content: string;
        expandableDetails?: Array<{ label: string; value: string }>;
      }>('ai-thought', (event) => {
        const { type, content, expandableDetails } = event.payload;
        get().addThought(
          type as ThoughtType,
          content,
          undefined,
          expandableDetails
        );
      });

      // Store cleanup in state for proper lifecycle
      set({ _analysisCleanup: cleanupListeners });

      // Get userId for billing checks
      const userId = useSubscriptionStore.getState().userId;
      const rawPlan = await invoke<OrganizePlan>('generate_simplification_plan', {
        userId,
        folderPath: targetFolder,
      });

      // Clean up listeners
      cleanupListeners();

      if (!rawPlan?.operations) {
        throw new Error('No plan returned from simplification analysis');
      }

      const plan = addRiskLevels(rawPlan);

      if (plan.operations.length === 0) {
        get().addThought('complete', 'Folder structure is already simple!');
        set({
          currentPlan: plan,
          isAnalyzing: false,
          phase: 'complete',
        });
        return;
      }

      get().addThought('planning', `Simplification plan ready: ${plan.operations.length} operations`, plan.description);

      set({
        currentPlan: plan,
        isAnalyzing: false,
        phase: 'simulation',
      });
    } catch (error) {
      cleanupListeners();
      get().addThought('error', 'Simplification failed', String(error));
      set({
        isAnalyzing: false,
        analysisError: String(error),
        awaitingSimplificationChoice: false, // Reset to prevent UI stuck state
        phase: 'failed',
      });
    }
  },

  rejectSimplification: () => {
    get().addThought('complete', 'Folder is already well organized!');
    set({
      awaitingSimplificationChoice: false,
      phase: 'complete',
    });

    // Mark job as complete
    const jobId = get().currentJobId;
    if (jobId && !jobId.startsWith('local-')) {
      invoke('complete_organize_job', { jobId }).catch(console.error);
      setTimeout(() => invoke('clear_organize_job').catch(console.error), 1000);
    }
  },
}));

// Helper to describe an operation
function getOperationDescription(op: OrganizeOperation): string {
  switch (op.type) {
    case 'create_folder':
      return `Creating folder: ${op.path?.split('/').pop()}`;
    case 'move':
      return `Moving: ${op.source?.split('/').pop()}`;
    case 'rename':
      return `Renaming: ${op.path?.split('/').pop()} → ${op.newName}`;
    case 'trash':
      return `Deleting: ${op.path?.split('/').pop()}`;
    case 'copy':
      return `Copying: ${op.source?.split('/').pop()}`;
    default:
      return op.type;
  }
}

/**
 * React hook that computes the current phase from organize-store state.
 * This hook subscribes to the main organize-store and computes the phase
 * from actual state values.
 *
 * Uses useShallow for proper memoization - only triggers re-renders when
 * the selected values actually change, not on every store update.
 *
 * NOTE: Defined here in organize-store.ts to avoid circular imports.
 * Re-exported from organize/index.ts for convenience.
 */
export function useComputedPhase(): OrganizePhase {
  const state = useOrganizeStore(
    useShallow((s) => ({
      currentPlan: s.currentPlan,
      awaitingInstruction: s.awaitingInstruction,
      isAnalyzing: s.isAnalyzing,
      analysisProgress: s.analysisProgress,
      analysisError: s.analysisError,
      isExecuting: s.isExecuting,
      executionProgress: s.executionProgress,
      rollbackProgress: s.rollbackProgress,
    }))
  );

  return computePhase(
    {
      currentPlan: state.currentPlan,
      awaitingInstruction: state.awaitingInstruction,
      isAnalyzing: state.isAnalyzing,
      analysisProgress: state.analysisProgress,
      analysisError: state.analysisError,
    },
    {
      isExecuting: state.isExecuting,
      executionProgress: state.executionProgress,
    },
    {
      rollbackProgress: state.rollbackProgress,
    }
  );
}
