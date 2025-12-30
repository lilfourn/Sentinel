import { useState, useRef, useEffect, KeyboardEvent } from 'react';
import { ArrowUp, StopCircle, Plus, ChevronDown } from 'lucide-react';
import { useChatStore, type ChatModel } from '../../stores/chat-store';

interface ChatInputProps {
  onOpenMention: () => void;
}

const MODEL_OPTIONS: { value: ChatModel; label: string }[] = [
  { value: 'claude-sonnet-4-5', label: 'Sonnet 4.5' },
  { value: 'claude-haiku-4-5', label: 'Haiku 4.5' },
];

export function ChatInput({ onOpenMention }: ChatInputProps) {
  const [input, setInput] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const { sendMessage, abort, status, activeContext, model, setModel } = useChatStore();

  const isProcessing = status === 'thinking' || status === 'streaming';

  // Auto-resize textarea
  useEffect(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = 'auto';
      textarea.style.height = `${Math.min(textarea.scrollHeight, 120)}px`;
    }
  }, [input]);

  const handleSend = async () => {
    if (!input.trim() || isProcessing) return;

    const message = input;
    setInput('');
    await sendMessage(message);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // Submit on Enter (without Shift)
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
      return;
    }

    // Open mention on @
    if (e.key === '@') {
      // Let the @ be typed first, then open mention
      setTimeout(() => onOpenMention(), 0);
    }
  };

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = e.target.value;
    setInput(value);

    // Check for @ trigger
    const lastChar = value.slice(-1);
    if (lastChar === '@') {
      onOpenMention();
    }
  };

  return (
    <div className="p-3">
      {/* Unified prompt container */}
      <div className="rounded-xl bg-[#2a2a2a] border border-white/10 overflow-hidden">
        {/* Textarea area */}
        <div className="px-4 pt-3 pb-2">
          <textarea
            ref={textareaRef}
            value={input}
            onChange={handleChange}
            onKeyDown={handleKeyDown}
            placeholder={
              activeContext.length > 0
                ? 'Ask about the selected files...'
                : 'How can I help you today?'
            }
            className="w-full resize-none bg-transparent text-sm text-gray-100 placeholder-gray-500 focus:outline-none min-h-[24px] max-h-[120px]"
            rows={1}
            disabled={isProcessing}
          />
        </div>

        {/* Bottom toolbar */}
        <div className="flex items-center justify-between px-3 pb-3">
          {/* Left side buttons */}
          <div className="flex items-center gap-1">
            {/* Add context button */}
            <button
              onClick={onOpenMention}
              className="p-2 rounded-lg hover:bg-white/10 text-gray-400 hover:text-gray-200 transition-colors"
              title="Add file or folder context (@)"
            >
              <Plus size={18} />
            </button>
          </div>

          {/* Right side - Model selector + Send */}
          <div className="flex items-center gap-2">
            {/* Model selector */}
            <div className="relative">
              <select
                value={model}
                onChange={(e) => setModel(e.target.value as ChatModel)}
                className="appearance-none text-xs text-gray-400 bg-transparent pr-5 pl-2 py-1 focus:outline-none cursor-pointer hover:text-gray-200 transition-colors"
                disabled={isProcessing}
              >
                {MODEL_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value} className="bg-[#2a2a2a] text-gray-200">
                    {opt.label}
                  </option>
                ))}
              </select>
              <ChevronDown size={12} className="absolute right-0 top-1/2 -translate-y-1/2 text-gray-500 pointer-events-none" />
            </div>

            {/* Send/Stop button */}
            {isProcessing ? (
              <button
                onClick={abort}
                className="p-2 rounded-lg bg-red-500/80 hover:bg-red-500 text-white transition-colors"
                title="Stop generation"
              >
                <StopCircle size={18} />
              </button>
            ) : (
              <button
                onClick={handleSend}
                disabled={!input.trim()}
                className="p-2 rounded-lg bg-orange-600/80 hover:bg-orange-600 disabled:bg-gray-700 disabled:text-gray-500 disabled:cursor-not-allowed text-white transition-colors"
                title="Send message"
              >
                <ArrowUp size={18} />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
