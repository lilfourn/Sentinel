import { create } from 'zustand';

export interface UpdateInfo {
  version: string;
  currentVersion: string;
  releaseNotes?: string;
  releaseDate?: string;
}

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'ready'
  | 'error'
  | 'up-to-date';

interface UpdateState {
  status: UpdateStatus;
  updateInfo: UpdateInfo | null;
  downloadProgress: number;
  error: string | null;
  lastChecked: number | null;

  // Actions
  setStatus: (status: UpdateStatus) => void;
  setUpdateInfo: (info: UpdateInfo | null) => void;
  setDownloadProgress: (progress: number) => void;
  setError: (error: string | null) => void;
  setLastChecked: (timestamp: number) => void;
  reset: () => void;
}

export const useUpdateStore = create<UpdateState>((set) => ({
  status: 'idle',
  updateInfo: null,
  downloadProgress: 0,
  error: null,
  lastChecked: null,

  setStatus: (status) => set({ status }),
  setUpdateInfo: (updateInfo) => set({ updateInfo }),
  setDownloadProgress: (downloadProgress) => set({ downloadProgress }),
  setError: (error) => set({ error, status: error ? 'error' : 'idle' }),
  setLastChecked: (lastChecked) => set({ lastChecked }),
  reset: () => set({
    status: 'idle',
    updateInfo: null,
    downloadProgress: 0,
    error: null,
  }),
}));
