import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

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
  contextRefs?: string[];  // IDs of ContextItems used
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
  removeContext: (id: string) => void;
  clearContext: () => void;

  // Model
  setModel: (model: ChatModel) => void;
  setExtendedThinking: (enabled: boolean) => void;

  // Messaging
  sendMessage: (text: string) => Promise<void>;
  abort: () => void;
  clearHistory: () => void;

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
    const { activeContext, model, messages, extendedThinking } = get();

    if (!text.trim()) return;

    // Create user message
    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: text,
      timestamp: Date.now(),
      contextRefs: activeContext.map(c => c.id),
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
    });

    // Set up event listeners
    let unlistenToken: UnlistenFn | null = null;
    let unlistenThought: UnlistenFn | null = null;
    let unlistenThinking: UnlistenFn | null = null;
    let unlistenComplete: UnlistenFn | null = null;
    let unlistenError: UnlistenFn | null = null;

    try {
      // Listen for streaming tokens
      unlistenToken = await listen<{ chunk: string }>('chat:token', (event) => {
        get()._appendContent(assistantMessage.id, event.payload.chunk);
        set({ status: 'streaming' });
      });

      // Listen for extended thinking
      unlistenThinking = await listen<{ status: string; chunk?: string; content?: string }>('chat:thinking', (event) => {
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

      // Listen for completion
      unlistenComplete = await listen('chat:complete', () => {
        get()._finishStream(assistantMessage.id);
      });

      // Listen for errors
      unlistenError = await listen<{ message: string }>('chat:error', (event) => {
        get()._setError(event.payload.message);
        get()._finishStream(assistantMessage.id);
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

    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      get()._setError(errorMessage);
      get()._finishStream(assistantMessage.id);
    } finally {
      // Cleanup listeners
      unlistenToken?.();
      unlistenThinking?.();
      unlistenThought?.();
      unlistenComplete?.();
      unlistenError?.();
    }
  },

  abort: () => {
    const { currentStreamId } = get();
    if (currentStreamId) {
      invoke('abort_chat').catch(console.error);
      get()._finishStream(currentStreamId);
    }
  },

  clearHistory: () => set({
    messages: [],
    activeContext: [],
    error: null,
    status: 'idle',
  }),

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
    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId
          ? { ...m, thoughts: [...(m.thoughts || []), thought] }
          : m
      ),
    }));
  },

  _updateThought: (messageId, thoughtId, update) => {
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
    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId ? { ...m, content: m.content + chunk } : m
      ),
    }));
  },

  _appendThinking: (messageId, chunk) => {
    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId
          ? { ...m, thinking: (m.thinking || '') + chunk, isThinking: true }
          : m
      ),
    }));
  },

  _finishThinking: (messageId, fullContent) => {
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
