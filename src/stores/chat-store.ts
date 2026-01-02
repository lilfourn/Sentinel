import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useSubscriptionStore, chatModelToSubscriptionModel } from './subscription-store';

// ============================================================================
// MODULE-LEVEL STATE FOR LISTENER CLEANUP
// ============================================================================

/** Cleanup function for active listeners - stored at module scope for unmount handling */
let activeListenerCleanup: (() => void) | null = null;

/** Auto-recovery timeout for error state */
const ERROR_AUTO_RECOVERY_MS = 30000;

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

export type ChatModel = 'claude-haiku-4-5' | 'claude-sonnet-4-5' | 'claude-opus-4-5';
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
  extendedThinking: true,
  currentStreamId: null,
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

    const newItems: ContextItem[] = items.map((item) => ({
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

    set({
      messages: [...messages, userMessage, assistantMessage],
      status: 'thinking',
      error: null,
      currentStreamId: assistantMessage.id,
      activeContext: [], // Clear context after attaching to message
    });

    // Optimistic update: increment usage counter immediately for responsive UI
    const modelType = chatModelToSubscriptionModel(model);
    useSubscriptionStore.getState().incrementUsage(modelType, extendedThinking);

    // Set up event listeners
    let unlistenToken: UnlistenFn | null = null;
    let unlistenThought: UnlistenFn | null = null;
    let unlistenThinking: UnlistenFn | null = null;
    let unlistenComplete: UnlistenFn | null = null;
    let unlistenError: UnlistenFn | null = null;
    let listenersActive = true;

    // Cleanup function - stored at module scope for unmount handling
    const cleanupListeners = () => {
      if (!listenersActive) return;
      listenersActive = false;
      activeListenerCleanup = null;
      unlistenToken?.();
      unlistenThinking?.();
      unlistenThought?.();
      unlistenComplete?.();
      unlistenError?.();
    };

    // Store cleanup ref at module scope
    activeListenerCleanup = cleanupListeners;

    try {
      // Listen for streaming tokens
      unlistenToken = await listen<{ chunk: string }>('chat:token', (event) => {
        // Verify this is for the active stream
        const { currentStreamId } = get();
        if (currentStreamId !== assistantMessage.id) return;

        get()._appendContent(assistantMessage.id, event.payload.chunk);
        set({ status: 'streaming' });
      });

      // Listen for extended thinking
      unlistenThinking = await listen<{ status: string; chunk?: string; content?: string }>('chat:thinking', (event) => {
        // Verify this is for the active stream
        const { currentStreamId } = get();
        if (currentStreamId !== assistantMessage.id) return;

        const { status, chunk, content } = event.payload;
        if (status === 'started') {
          // Mark message as thinking
          set((state) => ({
            messages: state.messages.map((m) =>
              m.id === assistantMessage.id ? { ...m, isThinking: true, thinking: '' } : m
            ),
          }));
        } else if (status === 'streaming' && chunk) {
          // Append thinking chunk
          get()._appendThinking(assistantMessage.id, chunk);
        } else if (status === 'complete') {
          // Finish thinking
          get()._finishThinking(assistantMessage.id, content);
        }
      });

      // Listen for thought steps (tool usage)
      unlistenThought = await listen<ThoughtStep>('chat:thought', (event) => {
        // Verify this is for the active stream
        const { currentStreamId } = get();
        if (currentStreamId !== assistantMessage.id) return;

        const thought = event.payload;
        const existingThought = get().messages
          .find(m => m.id === assistantMessage.id)?.thoughts
          ?.find(t => t.id === thought.id);

        if (existingThought) {
          // Update existing thought
          get()._updateThought(assistantMessage.id, thought.id, thought);
        } else {
          // Add new thought
          get()._addThought(assistantMessage.id, thought);
        }
      });

      // Listen for completion - cleanup listeners HERE, not in finally
      unlistenComplete = await listen('chat:complete', () => {
        get()._finishStream(assistantMessage.id);
        // Sync usage with backend to ensure accuracy
        useSubscriptionStore.getState().refreshUsage();
        cleanupListeners(); // Cleanup after completion
      });

      // Listen for errors - cleanup listeners HERE, not in finally
      unlistenError = await listen<{ message: string }>('chat:error', (event) => {
        get()._setError(event.payload.message);
        get()._finishStream(assistantMessage.id);
        cleanupListeners(); // Cleanup after error
      });

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
      const conversationHistory = messages.map(m => ({
        role: m.role,
        content: m.content,
      }));

      // Invoke backend command
      // The request param maps to ChatStreamRequest struct with camelCase fields
      await invoke('chat_stream', {
        request: {
          message: text,
          contextItems,
          model,
          history: conversationHistory,
          extendedThinking,
        },
      });

      // NOTE: invoke() returns when the backend command completes,
      // but events are emitted DURING processing, so we do NOT cleanup here.
      // Cleanup happens in the complete/error handlers above.

    } catch (err) {
      // Only cleanup in catch if invoke() itself failed (not streaming events)
      const errorMessage = err instanceof Error ? err.message : String(err);
      get()._setError(errorMessage);
      get()._finishStream(assistantMessage.id);
      cleanupListeners();
    }
    // NO finally block - listeners are cleaned up in complete/error handlers
  },

  abort: () => {
    const { currentStreamId } = get();
    if (currentStreamId) {
      invoke('abort_chat').catch(console.error);
      get()._finishStream(currentStreamId);
      // Cleanup any active listeners
      activeListenerCleanup?.();
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
    set({ status: 'error', error });

    // Auto-recovery: reset to idle after timeout if still in error state
    if (error) {
      setTimeout(() => {
        const { status } = get();
        if (status === 'error') {
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
