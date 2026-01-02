import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { X, Check, FolderOpen, CreditCard, Sparkles } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useWatcher } from '../../hooks/useAutoRename';
import { useSyncedSettings } from '../../hooks/useSyncedSettings';
import {
  useSubscriptionStore,
  PRO_PRICE,
} from '../../stores/subscription-store';
import { UsageDashboard, PlanBadge } from '../subscription';

interface SettingsPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

interface ProviderStatus {
  provider: string;
  configured: boolean;
}

export function SettingsPanel({ isOpen, onClose }: SettingsPanelProps) {
  const [providers, setProviders] = useState<ProviderStatus[]>([]);
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

  // Check if Anthropic is configured (needed for auto-rename watcher)
  const anthropicConfigured = providers.find((p) => p.provider === 'anthropic')?.configured;

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white dark:bg-[#2a2a2a] rounded-xl shadow-2xl w-full max-w-md mx-4 border border-gray-200 dark:border-gray-700 max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-gray-200 dark:border-gray-700 sticky top-0 bg-white dark:bg-[#2a2a2a]">
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

          {/* Subscription Section */}
          <SubscriptionSection />
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

/**
 * Subscription management section
 */
function SubscriptionSection() {
  const {
    tier,
    status,
    currentPeriodEnd,
    isLoading,
    openCheckout,
    openCustomerPortal,
    syncSubscription,
  } = useSubscriptionStore();

  // Sync on mount
  useEffect(() => {
    syncSubscription();
  }, [syncSubscription]);

  const periodEndDate = currentPeriodEnd
    ? new Date(currentPeriodEnd).toLocaleDateString()
    : null;

  return (
    <section>
      <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">
        Subscription
      </h3>

      <div className="space-y-4">
        {/* Current plan */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-sm text-gray-900 dark:text-gray-100">
              Current Plan
            </span>
            <PlanBadge size="md" />
            <button
              onClick={() => {
                console.log("[Settings] Manual subscription refresh");
                syncSubscription();
              }}
              disabled={isLoading}
              className="text-xs text-blue-500 hover:text-blue-400 disabled:opacity-50"
            >
              Refresh
            </button>
          </div>
          {tier === 'pro' && status === 'active' && periodEndDate && (
            <span className="text-xs text-gray-500">
              Renews {periodEndDate}
            </span>
          )}
        </div>

        {/* Usage dashboard */}
        <div className="p-3 bg-gray-100 dark:bg-[#1a1a1a] rounded-lg">
          <p className="text-xs text-gray-500 mb-2">Today's Usage</p>
          <UsageDashboard />
        </div>

        {/* Action buttons */}
        <div className="space-y-2">
          {tier === 'free' ? (
            <button
              onClick={() => openCheckout()}
              disabled={isLoading}
              className={cn(
                'w-full flex items-center justify-center gap-2 py-2 px-4 text-sm font-medium rounded-lg transition-colors',
                'bg-gradient-to-r from-orange-500 to-purple-500 text-white hover:from-orange-600 hover:to-purple-600',
                isLoading && 'opacity-50 cursor-not-allowed'
              )}
            >
              <Sparkles size={16} />
              Upgrade to Pro - ${PRO_PRICE}/mo
            </button>
          ) : (
            <button
              onClick={() => openCustomerPortal()}
              disabled={isLoading}
              className={cn(
                'w-full flex items-center justify-center gap-2 py-2 px-4 text-sm font-medium rounded-lg transition-colors',
                'bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300',
                'hover:bg-gray-300 dark:hover:bg-gray-600',
                isLoading && 'opacity-50 cursor-not-allowed'
              )}
            >
              <CreditCard size={16} />
              Manage Subscription
            </button>
          )}
        </div>

        {/* Pro features list (for free users) */}
        {tier === 'free' && (
          <div className="pt-2 border-t border-gray-200 dark:border-gray-700">
            <p className="text-xs text-gray-500 mb-2">Pro includes:</p>
            <ul className="space-y-1 text-xs text-gray-600 dark:text-gray-400">
              <li className="flex items-center gap-1.5">
                <Check size={12} className="text-green-500" />
                Sonnet 4.5 (50/day)
              </li>
              <li className="flex items-center gap-1.5">
                <Check size={12} className="text-green-500" />
                Opus 4.5 (10/day)
              </li>
              <li className="flex items-center gap-1.5">
                <Check size={12} className="text-green-500" />
                Extended thinking (5/day)
              </li>
              <li className="flex items-center gap-1.5">
                <Check size={12} className="text-green-500" />
                300 Haiku requests/day
              </li>
            </ul>
          </div>
        )}
      </div>
    </section>
  );
}
