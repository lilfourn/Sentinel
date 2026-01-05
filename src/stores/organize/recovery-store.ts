import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { WalEntry, WalRecoveryInfo, WalRecoveryResult } from '../../types/ghost';
import { useThinkingStore } from './thinking-store';

// Info about an interrupted job for recovery UI
export interface InterruptedJobInfo {
  jobId: string;
  folderName: string;
  targetFolder: string;
  completedOps: number;
  totalOps: number;
  startedAt: number;
}

interface RecoveryState {
  hasInterruptedJob: boolean;
  interruptedJob: InterruptedJobInfo | null;
  wal: WalEntry[];
  rollbackProgress: { completed: number; total: number } | null;
}

interface RecoveryActions {
  checkForInterruptedJob: () => Promise<void>;
  dismissInterruptedJob: () => Promise<void>;
  resumeInterruptedJob: (onPhaseTransition: (phase: string) => void) => Promise<void>;
  rollbackInterruptedJob: (onPhaseTransition: (phase: string) => void) => Promise<void>;
  setRollbackProgress: (progress: { completed: number; total: number } | null) => void;
  appendToWal: (entry: WalEntry) => void;
  clearWal: () => void;
}

export const useRecoveryStore = create<RecoveryState & RecoveryActions>((set, get) => ({
  // Initial state
  hasInterruptedJob: false,
  interruptedJob: null,
  wal: [],
  rollbackProgress: null,

  // Actions
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
      console.error('[Recovery] Failed to check for interrupted job:', e);
    }
  },

  dismissInterruptedJob: async () => {
    const { interruptedJob } = get();
    if (!interruptedJob) return;

    try {
      await invoke('wal_discard_job', { jobId: interruptedJob.jobId });
    } catch (e) {
      console.error('[Recovery] Failed to discard job:', e);
    }
    set({
      hasInterruptedJob: false,
      interruptedJob: null,
    });
  },

  resumeInterruptedJob: async (onPhaseTransition) => {
    const { interruptedJob } = get();
    if (!interruptedJob) return;

    set({ hasInterruptedJob: false });
    onPhaseTransition('committing');

    useThinkingStore.getState().addThought(
      'executing',
      'Resuming interrupted job...',
      `Continuing from operation ${interruptedJob.completedOps + 1}`
    );

    try {
      const result = await invoke<WalRecoveryResult>('wal_resume_job', {
        jobId: interruptedJob.jobId,
      });

      if (result.success) {
        useThinkingStore.getState().addThought(
          'complete',
          'Resume complete!',
          `Completed ${result.completedCount} remaining operations`
        );
        onPhaseTransition('complete');
        set({ interruptedJob: null });
      } else {
        for (const error of result.errors) {
          useThinkingStore.getState().addThought('error', 'Operation failed', error);
        }
        onPhaseTransition('failed');
      }
    } catch (e) {
      useThinkingStore.getState().addThought('error', 'Resume failed', String(e));
      onPhaseTransition('failed');
    }
  },

  rollbackInterruptedJob: async (onPhaseTransition) => {
    const { interruptedJob } = get();
    if (!interruptedJob) return;

    set({
      hasInterruptedJob: false,
      rollbackProgress: { completed: 0, total: interruptedJob.completedOps },
    });
    onPhaseTransition('rolling_back');

    useThinkingStore.getState().addThought(
      'executing',
      'Rolling back changes...',
      `Undoing ${interruptedJob.completedOps} completed operations`
    );

    try {
      const result = await invoke<WalRecoveryResult>('wal_rollback_job', {
        jobId: interruptedJob.jobId,
      });

      if (result.success) {
        useThinkingStore.getState().addThought(
          'complete',
          'Rollback complete!',
          `Undid ${result.completedCount} operations`
        );
        onPhaseTransition('idle');
        set({ rollbackProgress: null, interruptedJob: null });
      } else {
        for (const error of result.errors) {
          useThinkingStore.getState().addThought('error', 'Rollback failed', error);
        }
        onPhaseTransition('failed');
        set({ rollbackProgress: null });
      }
    } catch (e) {
      useThinkingStore.getState().addThought('error', 'Rollback failed', String(e));
      onPhaseTransition('failed');
      set({ rollbackProgress: null });
    }
  },

  setRollbackProgress: (progress) => set({ rollbackProgress: progress }),

  appendToWal: (entry) =>
    set((state) => ({
      wal: [...state.wal, entry],
    })),

  clearWal: () => set({ wal: [] }),
}));
