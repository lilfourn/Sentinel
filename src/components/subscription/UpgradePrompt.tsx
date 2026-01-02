import { useSubscriptionStore, PRO_PRICE } from '../../stores/subscription-store';

type UpgradeReason = 'model_locked' | 'limit_reached' | 'extended_thinking';

interface UpgradePromptProps {
  reason: UpgradeReason;
  modelName?: string;
  onDismiss?: () => void;
  className?: string;
}

const MESSAGES: Record<UpgradeReason, (modelName?: string) => string> = {
  model_locked: (model) => `Upgrade to Pro for access to ${model || 'premium models'}`,
  limit_reached: () => "You've reached your daily limit",
  extended_thinking: () => 'Extended thinking requires Pro',
};

/**
 * Inline upgrade CTA shown when user hits limits or tries premium features
 */
export function UpgradePrompt({
  reason,
  modelName,
  onDismiss,
  className = '',
}: UpgradePromptProps) {
  const { openCheckout } = useSubscriptionStore();

  const message = MESSAGES[reason](modelName);

  return (
    <div
      className={`flex items-center gap-3 p-3 bg-gradient-to-r from-orange-500/10 to-purple-500/10
                  rounded-lg border border-orange-500/20 ${className}`}
    >
      <SparklesIcon className="w-5 h-5 text-orange-400 shrink-0" />
      <span className="flex-1 text-sm text-gray-200">{message}</span>
      <button
        onClick={() => openCheckout()}
        className="px-3 py-1.5 bg-orange-500 hover:bg-orange-600 text-white text-xs
                   font-medium rounded-lg transition-colors shrink-0"
      >
        Upgrade - ${PRO_PRICE}/mo
      </button>
      {onDismiss && (
        <button
          onClick={onDismiss}
          className="p-1 text-gray-500 hover:text-gray-300 transition-colors"
        >
          <XIcon className="w-4 h-4" />
        </button>
      )}
    </div>
  );
}

/**
 * Compact upgrade badge for tight spaces
 */
export function UpgradeBadge({ onClick }: { onClick?: () => void }) {
  const { openCheckout } = useSubscriptionStore();

  return (
    <button
      onClick={onClick || (() => openCheckout())}
      className="inline-flex items-center gap-1 px-2 py-0.5 text-[10px] font-medium
                 text-orange-400 bg-orange-500/10 rounded border border-orange-500/20
                 hover:bg-orange-500/20 transition-colors"
    >
      <SparklesIcon className="w-3 h-3" />
      PRO
    </button>
  );
}

// Simple inline icons
function SparklesIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeWidth={1.5}
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.456 2.456L21.75 6l-1.035.259a3.375 3.375 0 00-2.456 2.456zM16.894 20.567L16.5 21.75l-.394-1.183a2.25 2.25 0 00-1.423-1.423L13.5 18.75l1.183-.394a2.25 2.25 0 001.423-1.423l.394-1.183.394 1.183a2.25 2.25 0 001.423 1.423l1.183.394-1.183.394a2.25 2.25 0 00-1.423 1.423z"
      />
    </svg>
  );
}

function XIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeWidth={2}
    >
      <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
    </svg>
  );
}
