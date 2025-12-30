import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { NamingConvention, NamingConventionSuggestions } from '../types/naming-convention';
import type { OperationStatus, WalEntry, WalRecoveryInfo, WalRecoveryResult } from '../types/ghost';

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
}

// Thought/step types for the AI thinking stream
export type ThoughtType =
  | 'scanning'
  | 'analyzing'
  | 'naming_conventions'
  | 'planning'
  | 'thinking'
  | 'executing'
  | 'complete'
  | 'error';

// Expandable detail item for rich thought display
export interface ThoughtDetail {
  label: string;
  value: string;
}

export interface AIThought {
  id: string;
  type: ThoughtType;
  content: string;
  timestamp: number;
  detail?: string;
  // Expandable details for richer information display
  expandableDetails?: ThoughtDetail[];
}

/**
 * Phase state machine for the organize workflow.
 * This provides a higher-level view of where we are in the process.
 */
export type OrganizePhase =
  | 'idle'         // Not organizing anything
  | 'indexing'     // Scanning folder structure
  | 'planning'     // AI is generating the plan
  | 'simulation'   // Plan is ready, showing ghost preview
  | 'review'       // User is reviewing the plan (diff view)
  | 'committing'   // Executing operations
  | 'rolling_back' // Undoing completed operations
  | 'complete'     // All done
  | 'failed';      // Error occurred

// Execution progress tracked from backend events
export interface ExecutionProgressState {
  completed: number;
  total: number;
  phase: 'preparing' | 'executing' | 'complete' | 'failed';
}

interface OrganizeState {
  // UI state
  isOpen: boolean;
  targetFolder: string | null;

  // Job persistence
  currentJobId: string | null;

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

  // Recovery state
  hasInterruptedJob: boolean;
  interruptedJob: InterruptedJobInfo | null;

  // V6: User instruction for deep organization
  userInstruction: string;
  awaitingInstruction: boolean;

  // Naming convention selection state (deprecated - kept for compatibility)
  awaitingConventionSelection: boolean;
  suggestedConventions: NamingConvention[] | null;
  selectedConvention: NamingConvention | null;
  conventionSkipped: boolean;

  // Latest event for dynamic status display
  latestEvent: { type: string; detail: string } | null;
}

// Info about an interrupted job for recovery UI
export interface InterruptedJobInfo {
  jobId: string;
  folderName: string;
  targetFolder: string;
  completedOps: number;
  totalOps: number;
  startedAt: number;
}

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

  // Naming convention actions (deprecated - kept for compatibility)
  selectConvention: (convention: NamingConvention) => void;
  skipConventionSelection: () => void;

  // New phase state machine actions
  transitionTo: (phase: OrganizePhase) => void;
  acceptPlan: () => Promise<void>;
  acceptPlanParallel: () => Promise<void>;
  rejectPlan: () => void;
  startRollback: () => Promise<void>;
  setOperationStatus: (opId: string, status: OperationStatus) => void;

  // V5: Execution progress updates
  setExecutionProgress: (progress: ExecutionProgressState | null) => void;
}

let thoughtId = 0;

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
      const parentPath = op.path!.split('/').slice(0, -1).join('/');
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
  awaitingConventionSelection: false,
  suggestedConventions: null,
  selectedConvention: null,
  conventionSkipped: false,
  latestEvent: null,
  executionProgress: null,

  // Thought actions
  addThought: (type, content, detail, expandableDetails) => set((state) => ({
    thoughts: [...state.thoughts, {
      id: `thought-${++thoughtId}`,
      type,
      content,
      detail,
      expandableDetails,
      timestamp: Date.now(),
    }],
    currentPhase: type,
    latestEvent: { type, detail: content },
  })),

  setPhase: (phase) => set({ currentPhase: phase }),

  clearThoughts: () => set({ thoughts: [], currentPhase: 'scanning' }),

  // Start organize flow - V6: Wait for user instruction
  // Opens the panel and waits for user to provide organization instructions
  startOrganize: async (folderPath: string) => {
    const state = get();
    state.clearThoughts();

    // Start persistent job
    let jobId: string;
    try {
      const job = await invoke<{ jobId: string }>('start_organize_job', { targetFolder: folderPath });
      jobId = job.jobId;
    } catch (e) {
      console.error('[Organize] Failed to start job:', e);
      jobId = `local-${Date.now()}`;
    }

    const folderName = folderPath.split('/').pop() || 'folder';

    // V6: Open panel and wait for user instruction
    set({
      isOpen: true,
      targetFolder: folderPath,
      currentJobId: jobId,
      currentPlan: null,
      isAnalyzing: false,
      analysisError: null,
      isExecuting: false,
      executedOps: [],
      failedOp: null,
      currentOpIndex: -1,
      userInstruction: '',
      awaitingInstruction: true,
      awaitingConventionSelection: false,
      suggestedConventions: null,
      selectedConvention: null,
      conventionSkipped: false,
      phase: 'idle',
    });

    state.addThought('scanning', `Ready to organize ${folderName}`, 'Provide instructions to begin');

    // Flow continues when user calls submitInstruction()
  },

  closeOrganizer: () => {
    set({
      isOpen: false,
      targetFolder: null,
      currentJobId: null,
      thoughts: [],
      currentPhase: 'scanning',
      phase: 'idle',
      currentPlan: null,
      isAnalyzing: false,
      analysisError: null,
      isExecuting: false,
      executedOps: [],
      failedOp: null,
      currentOpIndex: -1,
      executionProgress: null,
      userInstruction: '',
      awaitingInstruction: false,
      awaitingConventionSelection: false,
      suggestedConventions: null,
      selectedConvention: null,
      conventionSkipped: false,
    });
  },

  setPlan: (plan) => set({ currentPlan: plan }),
  setAnalyzing: (analyzing) => set({ isAnalyzing: analyzing }),
  setAnalysisError: (error) => set({ analysisError: error }),

  setExecuting: (executing) => set({ isExecuting: executing }),
  markOpExecuted: (opId) => set((state) => ({
    executedOps: [...state.executedOps, opId],
  })),
  markOpFailed: (opId) => set({ failedOp: opId, isExecuting: false }),
  setCurrentOpIndex: (index) => set({ currentOpIndex: index }),
  resetExecution: () => set({
    isExecuting: false,
    executedOps: [],
    failedOp: null,
    currentOpIndex: -1,
  }),

  // Recovery actions (WAL-based)
  checkForInterruptedJob: async () => {
    try {
      const recoveryInfo = await invoke<WalRecoveryInfo | null>('wal_check_recovery');

      if (recoveryInfo) {
        set({
          hasInterruptedJob: true,
          interruptedJob: {
            jobId: recoveryInfo.jobId,
            folderName: recoveryInfo.targetFolder.split('/').pop() || 'folder',
            targetFolder: recoveryInfo.targetFolder,
            completedOps: recoveryInfo.completedCount,
            totalOps: recoveryInfo.completedCount + recoveryInfo.pendingCount,
            startedAt: new Date(recoveryInfo.startedAt).getTime(),
          },
        });
      }
    } catch (e) {
      console.error('[Organize] Failed to check for interrupted job:', e);
    }
  },

  dismissInterruptedJob: async () => {
    const { interruptedJob } = get();
    if (!interruptedJob) return;

    try {
      await invoke('wal_discard_job', { jobId: interruptedJob.jobId });
    } catch (e) {
      console.error('[Organize] Failed to discard job:', e);
    }
    set({
      hasInterruptedJob: false,
      interruptedJob: null,
    });
  },

  resumeInterruptedJob: async () => {
    const { interruptedJob } = get();
    if (!interruptedJob) return;

    set({
      hasInterruptedJob: false,
      isExecuting: true,
      phase: 'committing',
    });

    get().addThought('executing', 'Resuming interrupted job...',
      `Continuing from operation ${interruptedJob.completedOps + 1}`);

    try {
      const result = await invoke<WalRecoveryResult>('wal_resume_job', {
        jobId: interruptedJob.jobId
      });

      if (result.success) {
        get().addThought('complete', 'Resume complete!',
          `Completed ${result.completedCount} remaining operations`);
        set({
          phase: 'complete',
          isExecuting: false,
          interruptedJob: null,
        });
      } else {
        for (const error of result.errors) {
          get().addThought('error', 'Operation failed', error);
        }
        set({
          phase: 'failed',
          isExecuting: false,
        });
      }
    } catch (e) {
      get().addThought('error', 'Resume failed', String(e));
      set({
        phase: 'failed',
        isExecuting: false,
      });
    }
  },

  rollbackInterruptedJob: async () => {
    const { interruptedJob } = get();
    if (!interruptedJob) return;

    set({
      hasInterruptedJob: false,
      phase: 'rolling_back',
      rollbackProgress: { completed: 0, total: interruptedJob.completedOps },
    });

    get().addThought('executing', 'Rolling back changes...',
      `Undoing ${interruptedJob.completedOps} completed operations`);

    try {
      const result = await invoke<WalRecoveryResult>('wal_rollback_job', {
        jobId: interruptedJob.jobId
      });

      if (result.success) {
        get().addThought('complete', 'Rollback complete!',
          `Undid ${result.completedCount} operations`);
        set({
          phase: 'idle',
          rollbackProgress: null,
          interruptedJob: null,
        });
      } else {
        for (const error of result.errors) {
          get().addThought('error', 'Rollback failed', error);
        }
        set({
          phase: 'failed',
          rollbackProgress: null,
        });
      }
    } catch (e) {
      get().addThought('error', 'Rollback failed', String(e));
      set({
        phase: 'failed',
        rollbackProgress: null,
      });
    }
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
      // Set up event listener for streaming thoughts from Rust
      let unlisten: UnlistenFn | null = null;
      try {
        unlisten = await listen<{ type: string; content: string; detail?: string; expandableDetails?: ThoughtDetail[] }>('ai-thought', (event) => {
          const { type, content, detail, expandableDetails } = event.payload;
          get().addThought(type as ThoughtType, content, detail, expandableDetails);
        });
      } catch {
        // Event listener failed, continue without it
      }

      // V6: Call the agentic organize with user instruction
      const rawPlan = await invoke<OrganizePlan>('generate_organize_plan_agentic', {
        folderPath: targetFolder,
        userRequest: userInstruction,
      });

      // Clean up listener
      if (unlisten) unlisten();

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
      });
    }
  },

  // Select a naming convention and continue with plan generation (deprecated)
  selectConvention: (convention: NamingConvention) => {
    set({
      selectedConvention: convention,
      conventionSkipped: false,
      awaitingConventionSelection: false,
    });

    // Continue with plan generation
    continueWithConvention(get, set);
  },

  // Skip naming convention selection and continue with folder-only organization
  skipConventionSelection: () => {
    set({
      selectedConvention: null,
      conventionSkipped: true,
      awaitingConventionSelection: false,
    });

    // Continue with plan generation
    continueWithConvention(get, set);
  },

  // New phase state machine actions
  transitionTo: (phase: OrganizePhase) => {
    set({ phase });
  },

  // Sequential execution (kept for fallback/compatibility)
  // V5: Uses progress tracking instead of per-operation thoughts
  acceptPlan: async () => {
    const { currentPlan, phase } = get();
    if (!currentPlan || (phase !== 'simulation' && phase !== 'review')) {
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
  },

  // Execute plan using parallel DAG-based execution
  // V5: Uses event-based progress tracking instead of per-operation thoughts
  acceptPlanParallel: async () => {
    const { currentPlan, phase } = get();
    if (!currentPlan || (phase !== 'simulation' && phase !== 'review')) {
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
    try {
      unlisten = await listen<{ completed: number; total: number }>('execution-progress', (event) => {
        get().setExecutionProgress({
          completed: event.payload.completed,
          total: event.payload.total,
          phase: 'executing',
        });
      });
    } catch {
      // Event listener failed, continue without real-time progress
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
      const result = await invoke<ExecutionResult>('execute_plan_parallel', {
        plan: backendPlan,
        conflictPolicy: 'auto_rename', // Auto-rename duplicates like file_1.pdf, file_2.pdf
      });

      // Clean up listener
      if (unlisten) unlisten();

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

        set({
          phase: 'complete',
          isExecuting: false,
          currentOpIndex: -1,
        });

        // Mark job as complete
        const jobId = get().currentJobId;
        if (jobId && !jobId.startsWith('local-')) {
          invoke('complete_organize_job', { jobId }).catch(console.error);
          setTimeout(() => invoke('clear_organize_job').catch(console.error), 1000);
        }
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

        set({ phase: 'failed', isExecuting: false });

        // Persist failure
        const jobId = get().currentJobId;
        if (jobId && !jobId.startsWith('local-')) {
          invoke('fail_organize_job', {
            jobId,
            error: `${result.failedCount} operations failed: ${result.errors.slice(0, 3).join('; ')}`
          }).catch(console.error);
        }
      }
    } catch (error) {
      // Clean up listener
      if (unlisten) unlisten();

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

        set((state) => ({
          rollbackProgress: state.rollbackProgress
            ? { ...state.rollbackProgress, completed: i + 1 }
            : null,
        }));
      } catch (error) {
        get().addThought('error', `Rollback failed: ${entry.type}`, String(error));
        // Continue with next operation
      }
    }

    get().addThought('complete', 'Rollback complete', `Reversed ${reversedWal.length} operations`);
    set({
      phase: 'idle',
      rollbackProgress: null,
      executedOps: [],
      wal: [],
    });
  },

  setOperationStatus: (opId: string, status: OperationStatus) => {
    set((state) => {
      const newStatuses = new Map(state.operationStatuses);
      newStatuses.set(opId, status);
      return { operationStatuses: newStatuses };
    });
  },

  // V5: Execution progress updates
  setExecutionProgress: (progress: ExecutionProgressState | null) => {
    set({ executionProgress: progress });
  },
}));

// Phase 2: Continue with plan generation after convention selection
async function continueWithConvention(
  get: () => OrganizeState & OrganizeActions,
  set: (state: Partial<OrganizeState>) => void
) {
  const state = get();
  const { targetFolder, selectedConvention, conventionSkipped } = state;

  if (!targetFolder) return;

  set({ isAnalyzing: true });

  state.addThought('planning', 'Generating organization plan...',
    conventionSkipped ? 'Skipping file renaming' : `Using ${selectedConvention?.name} convention`);

  try {
    // Set up event listener for streaming thoughts from Rust
    let unlisten: UnlistenFn | null = null;
    try {
      unlisten = await listen<{ type: string; content: string; detail?: string; expandableDetails?: ThoughtDetail[] }>('ai-thought', (event) => {
        const { type, content, detail, expandableDetails } = event.payload;
        get().addThought(type as ThoughtType, content, detail, expandableDetails);
      });
    } catch {
      // Event listener failed, continue without it
    }

    const rawPlan = await invoke<OrganizePlan>('generate_organize_plan_with_convention', {
      folderPath: targetFolder,
      userRequest: 'Organize this folder by grouping files based on their content type and purpose. Create logical folder structures.',
      convention: conventionSkipped ? null : selectedConvention,
    });

    // Clean up listener
    if (unlisten) unlisten();

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
      state.addThought('complete', 'Folder is already well organized!', plan.description || 'No changes needed');
      set({
        currentPlan: plan,
        isAnalyzing: false,
        isExecuting: false,
      });

      // Mark job as complete since nothing to do
      const jobId = get().currentJobId;
      if (jobId && !jobId.startsWith('local-')) {
        invoke('complete_organize_job', { jobId }).catch(console.error);
        setTimeout(() => invoke('clear_organize_job').catch(console.error), 1000);
      }
      return;
    }

    state.addThought('planning', `Plan ready: ${plan.operations.length} operations`, plan.description);

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

    // V5: Pause at simulation phase for user approval
    // Instead of auto-executing, set phase to 'simulation' so user can review and approve
    set({
      currentPlan: plan,
      isAnalyzing: false,
      phase: 'simulation',
    });

    // User must now click "Apply" in SimulationControls to call acceptPlanParallel()

  } catch (error) {
    state.addThought('error', 'Organization failed', String(error));

    // Persist error
    const jobId = get().currentJobId;
    if (jobId && !jobId.startsWith('local-')) {
      invoke('fail_organize_job', { jobId, error: String(error) }).catch(console.error);
    }

    set({
      isAnalyzing: false,
      analysisError: String(error),
      phase: 'failed',
    });
  }
}

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
