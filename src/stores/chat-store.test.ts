import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useChatStore } from './chat-store';

// Mock invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

describe('ChatStore', () => {
  beforeEach(() => {
    // Reset store state to defaults
    useChatStore.setState({
      messages: [],
      status: 'idle',
      model: 'claude-haiku-4-5',
      extendedThinking: false,
      error: null,
      isOpen: false,
      activeContext: [],
    });
    vi.clearAllMocks();
  });

  describe('Model Selection', () => {
    it('should have default model as Haiku', () => {
      const state = useChatStore.getState();
      expect(state.model).toBe('claude-haiku-4-5');
    });

    it('should update model correctly', () => {
      const { setModel } = useChatStore.getState();
      setModel('gpt-5-mini-2025-08-07');

      const state = useChatStore.getState();
      expect(state.model).toBe('gpt-5-mini-2025-08-07');
    });

    it('should switch between Claude and GPT models', () => {
      const { setModel } = useChatStore.getState();

      setModel('gpt-5-nano-2025-08-07');
      expect(useChatStore.getState().model).toBe('gpt-5-nano-2025-08-07');

      setModel('claude-sonnet-4-5');
      expect(useChatStore.getState().model).toBe('claude-sonnet-4-5');
    });
  });

  describe('Extended Thinking', () => {
    it('should have extended thinking disabled by default', () => {
      const state = useChatStore.getState();
      expect(state.extendedThinking).toBe(false);
    });

    it('should toggle extended thinking', () => {
      const { setExtendedThinking } = useChatStore.getState();

      setExtendedThinking(true);
      expect(useChatStore.getState().extendedThinking).toBe(true);

      setExtendedThinking(false);
      expect(useChatStore.getState().extendedThinking).toBe(false);
    });
  });

  describe('Panel State', () => {
    it('should open and close panel', () => {
      const { open, close } = useChatStore.getState();

      expect(useChatStore.getState().isOpen).toBe(false);

      open();
      expect(useChatStore.getState().isOpen).toBe(true);

      close();
      expect(useChatStore.getState().isOpen).toBe(false);
    });

    it('should toggle panel', () => {
      const { toggle } = useChatStore.getState();

      expect(useChatStore.getState().isOpen).toBe(false);

      toggle();
      expect(useChatStore.getState().isOpen).toBe(true);

      toggle();
      expect(useChatStore.getState().isOpen).toBe(false);
    });
  });

  describe('Context Management', () => {
    it('should add context items', () => {
      const { addContext } = useChatStore.getState();

      addContext({
        type: 'file',
        path: '/test/file.txt',
        name: 'file.txt',
        strategy: 'read',
      });

      const state = useChatStore.getState();
      expect(state.activeContext).toHaveLength(1);
      expect(state.activeContext[0].path).toBe('/test/file.txt');
    });

    it('should remove context items', () => {
      const { addContext, removeContext } = useChatStore.getState();

      addContext({
        type: 'file',
        path: '/test/file.txt',
        name: 'file.txt',
        strategy: 'read',
      });

      const contextId = useChatStore.getState().activeContext[0].id;
      removeContext(contextId);

      expect(useChatStore.getState().activeContext).toHaveLength(0);
    });

    it('should clear all context', () => {
      const { addContext, clearContext } = useChatStore.getState();

      addContext({ type: 'file', path: '/test/a.txt', name: 'a.txt', strategy: 'read' });
      addContext({ type: 'file', path: '/test/b.txt', name: 'b.txt', strategy: 'read' });

      expect(useChatStore.getState().activeContext).toHaveLength(2);

      clearContext();
      expect(useChatStore.getState().activeContext).toHaveLength(0);
    });
  });

  describe('Error Handling', () => {
    it('should set and clear errors', () => {
      const { _setError, clearError } = useChatStore.getState();

      _setError('Test error');
      expect(useChatStore.getState().error).toBe('Test error');
      expect(useChatStore.getState().status).toBe('error');

      clearError();
      expect(useChatStore.getState().error).toBeNull();
      expect(useChatStore.getState().status).toBe('idle');
    });
  });

  describe('History Management', () => {
    it('should clear message history', () => {
      // Manually set some messages
      useChatStore.setState({
        messages: [
          { id: '1', role: 'user', content: 'Hello', timestamp: Date.now(), contextItems: [] },
          { id: '2', role: 'assistant', content: 'Hi', timestamp: Date.now() },
        ],
      });

      expect(useChatStore.getState().messages).toHaveLength(2);

      const { clearHistory } = useChatStore.getState();
      clearHistory();

      expect(useChatStore.getState().messages).toHaveLength(0);
    });
  });
});
