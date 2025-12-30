import { create } from 'zustand';

export interface ToastData {
  id: string;
  type: 'success' | 'error' | 'info' | 'rename';
  title: string;
  message?: string;
  duration?: number;
  onUndo?: () => void;
  undoTimeout?: number;
}

interface ToastState {
  toasts: ToastData[];
}

interface ToastActions {
  addToast: (toast: Omit<ToastData, 'id'>) => string;
  removeToast: (id: string) => void;
  clearAllToasts: () => void;
}

let toastId = 0;

export const useToastStore = create<ToastState & ToastActions>((set) => ({
  toasts: [],

  addToast: (toast) => {
    const id = `toast-${++toastId}`;
    set((state) => ({
      toasts: [...state.toasts, { ...toast, id }],
    }));
    return id;
  },

  removeToast: (id) => {
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    }));
  },

  clearAllToasts: () => {
    set({ toasts: [] });
  },
}));

// Convenience functions
export function showToast(toast: Omit<ToastData, 'id'>) {
  return useToastStore.getState().addToast(toast);
}

export function showSuccess(title: string, message?: string) {
  return showToast({ type: 'success', title, message });
}

export function showError(title: string, message?: string) {
  return showToast({ type: 'error', title, message, duration: 5000 });
}

export function showInfo(title: string, message?: string) {
  return showToast({ type: 'info', title, message });
}

export function showRenameToast(
  title: string,
  message: string,
  onUndo: () => void
) {
  return showToast({
    type: 'rename',
    title,
    message,
    onUndo,
    undoTimeout: 6000,
  });
}
