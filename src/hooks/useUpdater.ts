import { useCallback, useEffect, useRef } from 'react';
import { check, Update } from '@tauri-apps/plugin-updater';
import { ask } from '@tauri-apps/plugin-dialog';
import { relaunch } from '@tauri-apps/plugin-process';
import { getVersion } from '@tauri-apps/api/app';
import { useUpdateStore, UpdateInfo } from '../stores/update-store';

// Check for updates every 4 hours
const UPDATE_CHECK_INTERVAL = 4 * 60 * 60 * 1000;

export function useUpdater() {
  const {
    status,
    updateInfo,
    downloadProgress,
    error,
    lastChecked,
    setStatus,
    setUpdateInfo,
    setDownloadProgress,
    setError,
    setLastChecked,
    reset,
  } = useUpdateStore();

  const updateRef = useRef<Update | null>(null);

  const checkForUpdates = useCallback(async (silent = false): Promise<boolean> => {
    try {
      setStatus('checking');
      setError(null);

      const update = await check();
      setLastChecked(Date.now());

      if (update) {
        updateRef.current = update;
        const currentVersion = await getVersion();

        const info: UpdateInfo = {
          version: update.version,
          currentVersion,
          releaseNotes: update.body || undefined,
          releaseDate: update.date || undefined,
        };

        setUpdateInfo(info);
        setStatus('available');
        return true;
      } else {
        setStatus('up-to-date');
        return false;
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to check for updates';
      if (!silent) {
        setError(errorMessage);
      }
      setStatus('idle');
      console.error('[Updater] Check failed:', err);
      return false;
    }
  }, [setStatus, setError, setUpdateInfo, setLastChecked]);

  const downloadAndInstall = useCallback(async () => {
    const update = updateRef.current;
    if (!update) {
      setError('No update available');
      return;
    }

    try {
      setStatus('downloading');
      setDownloadProgress(0);

      let totalSize = 0;
      let downloaded = 0;

      // Download with progress
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          totalSize = event.data.contentLength || 0;
          console.log('[Updater] Download started, size:', totalSize);
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          if (totalSize > 0) {
            const percent = Math.round((downloaded / totalSize) * 100);
            setDownloadProgress(percent);
          }
        } else if (event.event === 'Finished') {
          console.log('[Updater] Download finished');
          setDownloadProgress(100);
        }
      });

      setStatus('ready');

      // Ask user to restart
      const shouldRestart = await ask(
        'The update has been installed. Would you like to restart Sentinel now to apply the changes?',
        {
          title: 'Update Ready',
          kind: 'info',
          okLabel: 'Restart Now',
          cancelLabel: 'Later',
        }
      );

      if (shouldRestart) {
        await relaunch();
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to download update';
      setError(errorMessage);
      console.error('[Updater] Download failed:', err);
    }
  }, [setStatus, setDownloadProgress, setError]);

  const dismissUpdate = useCallback(() => {
    reset();
    updateRef.current = null;
  }, [reset]);

  // Auto-check on mount and periodically (silent)
  useEffect(() => {
    // Initial check (silent) after 5 seconds
    const initialTimeout = setTimeout(() => {
      checkForUpdates(true);
    }, 5000);

    // Periodic checks
    const interval = setInterval(() => {
      const timeSinceLastCheck = Date.now() - (lastChecked || 0);
      if (timeSinceLastCheck >= UPDATE_CHECK_INTERVAL) {
        checkForUpdates(true);
      }
    }, 60000); // Check eligibility every minute

    return () => {
      clearTimeout(initialTimeout);
      clearInterval(interval);
    };
  }, [checkForUpdates, lastChecked]);

  return {
    status,
    updateInfo,
    downloadProgress,
    error,
    lastChecked,
    checkForUpdates,
    downloadAndInstall,
    dismissUpdate,
  };
}
