import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { invoke } from '@tauri-apps/api/core';
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
import { useSubscriptionStore, chatModelToSubscriptionModel } from './subscription-store';
import { showInfo, showError } from './toast-store';

/** Auto-recovery timeout for error state (only if user hasn't interacted) */
const ERROR_AUTO_RECOVERY_MS = 30000;

/** Maximum number of files that can be attached to a single chat message */
export const MAX_CONTEXT_ITEMS = 10;

// ============================================================================
// TYPES
// ============================================================================

export interface ContextItem {
  id: string;
  type: 'file' | 'folder' | 'image';
  path: string;
  name: string;
  /** Context injection strategy */
  strategy: 'hologram' | 'read' | 'vision';
  /** Size in bytes (for display) */
  size?: number;
  /** MIME type for images */
  mimeType?: string;
}

export interface ThoughtStep {
  id: string;
  tool: string;
  input: string;
  output?: string;
  status: 'pending' | 'running' | 'complete' | 'error';
  timestamp: number;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
  thoughts?: ThoughtStep[];
  /** Attached context items (files/folders) for this message */
  contextItems?: ContextItem[];
  isStreaming?: boolean;
  /** Extended thinking content (Claude's internal reasoning) */
  thinking?: string;
  isThinking?: boolean;
}

export type ChatModel =
  | 'claude-haiku-4-5'
  | 'claude-sonnet-4-5'
  | 'gpt-5.2-2025-12-11'
  | 'gpt-5-mini-2025-08-07'
  | 'gpt-5-nano-2025-08-07';
export type ChatStatus = 'idle' | 'thinking' | 'streaming' | 'error';

/** File/folder item for mention autocomplete */
export interface MentionItem {
  path: string;
  name: string;
  isDirectory: boolean;
}

// ============================================================================
// STATE INTERFACE
// ============================================================================

interface ChatState {
  // Panel state
  isOpen: boolean;
  width: number;

  // Conversation
  messages: ChatMessage[];
  activeContext: ContextItem[];

  // Model & status
  model: ChatModel;
  status: ChatStatus;
  error: string | null;
  extendedThinking: boolean;

  // Streaming
  currentStreamId: string | null;
  /** Cleanup function for active stream listeners - stored in state for proper lifecycle */
  _activeCleanup: (() => void) | null;
  /** Timestamp of last user interaction (for smart error auto-recovery) */
  _lastInteraction: number;

  // Mention autocomplete
  isMentionOpen: boolean;
  mentionQuery: string;
  mentionStartIndex: number;
  mentionResults: MentionItem[];
  selectedMentionIndex: number;
  isMentionLoading: boolean;
}

interface ChatActions {
  // Panel
  open: () => void;
  close: () => void;
  toggle: () => void;
  setWidth: (width: number) => void;

  // Context
  addContext: (item: Omit<ContextItem, 'id'>) => void;
  addContextBatch: (items: Omit<ContextItem, 'id'>[]) => void;
  removeContext: (id: string) => void;
  clearContext: () => void;

  // Model
  setModel: (model: ChatModel) => void;
  setExtendedThinking: (enabled: boolean) => void;

  // Messaging
  sendMessage: (text: string) => Promise<void>;
  abort: () => void;
  clearHistory: () => void;
  clearError: () => void;
  retryLastMessage: () => Promise<void>;

  // Mention autocomplete
  openMention: (startIndex: number) => void;
  closeMention: () => void;
  setMentionQuery: (query: string) => void;
  setMentionResults: (items: MentionItem[]) => void;
  setMentionLoading: (loading: boolean) => void;
  selectNextMention: () => void;
  selectPrevMention: () => void;
  resetMentionSelection: () => void;

  // Internal
  _addThought: (messageId: string, thought: ThoughtStep) => void;
  _updateThought: (messageId: string, thoughtId: string, update: Partial<ThoughtStep>) => void;
  _appendContent: (messageId: string, chunk: string) => void;
  _appendThinking: (messageId: string, chunk: string) => void;
  _finishThinking: (messageId: string, fullContent?: string) => void;
  _finishStream: (messageId: string) => void;
  _setError: (error: string | null) => void;
}

// ============================================================================
// STORE IMPLEMENTATION
// ============================================================================

export const useChatStore = create<ChatState & ChatActions>()(
  persist(
    (set, get) => ({
  // Initial state
  isOpen: false,
  width: 400,
  messages: [],
  activeContext: [],
  model: 'claude-sonnet-4-5',
  status: 'idle',
  error: null,
  extendedThinking: false, // Default to false - Pro users can enable it
  currentStreamId: null,
  _activeCleanup: null,
  _lastInteraction: Date.now(),
  isMentionOpen: false,
  mentionQuery: '',
  mentionStartIndex: -1,
  mentionResults: [],
  selectedMentionIndex: 0,
  isMentionLoading: false,

  // Panel actions
  open: () => set({ isOpen: true }),
  close: () => set({ isOpen: false }),
  toggle: () => set((state) => ({ isOpen: !state.isOpen })),
  setWidth: (width) => set({ width: Math.max(320, Math.min(600, width)) }),

  // Context actions
  addContext: (item) => {
    const { activeContext } = get();

    // Check if item already exists (will be replaced, not added)
    const isReplacement = activeContext.some(c => c.path === item.path);

    // Check limit only for new items
    if (!isReplacement && activeContext.length >= MAX_CONTEXT_ITEMS) {
      showInfo('Attachment limit reached', `Maximum ${MAX_CONTEXT_ITEMS} files can be attached per message`);
      return;
    }

    const newItem: ContextItem = {
      ...item,
      id: crypto.randomUUID(),
    };
    set((state) => ({
      // Replace if same path already exists
      activeContext: [...state.activeContext.filter(c => c.path !== item.path), newItem],
    }));
  },

  addContextBatch: (items) => {
    if (items.length === 0) return;

    const { activeContext } = get();

    // Calculate how many new items we can add
    const existingPaths = new Set(activeContext.map(c => c.path));
    const trulyNewItems = items.filter(item => !existingPaths.has(item.path));
    const replacementItems = items.filter(item => existingPaths.has(item.path));

    // Count current items that won't be replaced
    const remainingExisting = activeContext.filter(c => !items.some(item => item.path === c.path));
    const availableSlots = MAX_CONTEXT_ITEMS - remainingExisting.length - replacementItems.length;

    // Limit truly new items to available slots
    const itemsToAdd = trulyNewItems.slice(0, Math.max(0, availableSlots));
    const droppedCount = trulyNewItems.length - itemsToAdd.length;

    // Show notification if some items were dropped
    if (droppedCount > 0) {
      showInfo(
        'Attachment limit reached',
        `Added ${itemsToAdd.length + replacementItems.length} of ${items.length} files. Maximum ${MAX_CONTEXT_ITEMS} allowed.`
      );
    }

    // If nothing to add, just return
    if (itemsToAdd.length === 0 && replacementItems.length === 0) return;

    const allItemsToAdd = [...replacementItems, ...itemsToAdd];
    const newItems: ContextItem[] = allItemsToAdd.map((item) => ({
      ...item,
      id: crypto.randomUUID(),
    }));

    set((state) => {
      // Get paths of new items for deduplication
      const newPaths = new Set(newItems.map((i) => i.path));
      // Filter out existing items with same paths, then add new items
      const filtered = state.activeContext.filter((c) => !newPaths.has(c.path));
      return {
        activeContext: [...filtered, ...newItems],
      };
    });
  },

  removeContext: (id) => {
    set((state) => ({
      activeContext: state.activeContext.filter((c) => c.id !== id),
    }));
  },

  clearContext: () => set({ activeContext: [] }),

  // Model
  setModel: (model) => set({ model }),
  setExtendedThinking: (enabled) => set({ extendedThinking: enabled }),

  // Messaging
  sendMessage: async (text) => {
    const { activeContext, model, messages, extendedThinking, status } = get();

    // Concurrent stream guard - prevent multiple simultaneous streams
    if (status === 'thinking' || status === 'streaming') {
      console.warn('[ChatStore] Ignoring send - already processing');
      return;
    }

    if (!text.trim()) return;

    // Create user message with attached context items
    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: text,
      timestamp: Date.now(),
      contextItems: activeContext.length > 0 ? [...activeContext] : undefined,
    };

    // Create placeholder assistant message
    const assistantMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      thoughts: [],
      isStreaming: true,
    };

    // Track user interaction for smart error auto-recovery
    set({
      messages: [...messages, userMessage, assistantMessage],
      status: 'thinking',
      error: null,
      currentStreamId: assistantMessage.id,
      activeContext: [], // Clear context after attaching to message
      _lastInteraction: Date.now(),
    });

    // NOTE: No optimistic usage increment - only record usage on successful completion
    // This prevents double-charging when users retry failed requests
    const modelType = chatModelToSubscriptionModel(model);

    // IMPORTANT: Only use extendedThinking if user can actually use it
    // Free tier users may have stale localStorage state with extendedThinking=true
    // but they're not allowed to use it, so we force it to false
    const canUseThinking = useSubscriptionStore.getState().canUseExtendedThinking();
    const effectiveExtendedThinking = extendedThinking && canUseThinking;

    // Set up event listeners
    let unlistenToken: UnlistenFn | null = null;
    let unlistenThought: UnlistenFn | null = null;
    let unlistenThinking: UnlistenFn | null = null;
    let unlistenComplete: UnlistenFn | null = null;
    let unlistenError: UnlistenFn | null = null;
    let unlistenLimitError: UnlistenFn | null = null;
    let listenersActive = true;

    // Cleanup function - stored in state for proper lifecycle management
    const cleanupListeners = () => {
      if (!listenersActive) return;
      listenersActive = false;
      set({ _activeCleanup: null });
      unlistenToken?.();
      unlistenThinking?.();
      unlistenThought?.();
      unlistenComplete?.();
      unlistenError?.();
      unlistenLimitError?.();
    };

    // Store cleanup ref in state (not module-level) for proper React lifecycle
    set({ _activeCleanup: cleanupListeners });

    try {
      // CRITICAL: Register ALL event listeners BEFORE invoking the backend command
      // This prevents race conditions where events are emitted before listeners are ready
      const [tokenUn, thinkingUn, thoughtUn, completeUn, errorUn, limitUn] = await Promise.all([
        // Listen for streaming tokens
        listen<{ chunk: string }>('chat:token', (event) => {
          const { currentStreamId } = get();
          if (currentStreamId !== assistantMessage.id) return;
          get()._appendContent(assistantMessage.id, event.payload.chunk);
          set({ status: 'streaming' });
        }),

        // Listen for extended thinking
        listen<{ status: string; chunk?: string; content?: string }>('chat:thinking', (event) => {
          const { currentStreamId } = get();
          if (currentStreamId !== assistantMessage.id) return;
          const { status, chunk, content } = event.payload;
          if (status === 'started') {
            set((state) => ({
              messages: state.messages.map((m) =>
                m.id === assistantMessage.id ? { ...m, isThinking: true, thinking: '' } : m
              ),
            }));
          } else if (status === 'streaming' && chunk) {
            get()._appendThinking(assistantMessage.id, chunk);
          } else if (status === 'complete') {
            get()._finishThinking(assistantMessage.id, content);
          }
        }),

        // Listen for thought steps (tool usage)
        listen<ThoughtStep>('chat:thought', (event) => {
          const { currentStreamId } = get();
          if (currentStreamId !== assistantMessage.id) return;
          const thought = event.payload;
          const existingThought = get().messages
            .find(m => m.id === assistantMessage.id)?.thoughts
            ?.find(t => t.id === thought.id);
          if (existingThought) {
            get()._updateThought(assistantMessage.id, thought.id, thought);
          } else {
            get()._addThought(assistantMessage.id, thought);
          }
        }),

        // Listen for completion
        listen('chat:complete', () => {
          get()._finishStream(assistantMessage.id);
          useSubscriptionStore.getState().incrementUsage(modelType, effectiveExtendedThinking);
          useSubscriptionStore.getState().refreshUsage();
          cleanupListeners();
          emit('usage:record-chat', { model: modelType, extendedThinking: effectiveExtendedThinking });
        }),

        // Listen for errors
        listen<{ message: string }>('chat:error', (event) => {
          get()._setError(event.payload.message);
          get()._finishStream(assistantMessage.id);
          cleanupListeners();
        }),

        // Listen for limit errors (subscription/quota exceeded)
        listen<{ reason: string; upgradeUrl?: string }>('chat:limit-error', (event) => {
          get()._setError(event.payload.reason);
          get()._finishStream(assistantMessage.id);
          cleanupListeners();
        }),
      ]);

      // Assign unlisten functions after Promise.all completes
      unlistenToken = tokenUn;
      unlistenThinking = thinkingUn;
      unlistenThought = thoughtUn;
      unlistenComplete = completeUn;
      unlistenError = errorUn;
      unlistenLimitError = limitUn;

      // Convert context items to format expected by backend
      const contextItems = activeContext.map(c => ({
        id: c.id,
        type: c.type,
        path: c.path,
        name: c.name,
        strategy: c.strategy,
        size: c.size,
        mimeType: c.mimeType,
      }));

      // Convert messages to conversation history format
      // Include contextItems so backend knows about previous attachments
      const conversationHistory = messages.map(m => ({
        role: m.role,
        content: m.content,
        contextItems: m.contextItems || [],
      }));

      // Invoke backend command
      // The request param maps to ChatStreamRequest struct with camelCase fields
      // Get userId from subscription store for billing tracking
      const userId = useSubscriptionStore.getState().userId;

      await invoke('chat_stream', {
        request: {
          message: text,
          contextItems,
          model,
          history: conversationHistory,
          extendedThinking: effectiveExtendedThinking,
          userId,
        },
      });

      // NOTE: invoke() returns when the backend command completes,
      // but events are emitted DURING processing, so we do NOT cleanup here.
      // Cleanup happens in the complete/error handlers above.

    } catch (err) {
      // Only cleanup in catch if invoke() itself failed (not streaming events)
      const errorMessage = err instanceof Error ? err.message : String(err);
      console.error('[ChatStore] invoke failed:', err);

      // Provide more helpful error messages for common issues
      let displayError = errorMessage;
      if (errorMessage.includes('CLAUDE_API_KEY') || errorMessage.includes('not configured')) {
        displayError = 'API key not configured. Please contact support.';
      } else if (errorMessage.includes('401') || errorMessage.includes('Authentication')) {
        displayError = 'Authentication failed. Please check your API key.';
      } else if (errorMessage.includes('429') || errorMessage.includes('rate')) {
        displayError = 'Rate limit exceeded. Please wait a moment and try again.';
      } else if (errorMessage.includes('network') || errorMessage.includes('fetch')) {
        displayError = 'Network error. Please check your internet connection.';
      }

      get()._setError(displayError);
      get()._finishStream(assistantMessage.id);
      // No usage recorded for failed requests (we only increment on success)
      cleanupListeners();
    }
    // NO finally block - listeners are cleaned up in complete/error handlers
  },

  abort: () => {
    const { currentStreamId, _activeCleanup } = get();
    if (currentStreamId) {
      invoke('abort_chat').catch(console.error);
      get()._finishStream(currentStreamId);
      // No usage recorded for aborted requests (we only increment on success)
      // Cleanup any active listeners (using state-stored ref, not module-level)
      _activeCleanup?.();
    }
  },

  clearHistory: () => set({
    messages: [],
    activeContext: [],
    error: null,
    status: 'idle',
  }),

  clearError: () => set({ status: 'idle', error: null }),

  retryLastMessage: async () => {
    const { messages, status } = get();

    // Don't retry if already processing
    if (status === 'thinking' || status === 'streaming') {
      return;
    }

    // Find the last user message
    const lastUserMessage = [...messages].reverse().find((m: ChatMessage) => m.role === 'user');
    if (!lastUserMessage) return;

    // Remove the failed assistant message (if any) and retry
    // Using reverse iteration for ES2022 compatibility instead of findLastIndex
    let lastAssistantIdx = -1;
    for (let i = messages.length - 1; i >= 0; i--) {
      if (messages[i].role === 'assistant') {
        lastAssistantIdx = i;
        break;
      }
    }
    if (lastAssistantIdx > 0) {
      // Check if it's empty or errored (failed response)
      const lastAssistant = messages[lastAssistantIdx];
      if (!lastAssistant.content || lastAssistant.content.length === 0) {
        // Remove the failed assistant message
        set({ messages: messages.slice(0, lastAssistantIdx) });
      }
    }

    // Clear error and retry
    set({ error: null });
    await get().sendMessage(lastUserMessage.content);
  },

  // Mention autocomplete actions
  openMention: (startIndex) => set({
    isMentionOpen: true,
    mentionQuery: '',
    mentionStartIndex: startIndex,
    mentionResults: [],
    selectedMentionIndex: 0,
  }),
  closeMention: () => set({
    isMentionOpen: false,
    mentionQuery: '',
    mentionStartIndex: -1,
    mentionResults: [],
    selectedMentionIndex: 0,
    isMentionLoading: false,
  }),
  setMentionQuery: (query) => set({ mentionQuery: query }),
  setMentionResults: (items) => set({ mentionResults: items, isMentionLoading: false }),
  setMentionLoading: (loading) => set({ isMentionLoading: loading }),
  selectNextMention: () => set((state) => ({
    selectedMentionIndex: Math.min(state.selectedMentionIndex + 1, state.mentionResults.length - 1),
  })),
  selectPrevMention: () => set((state) => ({
    selectedMentionIndex: Math.max(state.selectedMentionIndex - 1, 0),
  })),
  resetMentionSelection: () => set({ selectedMentionIndex: 0 }),

  // Internal actions
  _addThought: (messageId, thought) => {
    const { messages, currentStreamId } = get();
    // Verify stream ID matches and message exists
    if (currentStreamId !== messageId) return;
    if (!messages.some(m => m.id === messageId)) return;

    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId
          ? { ...m, thoughts: [...(m.thoughts || []), thought] }
          : m
      ),
    }));
  },

  _updateThought: (messageId, thoughtId, update) => {
    const { messages, currentStreamId } = get();
    // Verify stream ID matches and message exists
    if (currentStreamId !== messageId) return;
    if (!messages.some(m => m.id === messageId)) return;

    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId
          ? {
              ...m,
              thoughts: m.thoughts?.map((t) =>
                t.id === thoughtId ? { ...t, ...update } : t
              ),
            }
          : m
      ),
    }));
  },

  _appendContent: (messageId, chunk) => {
    const { messages, currentStreamId } = get();
    // Verify stream ID matches and message exists
    if (currentStreamId !== messageId) return;
    if (!messages.some(m => m.id === messageId)) return;

    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId ? { ...m, content: m.content + chunk } : m
      ),
    }));
  },

  _appendThinking: (messageId, chunk) => {
    const { messages, currentStreamId } = get();
    // Verify stream ID matches and message exists
    if (currentStreamId !== messageId) return;
    if (!messages.some(m => m.id === messageId)) return;

    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId
          ? { ...m, thinking: (m.thinking || '') + chunk, isThinking: true }
          : m
      ),
    }));
  },

  _finishThinking: (messageId, fullContent) => {
    const { messages } = get();
    // Verify message exists (don't check currentStreamId here - thinking can finish late)
    if (!messages.some(m => m.id === messageId)) return;

    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId
          ? { ...m, thinking: fullContent ?? m.thinking, isThinking: false }
          : m
      ),
    }));
  },

  _finishStream: (messageId) => {
    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId ? { ...m, isStreaming: false } : m
      ),
      status: 'idle',
      currentStreamId: null,
    }));
  },

  _setError: (error) => {
    const errorTime = Date.now();
    set({ status: 'error', error });

    // Show user-visible error toast for production debugging
    if (error) {
      console.error('[ChatStore] Error:', error);
      showError('Chat Error', error);
    }

    // Auto-recovery: reset to idle after timeout ONLY if user hasn't interacted
    // This prevents clearing errors while user is actively investigating
    if (error) {
      setTimeout(() => {
        const { status, _lastInteraction } = get();
        // Only auto-recover if still in error AND user hasn't interacted since error
        if (status === 'error' && _lastInteraction < errorTime) {
          set({ status: 'idle' });
        }
      }, ERROR_AUTO_RECOVERY_MS);
    }
  },
    }),
    {
      name: 'sentinel-chat-preferences',
      partialize: (state) => ({
        model: state.model,
        extendedThinking: state.extendedThinking,
      }),
    }
  )
);

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/**
 * Determine the context strategy based on item type and MIME
 */
export function getContextStrategy(
  type: 'file' | 'folder',
  mimeType?: string
): 'hologram' | 'read' | 'vision' {
  if (type === 'folder') {
    return 'hologram';
  }
  if (mimeType?.startsWith('image/')) {
    return 'vision';
  }
  return 'read';
}

/**
 * Format file size for display
 */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}
