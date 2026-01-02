import { useState } from 'react';
import {
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  LayoutList,
  LayoutGrid,
  Columns3,
  Eye,
  EyeOff,
  Settings,
  Bot,
  Crown,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import { useNavigationStore } from '../../stores/navigation-store';
import { useSubscriptionStore } from '../../stores/subscription-store';
import { UserMenu } from '../auth/UserMenu';
import { UpgradeDialog } from '../dialogs/UpgradeDialog';
import type { ViewMode } from '../../types/file';

interface ToolbarProps {
  onOpenSettings?: () => void;
  onToggleChat?: () => void;
}

export function Toolbar({ onOpenSettings, onToggleChat }: ToolbarProps) {
  const {
    historyIndex,
    history,
    viewMode,
    showHidden,
    goBack,
    goForward,
    goUp,
    setViewMode,
    toggleShowHidden,
  } = useNavigationStore();

  const { tier, isLoading: isSubscriptionLoading } = useSubscriptionStore();
  const isPro = tier === 'pro';
  const [showUpgradeDialog, setShowUpgradeDialog] = useState(false);

  const canGoBack = historyIndex > 0;
  const canGoForward = historyIndex < history.length - 1;

  const viewModes: { mode: ViewMode; icon: typeof LayoutList; label: string }[] = [
    { mode: 'list', icon: LayoutList, label: 'List' },
    { mode: 'grid', icon: LayoutGrid, label: 'Grid' },
    { mode: 'columns', icon: Columns3, label: 'Columns' },
  ];

  return (
    <header data-tauri-drag-region className="glass-toolbar h-12 flex items-center pl-20 pr-4 gap-2">
      {/* Spacer - draggable area */}
      <div data-tauri-drag-region className="flex-1" />

      {/* Navigation buttons */}
      <div className="flex items-center gap-1">
        <button
          onClick={goBack}
          disabled={!canGoBack}
          className={cn(
            'p-1.5 rounded-md transition-colors',
            canGoBack
              ? 'hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300'
              : 'text-gray-300 dark:text-gray-600 cursor-not-allowed'
          )}
          title="Go Back"
        >
          <ChevronLeft size={18} />
        </button>
        <button
          onClick={goForward}
          disabled={!canGoForward}
          className={cn(
            'p-1.5 rounded-md transition-colors',
            canGoForward
              ? 'hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300'
              : 'text-gray-300 dark:text-gray-600 cursor-not-allowed'
          )}
          title="Go Forward"
        >
          <ChevronRight size={18} />
        </button>
        <button
          onClick={goUp}
          className="p-1.5 rounded-md hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 transition-colors"
          title="Go to Parent"
        >
          <ChevronUp size={18} />
        </button>
      </div>

      {/* View mode toggle */}
      <div className="flex items-center bg-slate-200 dark:bg-neutral-800 rounded-md p-0.5">
        {viewModes.map(({ mode, icon: Icon, label }) => (
          <button
            key={mode}
            onClick={() => setViewMode(mode)}
            className={cn(
              'p-1.5 rounded transition-colors',
              viewMode === mode
                ? 'bg-white dark:bg-neutral-700 shadow-sm text-orange-500 dark:text-orange-400'
                : 'text-slate-500 dark:text-neutral-500 hover:text-slate-700 dark:hover:text-neutral-400'
            )}
            title={label}
          >
            <Icon size={16} />
          </button>
        ))}
      </div>

      {/* Hidden files toggle */}
      <button
        onClick={toggleShowHidden}
        className={cn(
          'p-1.5 rounded-md transition-colors',
          showHidden
            ? 'bg-orange-100 dark:bg-orange-900/30 text-orange-500 dark:text-orange-400'
            : 'hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-500 dark:text-gray-400'
        )}
        title={showHidden ? 'Hide Hidden Files' : 'Show Hidden Files'}
      >
        {showHidden ? <Eye size={16} /> : <EyeOff size={16} />}
      </button>

      {/* Settings button */}
      {onOpenSettings && (
        <button
          onClick={onOpenSettings}
          className="p-1.5 rounded-md hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-500 dark:text-gray-400 transition-colors"
          title="Settings"
        >
          <Settings size={16} />
        </button>
      )}

      {/* Chat button */}
      <button
        onClick={onToggleChat}
        className="p-1.5 rounded-md hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-500 dark:text-gray-400 transition-colors"
        title="Chat"
      >
        <Bot size={16} />
      </button>

      {/* Plan badge */}
      <div className="flex items-center gap-1">
        {isSubscriptionLoading ? (
          // Show loading state while fetching subscription
          <span className="flex items-center gap-1 px-2 py-1 rounded-md text-xs font-medium bg-slate-200 dark:bg-neutral-700 text-slate-400 dark:text-neutral-500 animate-pulse">
            ...
          </span>
        ) : (
          <>
            <span
              className={cn(
                'flex items-center gap-1 px-2 py-1 rounded-md text-xs font-medium',
                isPro
                  ? 'bg-gradient-to-r from-orange-500 to-amber-500 text-white'
                  : 'bg-slate-200 dark:bg-neutral-700 text-slate-600 dark:text-neutral-300'
              )}
            >
              {isPro && <Crown size={12} />}
              {isPro ? 'Pro' : 'Free'}
            </span>

            {!isPro && (
              <button
                onClick={() => setShowUpgradeDialog(true)}
                className="px-2 py-1 rounded-md text-xs font-medium bg-orange-500 hover:bg-orange-600 text-white transition-colors"
                title="Upgrade to Pro"
              >
                Upgrade
              </button>
            )}
          </>
        )}
      </div>

      {/* User menu (shows when signed in) */}
      <UserMenu />

      {/* Upgrade dialog */}
      <UpgradeDialog
        isOpen={showUpgradeDialog}
        onClose={() => setShowUpgradeDialog(false)}
      />
    </header>
  );
}
