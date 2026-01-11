import { useSubscriptionStore, TIER_LIMITS, type ModelType } from '../../stores/subscription-store';

interface UsageMeterProps {
  model: ModelType;
  variant?: 'compact' | 'full';
  className?: string;
}

/**
 * Usage meter showing remaining requests for a model
 */
export function UsageMeter({ model, variant = 'compact', className = '' }: UsageMeterProps) {
  const { tier, usage } = useSubscriptionStore();
  const limits = TIER_LIMITS[tier];
  const limit = limits[model];

  // Get current usage
  let current = 0;
  switch (model) {
    case 'haiku':
      current = usage.haikuRequests;
      break;
    case 'sonnet':
      current = usage.sonnetRequests;
      break;
    case 'gpt52':
      current = usage.gpt52Requests;
      break;
    case 'gpt5mini':
      current = usage.gpt5miniRequests;
      break;
    case 'gpt5nano':
      current = usage.gpt5nanoRequests;
      break;
  }

  // If model not available on this tier, don't show meter
  if (limit === 0) {
    return null;
  }

  const percentage = (current / limit) * 100;
  const remaining = Math.max(0, limit - current);

  // Color coding: green (< 70%), yellow (70-90%), red (> 90%)
  const getColor = () => {
    if (percentage < 70) return 'bg-green-500';
    if (percentage < 90) return 'bg-yellow-500';
    return 'bg-red-500';
  };

  if (variant === 'compact') {
    return (
      <div className={`flex items-center gap-1.5 ${className}`}>
        <div className="w-10 h-1 bg-[#3a3a3a] rounded-full overflow-hidden">
          <div
            className={`h-full transition-all duration-300 ${getColor()}`}
            style={{ width: `${Math.min(100, percentage)}%` }}
          />
        </div>
        <span className="text-[10px] text-gray-500 tabular-nums">{remaining}</span>
      </div>
    );
  }

  // Full variant
  return (
    <div className={`space-y-1 ${className}`}>
      <div className="flex items-center justify-between text-xs">
        <span className="text-gray-400 capitalize">{model}</span>
        <span className="text-gray-500 tabular-nums">
          {current}/{limit}
        </span>
      </div>
      <div className="h-1.5 bg-[#3a3a3a] rounded-full overflow-hidden">
        <div
          className={`h-full transition-all duration-300 ${getColor()}`}
          style={{ width: `${Math.min(100, percentage)}%` }}
        />
      </div>
    </div>
  );
}

/**
 * Usage dashboard showing all model usage
 */
export function UsageDashboard({ compact = false }: { compact?: boolean }) {
  const { tier, usage } = useSubscriptionStore();
  const limits = TIER_LIMITS[tier];

  const models: { key: ModelType; label: string }[] = [
    { key: 'haiku', label: 'Haiku' },
    { key: 'sonnet', label: 'Sonnet' },
    { key: 'gpt52', label: 'GPT-5.2' },
    { key: 'gpt5mini', label: 'GPT-5 Mini' },
    { key: 'gpt5nano', label: 'GPT-5 Nano' },
  ];

  if (compact) {
    return (
      <div className="flex items-center gap-3">
        {models.map(({ key, label }) => {
          const limit = limits[key];
          if (limit === 0) return null;
          return (
            <div key={key} className="flex items-center gap-1.5">
              <span className="text-[10px] text-gray-500">{label}</span>
              <UsageMeter model={key} variant="compact" />
            </div>
          );
        })}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {models.map(({ key, label }) => {
        const limit = limits[key];
        if (limit === 0) {
          return (
            <div key={key} className="flex items-center justify-between text-xs">
              <span className="text-gray-500">{label}</span>
              <span className="text-gray-600">Pro only</span>
            </div>
          );
        }
        return <UsageMeter key={key} model={key} variant="full" />;
      })}

      {/* Extended thinking */}
      <div className="flex items-center justify-between text-xs pt-2 border-t border-[#3a3a3a]">
        <span className="text-gray-400">Extended Thinking</span>
        {tier === 'pro' ? (
          <span className="text-gray-500 tabular-nums">
            {usage.extendedThinkingRequests}/{limits.extendedThinking}
          </span>
        ) : (
          <span className="text-gray-600">Pro only</span>
        )}
      </div>
    </div>
  );
}
