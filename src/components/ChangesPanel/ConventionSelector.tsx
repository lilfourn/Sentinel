import { SkipForward, Wand2 } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { NamingConvention } from '../../types/naming-convention';

interface ConventionSelectorProps {
  conventions: NamingConvention[];
  onSelect: (convention: NamingConvention) => void;
  onSkip: () => void;
}

export function ConventionSelector({ conventions, onSelect, onSkip }: ConventionSelectorProps) {
  return (
    <div className="p-4 space-y-4">
      {/* Section Header */}
      <div className="flex items-center gap-2">
        <div className="w-6 h-6 rounded-md bg-purple-500/20 flex items-center justify-center">
          <Wand2 size={12} className="text-purple-400" />
        </div>
        <span className="text-sm font-medium text-gray-200">Choose naming style</span>
      </div>

      {/* Convention Option Cards - Simple */}
      <div className="space-y-2">
        {conventions.map((conv) => (
          <button
            key={conv.id}
            onClick={() => onSelect(conv)}
            className={cn(
              'w-full px-4 py-3 rounded-lg border text-left transition-all',
              'border-white/10 bg-white/[0.02]',
              'hover:bg-orange-500/10 hover:border-orange-500/30',
              'focus:outline-none focus:ring-2 focus:ring-orange-500/50'
            )}
          >
            <p className="text-sm font-medium text-gray-200 mb-1">
              {conv.name}
            </p>
            <code className="text-xs text-orange-400/80 font-mono truncate block">
              {conv.example}
            </code>
          </button>
        ))}
      </div>

      {/* Skip Button */}
      <button
        onClick={onSkip}
        className={cn(
          'w-full py-2.5 rounded-lg border transition-all',
          'border-white/5 bg-transparent',
          'hover:bg-white/[0.03] hover:border-white/10',
          'focus:outline-none focus:ring-2 focus:ring-white/20',
          'text-xs text-gray-500 hover:text-gray-400',
          'flex items-center justify-center gap-2'
        )}
      >
        <SkipForward size={12} />
        Skip renaming
      </button>
    </div>
  );
}
