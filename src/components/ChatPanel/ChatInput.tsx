import { useState, useRef, useEffect, KeyboardEvent, useMemo, useCallback } from 'react';
import { ArrowUp, StopCircle, Plus, ChevronDown, Brain } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useChatStore, type ChatModel, type MentionItem, getContextStrategy } from '../../stores/chat-store';
import { useNavigationStore } from '../../stores/navigation-store';
import { InlineMentionDropdown } from './InlineMentionDropdown';

const MODEL_OPTIONS: { value: ChatModel; label: string }[] = [
  { value: 'claude-sonnet-4-5', label: 'Sonnet 4.5' },
  { value: 'claude-haiku-4-5', label: 'Haiku 4.5' },
  { value: 'claude-opus-4-5', label: 'Opus 4.5' },
];

// Debounce helper
function debounce<T extends (...args: Parameters<T>) => void>(
  fn: T,
  delay: number
): (...args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout>;
  return (...args: Parameters<T>) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  };
}

// Extract mention query from text at cursor position
function extractMentionQuery(text: string, cursorPos: number): { query: string; startIndex: number } | null {
  const textBeforeCursor = text.slice(0, cursorPos);
  const lastAtIndex = textBeforeCursor.lastIndexOf('@');

  if (lastAtIndex === -1) return null;

  // Check @ is at start or after whitespace
  const charBefore = lastAtIndex > 0 ? text[lastAtIndex - 1] : ' ';
  if (!/\s/.test(charBefore) && lastAtIndex !== 0) return null;

  const query = textBeforeCursor.slice(lastAtIndex + 1);
  // Space after @ closes the mention
  if (/\s/.test(query)) return null;

  return { query, startIndex: lastAtIndex };
}

export function ChatInput() {
  const [input, setInput] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const currentPath = useNavigationStore((s) => s.currentPath);

  const {
    sendMessage,
    abort,
    status,
    activeContext,
    model,
    setModel,
    extendedThinking,
    setExtendedThinking,
    addContext,
    // Mention state
    isMentionOpen,
    mentionQuery,
    mentionResults,
    selectedMentionIndex,
    openMention,
    closeMention,
    setMentionQuery,
    setMentionResults,
    setMentionLoading,
    selectNextMention,
    selectPrevMention,
    resetMentionSelection,
  } = useChatStore();

  const isProcessing = status === 'thinking' || status === 'streaming';

  // Debounced search function
  const debouncedSearch = useMemo(
    () =>
      debounce(async (query: string, directory: string) => {
        try {
          setMentionLoading(true);

          // Search current directory
          let results = await invoke<MentionItem[]>('list_files_for_mention', {
            directory,
            query: query || null,
            limit: 15,
          });

          // If few results, also search home directory
          const homeDir = await invoke<string>('get_home_dir').catch(() => null);
          if (results.length < 5 && homeDir && directory !== homeDir) {
            const homeResults = await invoke<MentionItem[]>('list_files_for_mention', {
              directory: homeDir,
              query: query || null,
              limit: 10,
            });
            // Dedupe and merge
            const seen = new Set(results.map((r) => r.path));
            results = [...results, ...homeResults.filter((r) => !seen.has(r.path))];
          }

          setMentionResults(results.slice(0, 15));
          resetMentionSelection();
        } catch (err) {
          console.error('Mention search failed:', err);
          setMentionResults([]);
        }
      }, 150),
    [setMentionLoading, setMentionResults, resetMentionSelection]
  );

  // Trigger search when mention query changes
  useEffect(() => {
    if (isMentionOpen && currentPath) {
      debouncedSearch(mentionQuery, currentPath);
    }
  }, [mentionQuery, isMentionOpen, currentPath, debouncedSearch]);

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
    closeMention();
    await sendMessage(message);
  };

  // Handle mention selection
  const handleMentionSelect = useCallback(
    (item: MentionItem) => {
      const startIndex = useChatStore.getState().mentionStartIndex;
      const query = useChatStore.getState().mentionQuery;

      // Remove @query from input
      const before = input.slice(0, startIndex);
      const after = input.slice(startIndex + query.length + 1); // +1 for @
      setInput(before + after);

      // Add to context
      addContext({
        type: item.isDirectory ? 'folder' : 'file',
        path: item.path,
        name: item.name,
        strategy: getContextStrategy(item.isDirectory ? 'folder' : 'file'),
      });

      closeMention();

      // Refocus textarea
      setTimeout(() => textareaRef.current?.focus(), 0);
    },
    [input, addContext, closeMention]
  );

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // When mention dropdown is open, intercept navigation keys
    if (isMentionOpen) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        selectNextMention();
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        selectPrevMention();
        return;
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        if (mentionResults.length > 0) {
          handleMentionSelect(mentionResults[selectedMentionIndex]);
        }
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        closeMention();
        return;
      }
    }

    // Submit on Enter (without Shift) when dropdown is closed
    if (e.key === 'Enter' && !e.shiftKey && !isMentionOpen) {
      e.preventDefault();
      handleSend();
      return;
    }
  };

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = e.target.value;
    const cursorPos = e.target.selectionStart;
    setInput(value);

    // Check for @ mention
    const mention = extractMentionQuery(value, cursorPos);

    if (mention) {
      if (!isMentionOpen) {
        openMention(mention.startIndex);
      }
      setMentionQuery(mention.query);
    } else if (isMentionOpen) {
      closeMention();
    }
  };

  const handlePlusClick = () => {
    // Insert @ at cursor position and open mention
    const textarea = textareaRef.current;
    if (!textarea) return;

    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const newValue = input.slice(0, start) + '@' + input.slice(end);
    setInput(newValue);
    openMention(start);
    setMentionQuery('');

    // Set cursor after @
    setTimeout(() => {
      textarea.selectionStart = textarea.selectionEnd = start + 1;
      textarea.focus();
    }, 0);
  };

  return (
    <div className="p-3">
      {/* Unified prompt container */}
      <div ref={containerRef} className="rounded-xl bg-[#2a2a2a] border border-white/10 overflow-hidden relative">
        {/* Inline mention dropdown */}
        <InlineMentionDropdown anchorRef={containerRef} onSelect={handleMentionSelect} />

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
              onClick={handlePlusClick}
              className="p-2 rounded-lg hover:bg-white/10 text-gray-400 hover:text-gray-200 transition-colors"
              title="Add file or folder context (@)"
            >
              <Plus size={18} />
            </button>

            {/* Extended thinking toggle */}
            <button
              onClick={() => setExtendedThinking(!extendedThinking)}
              className={`p-2 rounded-lg transition-colors ${
                extendedThinking
                  ? 'bg-purple-500/20 text-purple-400 hover:bg-purple-500/30'
                  : 'hover:bg-white/10 text-gray-400 hover:text-gray-200'
              }`}
              title={extendedThinking ? 'Extended thinking enabled' : 'Extended thinking disabled'}
              disabled={isProcessing}
            >
              <Brain size={18} />
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
