import { useSubscriptionStore, type SubscriptionTier } from '../../stores/subscription-store';

interface PlanBadgeProps {
  tier?: SubscriptionTier;
  size?: 'sm' | 'md';
  className?: string;
}

/**
 * Badge showing current subscription tier
 */
export function PlanBadge({ tier: tierProp, size = 'sm', className = '' }: PlanBadgeProps) {
  const { tier: storeTier } = useSubscriptionStore();
  const tier = tierProp ?? storeTier;

  const sizeClasses = {
    sm: 'px-1.5 py-0.5 text-[10px]',
    md: 'px-2 py-1 text-xs',
  };

  if (tier === 'pro') {
    return (
      <span
        className={`inline-flex items-center gap-1 font-medium rounded
                    bg-gradient-to-r from-orange-500/20 to-purple-500/20
                    text-orange-400 border border-orange-500/30
                    ${sizeClasses[size]} ${className}`}
      >
        <SparklesIcon className={size === 'sm' ? 'w-2.5 h-2.5' : 'w-3 h-3'} />
        PRO
      </span>
    );
  }

  return (
    <span
      className={`inline-flex items-center font-medium rounded
                  bg-[#3a3a3a] text-gray-400 border border-[#4a4a4a]
                  ${sizeClasses[size]} ${className}`}
    >
      FREE
    </span>
  );
}

function SparklesIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      fill="currentColor"
      viewBox="0 0 20 20"
    >
      <path
        fillRule="evenodd"
        d="M5 2a1 1 0 011 1v1h1a1 1 0 010 2H6v1a1 1 0 01-2 0V6H3a1 1 0 010-2h1V3a1 1 0 011-1zm0 10a1 1 0 011 1v1h1a1 1 0 110 2H6v1a1 1 0 11-2 0v-1H3a1 1 0 110-2h1v-1a1 1 0 011-1zM12 2a1 1 0 01.967.744L14.146 7.2 17.5 9.134a1 1 0 010 1.732l-3.354 1.935-1.18 4.455a1 1 0 01-1.933 0L9.854 12.8 6.5 10.866a1 1 0 010-1.732l3.354-1.935 1.18-4.455A1 1 0 0112 2z"
        clipRule="evenodd"
      />
    </svg>
  );
}
