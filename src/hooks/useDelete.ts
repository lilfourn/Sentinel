import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useSettingsStore } from '../stores/settings-store';

/**
 * Structured error returned by delete_to_trash command
 */
interface DeleteError {
  type: 'NOT_FOUND' | 'PROTECTED_PATH' | 'I_CLOUD_DOWNLOAD_REQUIRED' | 'IO_ERROR';
  path?: string;
  message?: string;
}

interface DeleteState {
  pendingPath: string | null;
  pendingName: string | null;
  showConfirmation: boolean;
  showICloudError: boolean;
  iCloudFileName: string | null;
}

/**
 * Hook for handling file/folder deletion with confirmation and iCloud error handling
 *
 * @param onSuccess - Callback to run after successful deletion (e.g., refresh directory)
 * @returns Object with delete functions and dialog state
 */
export function useDelete(onSuccess?: () => void) {
  const { skipDeleteConfirmation, setSkipDeleteConfirmation } = useSettingsStore();

  const [state, setState] = useState<DeleteState>({
    pendingPath: null,
    pendingName: null,
    showConfirmation: false,
    showICloudError: false,
    iCloudFileName: null,
  });

  /**
   * Execute the actual delete operation
   */
  const executeDelete = useCallback(async (path: string) => {
    try {
      await invoke('delete_to_trash', { path });
      onSuccess?.();
    } catch (error) {
      // Parse the error - Tauri returns it as a string or object
      let deleteError: DeleteError;

      if (typeof error === 'object' && error !== null && 'type' in error) {
        deleteError = error as DeleteError;
      } else {
        // Try to parse as JSON string
        const errorStr = String(error);
        try {
          deleteError = JSON.parse(errorStr);
        } catch {
          // If not JSON, treat as generic error
          deleteError = { type: 'IO_ERROR', message: errorStr };
        }
      }

      if (deleteError.type === 'I_CLOUD_DOWNLOAD_REQUIRED') {
        const fileName = path.split('/').pop() || path;
        setState(prev => ({
          ...prev,
          showICloudError: true,
          iCloudFileName: fileName,
          pendingPath: path,
        }));
        return;
      }

      // Log error for debugging but don't show toast
      const message = deleteError.message ||
        (deleteError.type === 'NOT_FOUND' ? 'File not found' :
         deleteError.type === 'PROTECTED_PATH' ? 'Cannot delete protected path' :
         'Failed to move to Trash');
      console.error('Delete failed:', message);
    }
  }, [onSuccess]);

  /**
   * Request deletion of a file/folder
   * Shows confirmation dialog if skipDeleteConfirmation is false
   */
  const requestDelete = useCallback((path: string, name?: string) => {
    if (skipDeleteConfirmation) {
      executeDelete(path);
    } else {
      setState({
        pendingPath: path,
        pendingName: name || path.split('/').pop() || path,
        showConfirmation: true,
        showICloudError: false,
        iCloudFileName: null,
      });
    }
  }, [skipDeleteConfirmation, executeDelete]);

  /**
   * Confirm deletion from the confirmation dialog
   */
  const confirmDelete = useCallback((dontAskAgain: boolean) => {
    if (dontAskAgain) {
      setSkipDeleteConfirmation(true);
    }
    if (state.pendingPath) {
      executeDelete(state.pendingPath);
    }
    setState(prev => ({ ...prev, showConfirmation: false, pendingPath: null }));
  }, [state.pendingPath, executeDelete, setSkipDeleteConfirmation]);

  /**
   * Cancel the deletion
   */
  const cancelDelete = useCallback(() => {
    setState({
      pendingPath: null,
      pendingName: null,
      showConfirmation: false,
      showICloudError: false,
      iCloudFileName: null,
    });
  }, []);

  /**
   * Close the iCloud error dialog
   */
  const closeICloudError = useCallback(() => {
    setState(prev => ({
      ...prev,
      showICloudError: false,
      iCloudFileName: null,
      pendingPath: null,
    }));
  }, []);

  /**
   * Use quarantine as fallback for iCloud files
   */
  const useQuarantineFallback = useCallback(async () => {
    if (!state.pendingPath) return;

    try {
      await invoke('quarantine_item', { path: state.pendingPath });
      setState({
        pendingPath: null,
        pendingName: null,
        showConfirmation: false,
        showICloudError: false,
        iCloudFileName: null,
      });
      onSuccess?.();
    } catch (error) {
      console.error('Quarantine failed:', error);
    }
  }, [state.pendingPath, onSuccess]);

  return {
    // Functions
    requestDelete,
    confirmDelete,
    cancelDelete,
    closeICloudError,
    useQuarantineFallback,
    // State for dialogs
    showConfirmation: state.showConfirmation,
    showICloudError: state.showICloudError,
    pendingName: state.pendingName,
    iCloudFileName: state.iCloudFileName,
  };
}
