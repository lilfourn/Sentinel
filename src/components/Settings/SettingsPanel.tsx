import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { X, Eye, EyeOff, Check, Loader2, FolderOpen } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useWatcher } from '../../hooks/useAutoRename';
import { useSyncedSettings } from '../../hooks/useSyncedSettings';
import { showSuccess, showError } from '../../stores/toast-store';

interface SettingsPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

interface ProviderStatus {
  provider: string;
  configured: boolean;
}

export function SettingsPanel({ isOpen, onClose }: SettingsPanelProps) {
  const [apiKey, setApiKey] = useState('');
  const [showApiKey, setShowApiKey] = useState(false);
  const [providers, setProviders] = useState<ProviderStatus[]>([]);
  const [savingKey, setSavingKey] = useState(false);
  const [watcherEnabled, setWatcherEnabled] = useState(false);
  const [watchingPath, setWatchingPath] = useState<string | null>(null);
  const [loadingWatcher, setLoadingWatcher] = useState(false);

  const { startWatcher, stopWatcher, getStatus } = useWatcher();
  const { watchDownloads, skipDeleteConfirmation, updateSettings } = useSyncedSettings();

  // Load initial state
  useEffect(() => {
    if (!isOpen) return;

    // Load provider status
    invoke<ProviderStatus[]>('get_configured_providers').then(setProviders);

    // Load watcher status
    getStatus().then((status) => {
      setWatcherEnabled(status.enabled);
      setWatchingPath(status.watchingPath);
    });
  }, [isOpen, getStatus]);

  // Sync local watcher state with settings store
  useEffect(() => {
    setWatcherEnabled(watchDownloads);
  }, [watchDownloads]);

  const handleSaveApiKey = async () => {
    if (!apiKey.trim()) return;

    setSavingKey(true);
    try {
      const isValid = await invoke<boolean>('set_api_key', {
        provider: 'anthropic',
        apiKey: apiKey.trim(),
      });

      if (isValid) {
        showSuccess('API key saved', 'Your Anthropic API key has been saved securely');
        setApiKey('');
        // Refresh provider status
        const status = await invoke<ProviderStatus[]>('get_configured_providers');
        setProviders(status);
      } else {
        showError('Invalid API key', 'The API key could not be validated');
      }
    } catch (error) {
      showError('Failed to save', String(error));
    } finally {
      setSavingKey(false);
    }
  };

  const handleDeleteApiKey = async () => {
    try {
      await invoke('delete_api_key', { provider: 'anthropic' });
      showSuccess('API key deleted');
      const status = await invoke<ProviderStatus[]>('get_configured_providers');
      setProviders(status);
    } catch (error) {
      showError('Failed to delete', String(error));
    }
  };

  const handleToggleWatcher = async () => {
    setLoadingWatcher(true);
    try {
      if (watcherEnabled) {
        await stopWatcher();
        setWatcherEnabled(false);
        setWatchingPath(null);
        // Save the setting to local store and Convex
        await updateSettings({ watchDownloads: false });
      } else {
        const success = await startWatcher();
        if (success) {
          const status = await getStatus();
          setWatcherEnabled(status.enabled);
          setWatchingPath(status.watchingPath);
          // Save the setting to local store and Convex
          await updateSettings({ watchDownloads: true });
        }
      }
    } finally {
      setLoadingWatcher(false);
    }
  };

  const anthropicConfigured = providers.find((p) => p.provider === 'anthropic')?.configured;

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white dark:bg-[#2a2a2a] rounded-xl shadow-2xl w-full max-w-md mx-4 border border-gray-200 dark:border-gray-700">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-gray-200 dark:border-gray-700">
          <h2 className="text-lg font-semibold">Settings</h2>
          <button
            onClick={onClose}
            className="p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-500"
          >
            <X size={20} />
          </button>
        </div>

        {/* Content */}
        <div className="p-4 space-y-6">
          {/* API Key Section */}
          <section>
            <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">
              AI Configuration
            </h3>

            <div className="space-y-3">
              {/* Status */}
              <div className="flex items-center justify-between text-sm">
                <span className="text-gray-600 dark:text-gray-400">Anthropic API</span>
                {anthropicConfigured ? (
                  <span className="flex items-center gap-1 text-green-600">
                    <Check size={14} />
                    Configured
                  </span>
                ) : (
                  <span className="text-gray-400">Not configured</span>
                )}
              </div>

              {/* API Key Input */}
              {!anthropicConfigured && (
                <div className="space-y-2">
                  <div className="relative">
                    <input
                      type={showApiKey ? 'text' : 'password'}
                      value={apiKey}
                      onChange={(e) => setApiKey(e.target.value)}
                      placeholder="sk-ant-..."
                      className="w-full px-3 py-2 pr-10 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-orange-500"
                    />
                    <button
                      type="button"
                      onClick={() => setShowApiKey(!showApiKey)}
                      className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-gray-400 hover:text-gray-600"
                    >
                      {showApiKey ? <EyeOff size={16} /> : <Eye size={16} />}
                    </button>
                  </div>
                  <button
                    onClick={handleSaveApiKey}
                    disabled={!apiKey.trim() || savingKey}
                    className={cn(
                      'w-full py-2 px-4 text-sm font-medium rounded-lg transition-colors',
                      'bg-orange-500 text-white hover:bg-orange-600',
                      'disabled:bg-gray-300 disabled:text-gray-500 disabled:cursor-not-allowed'
                    )}
                  >
                    {savingKey ? (
                      <Loader2 size={16} className="animate-spin mx-auto" />
                    ) : (
                      'Save API Key'
                    )}
                  </button>
                </div>
              )}

              {/* Delete Key Button */}
              {anthropicConfigured && (
                <button
                  onClick={handleDeleteApiKey}
                  className="text-sm text-red-600 hover:text-red-700"
                >
                  Remove API key
                </button>
              )}
            </div>
          </section>

          {/* Auto-Rename Section */}
          <section>
            <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">
              Auto-Rename Sentinel
            </h3>

            <div className="space-y-3">
              {/* Toggle */}
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm text-gray-900 dark:text-gray-100">
                    Watch Downloads folder
                  </p>
                  <p className="text-xs text-gray-500 dark:text-gray-400">
                    Automatically rename new files using AI
                  </p>
                </div>
                <button
                  onClick={handleToggleWatcher}
                  disabled={!anthropicConfigured || loadingWatcher}
                  className={cn(
                    'relative w-11 h-6 rounded-full transition-colors',
                    watcherEnabled
                      ? 'bg-orange-500'
                      : 'bg-gray-300 dark:bg-gray-600',
                    (!anthropicConfigured || loadingWatcher) && 'opacity-50 cursor-not-allowed'
                  )}
                >
                  <span
                    className={cn(
                      'absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform',
                      watcherEnabled && 'translate-x-5'
                    )}
                  />
                </button>
              </div>

              {/* Watching path */}
              {watchingPath && (
                <div className="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400">
                  <FolderOpen size={14} />
                  <span className="truncate">{watchingPath}</span>
                </div>
              )}

              {/* Warning if no API key */}
              {!anthropicConfigured && (
                <p className="text-xs text-amber-600 dark:text-amber-400">
                  Configure your Anthropic API key to enable auto-rename
                </p>
              )}
            </div>
          </section>

          {/* File Operations Section */}
          <section>
            <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">
              File Operations
            </h3>

            <div className="space-y-3">
              {/* Skip delete confirmation toggle */}
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm text-gray-900 dark:text-gray-100">
                    Skip delete confirmation
                  </p>
                  <p className="text-xs text-gray-500 dark:text-gray-400">
                    Move files to Trash without asking
                  </p>
                </div>
                <button
                  onClick={() => updateSettings({ skipDeleteConfirmation: !skipDeleteConfirmation })}
                  className={cn(
                    'relative w-11 h-6 rounded-full transition-colors',
                    skipDeleteConfirmation
                      ? 'bg-orange-500'
                      : 'bg-gray-300 dark:bg-gray-600'
                  )}
                >
                  <span
                    className={cn(
                      'absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform',
                      skipDeleteConfirmation && 'translate-x-5'
                    )}
                  />
                </button>
              </div>
            </div>
          </section>
        </div>

        {/* Footer */}
        <div className="p-4 border-t border-gray-200 dark:border-gray-700">
          <button
            onClick={onClose}
            className="w-full py-2 px-4 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
