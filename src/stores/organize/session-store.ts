import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

interface SessionState {
  isOpen: boolean;
  targetFolder: string | null;
  currentJobId: string | null;
  lastCompletedAt: number | null;
  completedTargetFolder: string | null;
  /** True if job persistence failed and we're using a local fallback ID */
  isOfflineMode: boolean;
  /** Error message if job persistence failed */
  persistenceError: string | null;
}

interface SessionActions {
  openSession: (folderPath: string) => Promise<string>;
  closeSession: () => void;
  markComplete: (targetFolder: string) => void;
  setJobId: (jobId: string | null) => void;
}

export const useSessionStore = create<SessionState & SessionActions>((set) => ({
  // Initial state
  isOpen: false,
  targetFolder: null,
  currentJobId: null,
  lastCompletedAt: null,
  completedTargetFolder: null,
  isOfflineMode: false,
  persistenceError: null,

  // Actions
  openSession: async (folderPath: string) => {
    // Start persistent job
    let jobId: string;
    let isOffline = false;
    let errorMsg: string | null = null;

    try {
      const job = await invoke<{ jobId: string }>('start_organize_job', {
        targetFolder: folderPath,
      });
      jobId = job.jobId;
    } catch (e) {
      const errStr = e instanceof Error ? e.message : String(e);
      console.error('[Session] Failed to start job:', errStr);
      jobId = `local-${Date.now()}`;
      isOffline = true;
      errorMsg = `Job persistence unavailable: ${errStr}. Changes won't be recoverable if interrupted.`;
    }

    set({
      isOpen: true,
      targetFolder: folderPath,
      currentJobId: jobId,
      lastCompletedAt: null,
      completedTargetFolder: null,
      isOfflineMode: isOffline,
      persistenceError: errorMsg,
    });

    return jobId;
  },

  closeSession: () => {
    // Note: Caller should handle cleanup (abort Grok, clear listeners, etc.)
    set({
      isOpen: false,
      targetFolder: null,
      currentJobId: null,
      isOfflineMode: false,
      persistenceError: null,
    });
  },

  markComplete: (targetFolder: string) => {
    set({
      lastCompletedAt: Date.now(),
      completedTargetFolder: targetFolder,
    });
  },

  setJobId: (jobId) => set({ currentJobId: jobId }),
}));
