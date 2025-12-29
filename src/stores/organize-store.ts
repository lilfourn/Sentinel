import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

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
  | 'planning'
  | 'thinking'
  | 'executing'
  | 'complete'
  | 'error';

export interface AIThought {
  id: string;
  type: ThoughtType;
  content: string;
  timestamp: number;
  detail?: string;
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

  // Plan state
  currentPlan: OrganizePlan | null;
  isAnalyzing: boolean;
  analysisError: string | null;

  // Execution state
  isExecuting: boolean;
  executedOps: string[];
  failedOp: string | null;
  currentOpIndex: number;

  // Recovery state
  hasInterruptedJob: boolean;
  interruptedJob: InterruptedJobInfo | null;
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
  addThought: (type: ThoughtType, content: string, detail?: string) => void;
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

  // Recovery actions
  checkForInterruptedJob: () => Promise<void>;
  dismissInterruptedJob: () => Promise<void>;
  resumeInterruptedJob: () => Promise<void>;
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
      await invoke('delete_to_trash', { path: op.path });
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
  currentPlan: null,
  isAnalyzing: false,
  analysisError: null,
  isExecuting: false,
  executedOps: [],
  failedOp: null,
  currentOpIndex: -1,
  hasInterruptedJob: false,
  interruptedJob: null,

  // Thought actions
  addThought: (type, content, detail) => set((state) => ({
    thoughts: [...state.thoughts, {
      id: `thought-${++thoughtId}`,
      type,
      content,
      detail,
      timestamp: Date.now(),
    }],
    currentPhase: type,
  })),

  setPhase: (phase) => set({ currentPhase: phase }),

  clearThoughts: () => set({ thoughts: [], currentPhase: 'scanning' }),

  // Start automatic organize flow with thinking stream
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

    set({
      isOpen: true,
      targetFolder: folderPath,
      currentJobId: jobId,
      currentPlan: null,
      isAnalyzing: true,
      analysisError: null,
      isExecuting: false,
      executedOps: [],
      failedOp: null,
      currentOpIndex: -1,
    });

    const folderName = folderPath.split('/').pop() || 'folder';

    try {
      // Phase 1: Scanning
      state.addThought('scanning', `Scanning ${folderName}...`, 'Agent is exploring folder structure');

      // Set up event listener for streaming thoughts from Rust
      let unlisten: UnlistenFn | null = null;
      try {
        unlisten = await listen<{ type: string; content: string; detail?: string }>('ai-thought', (event) => {
          const { type, content, detail } = event.payload;
          get().addThought(type as ThoughtType, content, detail);
        });
      } catch {
        // Event listener failed, continue without it
      }

      // Use the new agentic command that explores and generates plan
      state.addThought('analyzing', 'AI agent exploring folder...', 'Using tools to understand file structure');

      const rawPlan = await invoke<OrganizePlan>('generate_organize_plan_agentic', {
        folderPath,
        userRequest: 'Organize this folder by grouping files based on their content type and purpose. Create logical folder structures.',
      });

      // Clean up listener
      if (unlisten) unlisten();

      // Add risk levels to operations (backend doesn't include them)
      const plan = addRiskLevels(rawPlan);

      state.addThought('planning', `Plan ready: ${plan.operations.length} operations`, plan.description);

      set({ currentPlan: plan, isAnalyzing: false });

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

      // Phase 4: Executing
      state.addThought('executing', 'Starting execution...', 'Applying changes to folder');
      set({ isExecuting: true });

      for (let i = 0; i < plan.operations.length; i++) {
        const op = plan.operations[i];
        get().setCurrentOpIndex(i);

        const opName = getOperationDescription(op);
        state.addThought('executing', opName, `Operation ${i + 1} of ${plan.operations.length}`);

        try {
          await executeOperation(op);
          get().markOpExecuted(op.opId);

          // Persist progress
          const jobId = get().currentJobId;
          if (jobId && !jobId.startsWith('local-')) {
            invoke('complete_job_operation', { jobId, opId: op.opId, currentIndex: i }).catch(console.error);
          }

          await new Promise(resolve => setTimeout(resolve, 100));
        } catch (error) {
          state.addThought('error', `Failed: ${opName}`, String(error));
          get().markOpFailed(op.opId);

          // Persist failure
          const jobId = get().currentJobId;
          if (jobId && !jobId.startsWith('local-')) {
            invoke('fail_organize_job', { jobId, error: String(error) }).catch(console.error);
          }

          console.error(`Operation failed: ${op.type}`, error);
          return;
        }
      }

      // Phase 5: Complete
      state.addThought('complete', 'Organization complete!', `Successfully executed ${plan.operations.length} operations`);
      get().setCurrentOpIndex(-1);
      get().setExecuting(false);

      // Mark job as complete and clear
      const finalJobId = get().currentJobId;
      if (finalJobId && !finalJobId.startsWith('local-')) {
        invoke('complete_organize_job', { jobId: finalJobId }).catch(console.error);
        // Clear job file after successful completion
        setTimeout(() => invoke('clear_organize_job').catch(console.error), 1000);
      }

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
      });
    }
  },

  closeOrganizer: () => {
    set({
      isOpen: false,
      targetFolder: null,
      currentJobId: null,
      thoughts: [],
      currentPhase: 'scanning',
      currentPlan: null,
      isAnalyzing: false,
      analysisError: null,
      isExecuting: false,
      executedOps: [],
      failedOp: null,
      currentOpIndex: -1,
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

  // Recovery actions
  checkForInterruptedJob: async () => {
    try {
      const job = await invoke<{
        jobId: string;
        folderName: string;
        targetFolder: string;
        completedOps: string[];
        totalOps: number;
        startedAt: number;
        status: string;
      } | null>('check_interrupted_job');

      if (job && job.status === 'interrupted') {
        set({
          hasInterruptedJob: true,
          interruptedJob: {
            jobId: job.jobId,
            folderName: job.folderName,
            targetFolder: job.targetFolder,
            completedOps: job.completedOps.length,
            totalOps: job.totalOps,
            startedAt: job.startedAt,
          },
        });
      }
    } catch (e) {
      console.error('[Organize] Failed to check for interrupted job:', e);
    }
  },

  dismissInterruptedJob: async () => {
    try {
      await invoke('clear_organize_job');
    } catch (e) {
      console.error('[Organize] Failed to clear job:', e);
    }
    set({
      hasInterruptedJob: false,
      interruptedJob: null,
    });
  },

  resumeInterruptedJob: async () => {
    const { interruptedJob } = get();
    if (!interruptedJob) return;

    // Clear the interrupted state
    set({
      hasInterruptedJob: false,
      interruptedJob: null,
    });

    // Start a new organize job for the same folder
    // This will create a fresh job since the old one might have partial state
    await get().startOrganize(interruptedJob.targetFolder);
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
