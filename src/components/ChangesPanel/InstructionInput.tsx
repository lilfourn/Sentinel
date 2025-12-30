import { useState } from 'react';
import { MessageSquare, Sparkles, Loader2 } from 'lucide-react';
import { cn } from '../../lib/utils';

interface InstructionInputProps {
  instruction: string;
  onInstructionChange: (value: string) => void;
  onSubmit: () => void;
  isDisabled: boolean;
  folderName: string;
}

export function InstructionInput({
  instruction,
  onInstructionChange,
  onSubmit,
  isDisabled,
  folderName,
}: InstructionInputProps) {
  const [isFocused, setIsFocused] = useState(false);
  const isValid = instruction.trim().length > 0;

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Submit on Cmd/Ctrl + Enter
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter' && isValid && !isDisabled) {
      e.preventDefault();
      onSubmit();
    }
  };

  return (
    <div className="p-4 space-y-4">
      {/* Section Header */}
      <div className="flex items-center gap-2">
        <div className="w-6 h-6 rounded-md bg-purple-500/20 flex items-center justify-center">
          <MessageSquare size={12} className="text-purple-400" />
        </div>
        <div className="flex-1">
          <span className="text-sm font-medium text-gray-200">Organization Instructions</span>
          <p className="text-xs text-gray-500">How should we organize {folderName}?</p>
        </div>
      </div>

      {/* Instruction Textarea */}
      <div
        className={cn(
          'relative rounded-lg border transition-all',
          isFocused
            ? 'border-orange-500/50 bg-orange-500/5'
            : 'border-white/10 bg-white/[0.02]'
        )}
      >
        <textarea
          value={instruction}
          onChange={(e) => onInstructionChange(e.target.value)}
          onFocus={() => setIsFocused(true)}
          onBlur={() => setIsFocused(false)}
          onKeyDown={handleKeyDown}
          disabled={isDisabled}
          placeholder="e.g., 'Group invoices by Vendor, then by Year' or 'Organize photos by date taken'"
          className={cn(
            'w-full px-3 py-3 bg-transparent text-sm text-gray-200',
            'placeholder:text-gray-600 resize-none',
            'focus:outline-none',
            'disabled:opacity-50 disabled:cursor-not-allowed'
          )}
          rows={3}
        />

        {/* Character hint */}
        {instruction.length > 0 && (
          <div className="absolute bottom-2 right-2 text-xs text-gray-600">
            {instruction.length} chars
          </div>
        )}
      </div>

      {/* Quick suggestions */}
      <div className="flex flex-wrap gap-1.5">
        {[
          'Group by file type',
          'Organize by date',
          'Sort by project',
        ].map((suggestion) => (
          <button
            key={suggestion}
            onClick={() => onInstructionChange(suggestion)}
            disabled={isDisabled}
            className={cn(
              'px-2 py-1 rounded text-xs transition-all',
              'bg-white/[0.03] border border-white/5',
              'hover:bg-white/[0.06] hover:border-white/10',
              'text-gray-500 hover:text-gray-400',
              'disabled:opacity-50 disabled:cursor-not-allowed'
            )}
          >
            {suggestion}
          </button>
        ))}
      </div>

      {/* Submit Button */}
      <button
        onClick={onSubmit}
        disabled={!isValid || isDisabled}
        className={cn(
          'w-full py-2.5 rounded-lg border transition-all',
          'flex items-center justify-center gap-2',
          'text-sm font-medium',
          isValid && !isDisabled
            ? 'bg-gradient-to-r from-orange-500 to-orange-600 border-orange-500/50 text-white hover:from-orange-400 hover:to-orange-500'
            : 'bg-white/[0.02] border-white/10 text-gray-600 cursor-not-allowed'
        )}
      >
        {isDisabled ? (
          <>
            <Loader2 size={14} className="animate-spin" />
            Generating plan...
          </>
        ) : (
          <>
            <Sparkles size={14} />
            Generate Plan
          </>
        )}
      </button>

      {/* Keyboard hint */}
      <p className="text-xs text-gray-600 text-center">
        Press <kbd className="px-1 py-0.5 bg-white/5 rounded text-gray-500">⌘</kbd> + <kbd className="px-1 py-0.5 bg-white/5 rounded text-gray-500">Enter</kbd> to submit
      </p>
    </div>
  );
}
