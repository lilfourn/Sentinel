import { useState, useRef, useEffect, KeyboardEvent, useCallback } from 'react';
import { ArrowUp, StopCircle, Plus, ChevronDown, Brain, X, Lock } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useChatStore, type ChatModel, type MentionItem, getContextStrategy } from '../../stores/chat-store';
import { useNavigationStore } from '../../stores/navigation-store';
import { useSubscriptionStore, type ModelType, chatModelToSubscriptionModel } from '../../stores/subscription-store';
import { InlineMentionDropdown } from './InlineMentionDropdown';
import { UsageMeter } from '../subscription/UsageMeter';
import { UpgradeBadge } from '../subscription/UpgradePrompt';

interface ModelOption {
  value: ChatModel;
  label: string;
  modelType: ModelType;
  provider: 'anthropic' | 'openai';
}

const MODEL_OPTIONS: ModelOption[] = [
  { value: 'claude-sonnet-4-5', label: 'Sonnet 4.5', modelType: 'sonnet', provider: 'anthropic' },
  { value: 'claude-haiku-4-5', label: 'Haiku 4.5', modelType: 'haiku', provider: 'anthropic' },
  { value: 'gpt-5.2-2025-12-11', label: 'GPT-5.2', modelType: 'gpt52', provider: 'openai' },
  { value: 'gpt-5-mini-2025-08-07', label: 'GPT-5 Mini', modelType: 'gpt5mini', provider: 'openai' },
  { value: 'gpt-5-nano-2025-08-07', label: 'GPT-5 Nano', modelType: 'gpt5nano', provider: 'openai' },
];

// Debounce search delay
const MENTION_SEARCH_DEBOUNCE_MS = 150;

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

  // Refs for debounce cleanup and search race condition prevention
  const debounceTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const searchSequenceRef = useRef(0);

  const currentPath = useNavigationStore((s) => s.currentPath);

  const {
    sendMessage,
    abort,
    status,
    activeContext,
    removeContext,
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

  // Subscription state
  const { tier, canUseModel, canUseExtendedThinking, openCheckout } = useSubscriptionStore();
  const currentModelType = chatModelToSubscriptionModel(model);
  const [showModelDropdown, setShowModelDropdown] = useState(false);

  // Check if current model supports extended thinking (GPT models don't)
  const currentModelOption = MODEL_OPTIONS.find((opt) => opt.value === model);
  const modelSupportsThinking = currentModelOption?.provider === 'anthropic';

  // Cleanup debounce timeout on unmount
  useEffect(() => {
    return () => {
      if (debounceTimeoutRef.current) {
        clearTimeout(debounceTimeoutRef.current);
        debounceTimeoutRef.current = null;
      }
    };
  }, []);

  // Debounced search function with race condition protection
  // Searches from home directory by default for global file access
  const debouncedSearch = useCallback(
    (query: string, currentDirectory: string) => {
      // Clear any pending debounce
      if (debounceTimeoutRef.current) {
        clearTimeout(debounceTimeoutRef.current);
      }

      debounceTimeoutRef.current = setTimeout(async () => {
        // Increment sequence to track this specific search
        const currentSequence = ++searchSequenceRef.current;

        try {
          setMentionLoading(true);

          // Get home directory for primary search (command is get_home_directory)
          const homeDir = await invoke<string>('get_home_directory').catch(() => null);
          const primaryDir = homeDir || currentDirectory;

          // Search home directory first with recursive search enabled
          let results = await invoke<MentionItem[]>('list_files_for_mention', {
            directory: primaryDir,
            query: query || null,
            max_results: 20,
            recursive: true,
          });

          // Check if this search is still current (no newer search started)
          if (currentSequence !== searchSequenceRef.current) {
            return; // Stale result, discard
          }

          // Also search current directory if different from home (for quick access to browsed folder)
          if (currentDirectory !== primaryDir) {
            const currentDirResults = await invoke<MentionItem[]>('list_files_for_mention', {
              directory: currentDirectory,
              query: query || null,
              max_results: 10,
              recursive: true,
            });

            // Check again after second async operation
            if (currentSequence !== searchSequenceRef.current) {
              return; // Stale result, discard
            }

            // Dedupe and merge - current directory results first for relevance
            const seen = new Set(currentDirResults.map((r) => r.path));
            results = [...currentDirResults, ...results.filter((r) => !seen.has(r.path))];
          }

          setMentionResults(results.slice(0, 15));
          resetMentionSelection();
        } catch (err) {
          // Only log error if this search is still current
          if (currentSequence === searchSequenceRef.current) {
            console.error('Mention search failed:', err);
            setMentionResults([]);
          }
        }
      }, MENTION_SEARCH_DEBOUNCE_MS);
    },
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
      <div ref={containerRef} className="rounded-xl bg-[#2a2a2a] border border-white/10 relative">
        {/* Inline mention dropdown */}
        <InlineMentionDropdown anchorRef={containerRef} onSelect={handleMentionSelect} />

        {/* Context chips - inline above textarea */}
        {activeContext.length > 0 && (
          <div className="px-3 pt-3 pb-1 flex flex-wrap gap-1.5">
            {activeContext.map((item) => (
              <div
                key={item.id}
                className="flex items-center gap-1.5 pl-2 pr-1 py-0.5 bg-white/[0.06] rounded text-[11px] text-gray-400 group"
              >
                <span className="truncate max-w-[120px]">{item.name}</span>
                <button
                  onClick={() => removeContext(item.id)}
                  className="p-0.5 rounded hover:bg-white/10 text-gray-500 hover:text-gray-300 opacity-60 group-hover:opacity-100 transition-opacity"
                  aria-label={`Remove ${item.name}`}
                >
                  <X size={10} />
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Textarea area */}
        <div className={`px-4 ${activeContext.length > 0 ? 'pt-1' : 'pt-3'} pb-2`}>
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
            aria-label="Chat message input"
            aria-describedby={activeContext.length > 0 ? 'context-hint' : undefined}
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
              aria-label="Add file or folder context"
            >
              <Plus size={18} aria-hidden="true" />
            </button>

            {/* Extended thinking toggle */}
            <button
              onClick={() => {
                if (!modelSupportsThinking) {
                  // GPT models don't support extended thinking - do nothing
                  return;
                }
                if (canUseExtendedThinking()) {
                  setExtendedThinking(!extendedThinking);
                } else {
                  openCheckout();
                }
              }}
              className={`p-2 rounded-lg transition-colors relative ${
                !modelSupportsThinking
                  ? 'hover:bg-white/10 text-gray-600 cursor-not-allowed'
                  : extendedThinking && canUseExtendedThinking()
                    ? 'bg-purple-500/20 text-purple-400 hover:bg-purple-500/30'
                    : canUseExtendedThinking()
                      ? 'hover:bg-white/10 text-gray-400 hover:text-gray-200'
                      : 'hover:bg-white/10 text-gray-500'
              }`}
              title={
                !modelSupportsThinking
                  ? 'Extended thinking is not available for GPT models'
                  : !canUseExtendedThinking()
                    ? 'Extended thinking requires Pro'
                    : extendedThinking
                      ? 'Extended thinking enabled'
                      : 'Extended thinking disabled'
              }
              aria-label={
                !modelSupportsThinking
                  ? 'Extended thinking not available for this model'
                  : !canUseExtendedThinking()
                    ? 'Upgrade to Pro for extended thinking'
                    : extendedThinking
                      ? 'Disable extended thinking'
                      : 'Enable extended thinking'
              }
              aria-pressed={modelSupportsThinking && extendedThinking}
              disabled={isProcessing || !modelSupportsThinking}
            >
              <Brain size={18} aria-hidden="true" />
              {!modelSupportsThinking ? (
                <span className="absolute -top-0.5 -right-0.5 text-gray-500 text-[8px]">×</span>
              ) : !canUseExtendedThinking() && (
                <Lock size={8} className="absolute -top-0.5 -right-0.5 text-orange-400" />
              )}
            </button>
          </div>

          {/* Right side - Usage + Model selector + Send */}
          <div className="flex items-center gap-2">
            {/* Usage meter for current model */}
            <UsageMeter model={currentModelType} variant="compact" />

            {/* Custom model selector with tier gating */}
            <div className="relative">
              <button
                onClick={() => !isProcessing && setShowModelDropdown(!showModelDropdown)}
                className="flex items-center gap-1 text-xs text-gray-400 bg-transparent px-2 py-1 rounded hover:bg-white/5 hover:text-gray-200 transition-colors"
                disabled={isProcessing}
                aria-label="Select AI model"
                aria-haspopup="listbox"
                aria-expanded={showModelDropdown}
              >
                {MODEL_OPTIONS.find((opt) => opt.value === model)?.label}
                {!canUseModel(currentModelType) && <Lock size={10} className="text-orange-400" />}
                <ChevronDown size={12} className="text-gray-500" aria-hidden="true" />
              </button>

              {/* Dropdown menu */}
              {showModelDropdown && (
                <>
                  {/* Backdrop to close dropdown */}
                  <div
                    className="fixed inset-0 z-40"
                    onClick={() => setShowModelDropdown(false)}
                  />
                  <div className="absolute right-0 bottom-full mb-1 w-48 bg-[#2a2a2a] border border-white/10 rounded-lg shadow-xl z-50 overflow-hidden">
                    {MODEL_OPTIONS.map((opt) => {
                      const isAvailable = canUseModel(opt.modelType);
                      return (
                        <button
                          key={opt.value}
                          onClick={() => {
                            if (isAvailable) {
                              setModel(opt.value);
                              setShowModelDropdown(false);
                            } else {
                              openCheckout();
                            }
                          }}
                          className={`w-full flex items-center justify-between px-3 py-2 text-xs transition-colors ${
                            model === opt.value
                              ? 'bg-white/10 text-gray-100'
                              : isAvailable
                                ? 'text-gray-300 hover:bg-white/5'
                                : 'text-gray-500'
                          }`}
                        >
                          <span className="flex items-center gap-2">
                            {opt.label}
                            {!isAvailable && (
                              <span className="text-[10px] text-orange-400 flex items-center gap-0.5">
                                <Lock size={10} />
                                PRO
                              </span>
                            )}
                          </span>
                          {isAvailable && (
                            <UsageMeter model={opt.modelType} variant="compact" />
                          )}
                        </button>
                      );
                    })}
                    {tier === 'free' && (
                      <div className="px-3 py-2 border-t border-white/10">
                        <UpgradeBadge />
                      </div>
                    )}
                  </div>
                </>
              )}
            </div>

            {/* Send/Stop button */}
            {isProcessing ? (
              <button
                onClick={abort}
                className="p-2 rounded-lg bg-red-500/80 hover:bg-red-500 text-white transition-colors"
                title="Stop generation"
                aria-label="Stop generation"
              >
                <StopCircle size={18} aria-hidden="true" />
              </button>
            ) : (
              <button
                onClick={handleSend}
                disabled={!input.trim()}
                className="p-2 rounded-lg bg-orange-600/80 hover:bg-orange-600 disabled:bg-gray-700 disabled:text-gray-500 disabled:cursor-not-allowed text-white transition-colors"
                title="Send message"
                aria-label="Send message"
              >
                <ArrowUp size={18} aria-hidden="true" />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
