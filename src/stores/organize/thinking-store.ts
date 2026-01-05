import { create } from 'zustand';

// Thought/step types for the AI thinking stream
export type ThoughtType =
  | 'scanning'
  | 'analyzing'
  | 'naming_conventions'
  | 'planning'
  | 'thinking'
  | 'executing'
  | 'complete'
  | 'error';

// Expandable detail item for rich thought display
export interface ThoughtDetail {
  label: string;
  value: string;
}

export interface AIThought {
  id: string;
  type: ThoughtType;
  content: string;
  timestamp: number;
  detail?: string;
  // Expandable details for richer information display
  expandableDetails?: ThoughtDetail[];
}

interface ThinkingState {
  thoughts: AIThought[];
  currentPhase: ThoughtType;
  latestEvent: { type: string; detail: string } | null;
}

interface ThinkingActions {
  addThought: (
    type: ThoughtType,
    content: string,
    detail?: string,
    expandableDetails?: ThoughtDetail[]
  ) => void;
  setPhase: (phase: ThoughtType) => void;
  clearThoughts: () => void;
}

/**
 * Generate a unique thought ID using crypto.randomUUID.
 * Falls back to timestamp + random if crypto is unavailable.
 */
function generateThoughtId(): string {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return `thought-${crypto.randomUUID()}`;
  }
  // Fallback for environments without crypto.randomUUID
  return `thought-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

export const useThinkingStore = create<ThinkingState & ThinkingActions>((set) => ({
  // Initial state
  thoughts: [],
  currentPhase: 'scanning',
  latestEvent: null,

  // Actions
  addThought: (type, content, detail, expandableDetails) =>
    set((state) => ({
      thoughts: [
        ...state.thoughts,
        {
          id: generateThoughtId(),
          type,
          content,
          detail,
          expandableDetails,
          timestamp: Date.now(),
        },
      ],
      currentPhase: type,
      latestEvent: { type, detail: content },
    })),

  setPhase: (phase) => set({ currentPhase: phase }),

  clearThoughts: () => set({ thoughts: [], currentPhase: 'scanning', latestEvent: null }),
}));
