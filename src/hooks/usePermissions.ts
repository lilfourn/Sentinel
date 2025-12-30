import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ProtectedDirectory } from '../types/permissions';

export function usePermissions() {
  const [protectedDirs, setProtectedDirs] = useState<ProtectedDirectory[]>([]);
  const [isChecking, setIsChecking] = useState(false);
  const [hasChecked, setHasChecked] = useState(false);

  const checkPermissions = useCallback(async () => {
    setIsChecking(true);
    try {
      const dirs = await invoke<[string, string, boolean][]>('get_protected_directories');
      setProtectedDirs(
        dirs.map(([name, path, accessible]) => ({
          name,
          path,
          accessible,
        }))
      );
      setHasChecked(true);
    } catch (error) {
      console.error('Failed to check permissions:', error);
    } finally {
      setIsChecking(false);
    }
  }, []);

  const openSystemPreferences = useCallback(async () => {
    try {
      await invoke('open_privacy_settings');
    } catch (error) {
      console.error('Failed to open System Preferences:', error);
    }
  }, []);

  // Check on mount
  useEffect(() => {
    checkPermissions();
  }, [checkPermissions]);

  // Check if any directories are inaccessible
  const hasPermissionIssues = protectedDirs.some((d) => !d.accessible);
  const inaccessibleDirs = protectedDirs.filter((d) => !d.accessible);

  return {
    protectedDirs,
    inaccessibleDirs,
    hasPermissionIssues,
    isChecking,
    hasChecked,
    checkPermissions,
    openSystemPreferences,
  };
}
