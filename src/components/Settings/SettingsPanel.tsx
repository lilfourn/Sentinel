import { useState, useEffect, Component, ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { X, Check, CreditCard, Sparkles, ChevronDown, ChevronUp, AlertTriangle, RefreshCw, Download } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useWatcher } from '../../hooks/useAutoRename';
import { useSyncedSettings } from '../../hooks/useSyncedSettings';
import {
  useSubscriptionStore,
  PRO_PRICE,
} from '../../stores/subscription-store';
import { UsageDashboard, PlanBadge } from '../subscription';
import {
  RenameHistoryPanel,
  WatchedFoldersManager,
  CustomRulesEditor,
} from '../downloads-watcher';
import { useUpdater } from '../../hooks/useUpdater';

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
        // Save the setting to local store and Convex
        await updateSettings({ watchDownloads: false });
      } else {
        const success = await startWatcher();
        if (success) {
          const status = await getStatus();
          setWatcherEnabled(status.enabled);
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
          <AutoRenameSentinelSection
            anthropicConfigured={anthropicConfigured}
            watcherEnabled={watcherEnabled}
            loadingWatcher={loadingWatcher}
            onToggleWatcher={handleToggleWatcher}
          />

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

          {/* Software Updates Section */}
          <UpdatesSection />

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
 * Error boundary to catch rendering errors in settings sections
 * Prevents the entire settings panel from crashing if a section fails
 */
interface SettingsErrorBoundaryState {
  hasError: boolean;
  error?: Error;
}

interface SettingsErrorBoundaryProps {
  children: ReactNode;
  sectionName: string;
}

class SettingsErrorBoundary extends Component<
  SettingsErrorBoundaryProps,
  SettingsErrorBoundaryState
> {
  constructor(props: SettingsErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false };
  }

  static getDerivedStateFromError(error: Error): SettingsErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error(`[Settings] ${this.props.sectionName} error:`, error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg">
          <div className="flex items-center gap-2 text-red-700 dark:text-red-400">
            <AlertTriangle size={14} />
            <span className="text-xs font-medium">
              {this.props.sectionName} failed to load
            </span>
          </div>
          <p className="text-xs text-red-600 dark:text-red-400 mt-1">
            Try reloading the app. Error: {this.state.error?.message || 'Unknown error'}
          </p>
        </div>
      );
    }

    return this.props.children;
  }
}

/**
 * Auto-Rename Sentinel section with expanded features
 */
interface AutoRenameSentinelSectionProps {
  anthropicConfigured: boolean | undefined;
  watcherEnabled: boolean;
  loadingWatcher: boolean;
  onToggleWatcher: () => void;
}

function AutoRenameSentinelSection({
  anthropicConfigured,
  watcherEnabled,
  loadingWatcher,
  onToggleWatcher,
}: AutoRenameSentinelSectionProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <section className="space-y-3">
      {/* Header with toggle */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Auto-Rename Sentinel
          </h3>
          {watcherEnabled && (
            <span className="px-1.5 py-0.5 text-xs rounded-full bg-green-100 dark:bg-green-900/30 text-green-600 dark:text-green-400">
              Active
            </span>
          )}
        </div>
        <button
          onClick={onToggleWatcher}
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

      {/* Description */}
      <p className="text-xs text-gray-500 dark:text-gray-400">
        Automatically rename new files using AI when they appear in watched folders
      </p>

      {/* Warning if AI service unavailable */}
      {!anthropicConfigured && (
        <p className="text-xs text-amber-600 dark:text-amber-400 p-2 bg-amber-50 dark:bg-amber-900/20 rounded">
          AI service temporarily unavailable. Please try again later or contact support.
        </p>
      )}

      {/* Expand/collapse for advanced options */}
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-1 text-xs text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
      >
        {expanded ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
        {expanded ? 'Hide options' : 'Show folders, rules & history'}
      </button>

      {/* Expanded sections */}
      {expanded && (
        <div className="space-y-4 pt-2 border-t border-gray-100 dark:border-gray-700">
          {/* Watched Folders */}
          <div className="space-y-2">
            <h4 className="text-xs font-medium text-gray-600 dark:text-gray-400 uppercase tracking-wider">
              Watched Folders
            </h4>
            <SettingsErrorBoundary sectionName="Watched Folders">
              <WatchedFoldersManager />
            </SettingsErrorBoundary>
          </div>

          {/* Custom Rules */}
          <div className="space-y-2">
            <h4 className="text-xs font-medium text-gray-600 dark:text-gray-400 uppercase tracking-wider">
              Rename Rules
            </h4>
            <SettingsErrorBoundary sectionName="Rename Rules">
              <CustomRulesEditor compact />
            </SettingsErrorBoundary>
          </div>

          {/* Rename History */}
          <div className="space-y-2">
            <h4 className="text-xs font-medium text-gray-600 dark:text-gray-400 uppercase tracking-wider">
              Recent Renames
            </h4>
            <SettingsErrorBoundary sectionName="Rename History">
              <RenameHistoryPanel maxItems={10} compact />
            </SettingsErrorBoundary>
          </div>
        </div>
      )}
    </section>
  );
}

/**
 * Software Updates section
 */
function UpdatesSection() {
  const {
    status,
    updateInfo,
    lastChecked,
    checkForUpdates,
    downloadAndInstall,
  } = useUpdater();

  const lastCheckedText = lastChecked
    ? `Last checked: ${new Date(lastChecked).toLocaleString()}`
    : 'Never checked';

  return (
    <section>
      <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">
        Software Updates
      </h3>

      <div className="space-y-3">
        {/* Current version info */}
        <div className="flex items-center justify-between text-xs text-gray-500">
          <span>{lastCheckedText}</span>
          {status === 'up-to-date' && (
            <span className="text-green-500 flex items-center gap-1">
              <Check size={12} />
              Up to date
            </span>
          )}
        </div>

        {/* Update available banner */}
        {status === 'available' && updateInfo && (
          <div className="p-3 bg-orange-50 dark:bg-orange-900/20 border border-orange-200 dark:border-orange-800 rounded-lg">
            <p className="text-sm text-orange-700 dark:text-orange-300 font-medium">
              Version {updateInfo.version} is available!
            </p>
            <button
              onClick={downloadAndInstall}
              className="mt-2 w-full flex items-center justify-center gap-2 py-1.5 px-3 text-xs font-medium bg-orange-500 text-white rounded hover:bg-orange-600 transition-colors"
            >
              <Download size={12} />
              Download & Install
            </button>
          </div>
        )}

        {/* Check for updates button */}
        <button
          onClick={() => checkForUpdates(false)}
          disabled={status === 'checking' || status === 'downloading'}
          className={cn(
            'w-full flex items-center justify-center gap-2 py-2 px-4 text-sm font-medium rounded-lg transition-colors',
            'bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300',
            'hover:bg-gray-300 dark:hover:bg-gray-600',
            (status === 'checking' || status === 'downloading') && 'opacity-50 cursor-not-allowed'
          )}
        >
          <RefreshCw size={14} className={status === 'checking' ? 'animate-spin' : ''} />
          {status === 'checking' ? 'Checking...' : 'Check for Updates'}
        </button>
      </div>
    </section>
  );
}

/**
 * Subscription management section
 * Note: Subscription data is synced from Convex via AuthSync component.
 * We don't call syncSubscription() here as it would fetch stale data from Rust cache.
 */
function SubscriptionSection() {
  const {
    tier,
    status,
    currentPeriodEnd,
    cancelAtPeriodEnd,
    isLoading,
    openCheckout,
    openCustomerPortal,
  } = useSubscriptionStore();

  const periodEndDate = currentPeriodEnd
    ? new Date(currentPeriodEnd).toLocaleDateString()
    : null;

  // User is Pro but has cancelled - will downgrade at period end
  const isCancelling = tier === 'pro' && cancelAtPeriodEnd && status === 'active';

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
          </div>
          {tier === 'pro' && status === 'active' && periodEndDate && (
            <span className={cn(
              "text-xs",
              isCancelling ? "text-amber-500" : "text-gray-500"
            )}>
              {isCancelling ? `Cancels ${periodEndDate}` : `Renews ${periodEndDate}`}
            </span>
          )}
        </div>

        {/* Cancellation notice */}
        {isCancelling && (
          <div className="p-3 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-lg">
            <p className="text-xs text-amber-700 dark:text-amber-300">
              Your subscription has been cancelled. You'll continue to have Pro access
              {periodEndDate ? ` until ${periodEndDate}` : ' until your billing cycle ends'}.
            </p>
          </div>
        )}

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
                Extended thinking (5/day)
              </li>
              <li className="flex items-center gap-1.5">
                <Check size={12} className="text-green-500" />
                300 Haiku requests/day
              </li>
              <li className="flex items-center gap-1.5">
                <Check size={12} className="text-green-500" />
                20 AI organizes/day
              </li>
              <li className="flex items-center gap-1.5">
                <Check size={12} className="text-green-500" />
                100 AI renames/day
              </li>
            </ul>
          </div>
        )}
      </div>
    </section>
  );
}
