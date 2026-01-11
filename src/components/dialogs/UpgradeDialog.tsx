import { useEffect, useCallback, useState } from 'react';
import { createPortal } from 'react-dom';
import { X, Check } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useSubscriptionStore, TIER_LIMITS, PRO_PRICE } from '../../stores/subscription-store';

interface UpgradeDialogProps {
  isOpen: boolean;
  onClose: () => void;
  highlightFeature?: 'sonnet' | 'extendedThinking' | 'gpt52' | 'gpt5mini' | 'gpt5nano';
}

const features = [
  {
    id: 'sonnet',
    title: 'Sonnet 4.5',
    pro: `${TIER_LIMITS.pro.sonnet}/day`,
  },
  {
    id: 'extendedThinking',
    title: 'Extended Thinking',
    pro: `${TIER_LIMITS.pro.extendedThinking}/day`,
  },
  {
    id: 'haiku',
    title: 'Haiku 4.5',
    pro: `${TIER_LIMITS.pro.haiku}/day`,
  },
  {
    id: 'gpt52',
    title: 'GPT-5.2',
    pro: `${TIER_LIMITS.pro.gpt52}/day`,
  },
  {
    id: 'gpt5mini',
    title: 'GPT-5 Mini',
    pro: `${TIER_LIMITS.pro.gpt5mini}/day`,
  },
  {
    id: 'gpt5nano',
    title: 'GPT-5 Nano',
    pro: `${TIER_LIMITS.pro.gpt5nano}/day`,
  },
];

export function UpgradeDialog({ isOpen, onClose, highlightFeature }: UpgradeDialogProps) {
  const { openCheckout } = useSubscriptionStore();
  const [isLoading, setIsLoading] = useState(false);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!isOpen) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }, [isOpen, onClose]);

  useEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  const handleUpgrade = async () => {
    setIsLoading(true);
    try {
      await openCheckout();
      onClose();
    } finally {
      setIsLoading(false);
    }
  };

  if (!isOpen) return null;

  return createPortal(
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className={cn(
          "w-full max-w-xs mx-4",
          "bg-white/90 dark:bg-neutral-900/90",
          "backdrop-blur-xl",
          "rounded-xl shadow-2xl",
          "border border-white/20 dark:border-white/10",
          "animate-in zoom-in-95 fade-in duration-150"
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between p-4 pb-2">
          <div>
            <h2 className="text-base font-semibold text-gray-900 dark:text-white">
              Sentinel Pro
            </h2>
            <div className="flex items-baseline gap-1 mt-0.5">
              <span className="text-2xl font-bold text-gray-900 dark:text-white">${PRO_PRICE}</span>
              <span className="text-sm text-gray-500 dark:text-gray-400">/mo</span>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 -mr-1 rounded-lg hover:bg-black/5 dark:hover:bg-white/10 text-gray-400 transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        {/* Features */}
        <div className="px-4 py-3 space-y-2">
          {features.map((feature) => {
            const isHighlighted = highlightFeature === feature.id;
            return (
              <div
                key={feature.id}
                className={cn(
                  "flex items-center justify-between py-1.5",
                  isHighlighted && "text-orange-600 dark:text-orange-400"
                )}
              >
                <span className={cn(
                  "text-sm",
                  isHighlighted
                    ? "text-orange-600 dark:text-orange-400 font-medium"
                    : "text-gray-600 dark:text-gray-300"
                )}>
                  {feature.title}
                </span>
                <span className="flex items-center gap-1 text-sm font-medium text-orange-600 dark:text-orange-400">
                  <Check size={14} strokeWidth={2.5} />
                  {feature.pro}
                </span>
              </div>
            );
          })}
        </div>

        {/* Actions */}
        <div className="p-4 pt-2 space-y-2">
          <button
            onClick={handleUpgrade}
            disabled={isLoading}
            className={cn(
              "w-full py-2.5 text-sm font-medium rounded-lg transition-colors",
              "bg-orange-500 hover:bg-orange-600 text-white",
              "disabled:opacity-50 disabled:cursor-not-allowed"
            )}
          >
            {isLoading ? 'Opening...' : 'Upgrade'}
          </button>
          <button
            onClick={onClose}
            className="w-full py-2 text-sm text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 transition-colors"
          >
            Not now
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}
