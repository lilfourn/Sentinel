import { create } from 'zustand';
import type { GhostState, GhostStateMap } from '../types/ghost';
import type { OrganizePlan } from './organize/plan-store';

interface GhostRenderState {
  /** Map of file paths to their ghost rendering state */
  ghostMap: GhostStateMap;
  /** Set of paths currently animating (for completion animations) */
  activeAnimations: Set<string>;
  /** Whether ghost rendering is enabled */
  showGhosts: boolean;
  /** Track active animation timeouts for cleanup */
  _animationTimeouts: Map<string, ReturnType<typeof setTimeout>>;
}

interface GhostRenderActions {
  /** Set the ghost state for a specific path */
  setGhostState: (
    path: string,
    state: GhostState,
    metadata?: { operationId?: string; linkedPath?: string; isVirtual?: boolean }
  ) => void;
  /** Clear the ghost state for a path */
  clearGhostState: (path: string) => void;
  /** Build the ghost map from an organization plan */
  buildGhostMapFromPlan: (plan: OrganizePlan) => void;
  /** Mark an animation as complete and clean up */
  completeAnimation: (path: string) => void;
  /** Toggle ghost visibility */
  toggleGhosts: () => void;
  /** Reset all ghost state */
  reset: () => void;
  /** Mark an operation as completed (triggers completion animation) */
  markOperationCompleted: (operationId: string) => void;
}

/**
 * Extracts the parent directory from a path.
 */
function getParentPath(path: string): string {
  const parts = path.split('/');
  parts.pop();
  return parts.join('/') || '/';
}

export const useGhostStore = create<GhostRenderState & GhostRenderActions>((set, get) => ({
  // Initial state
  ghostMap: new Map(),
  activeAnimations: new Set(),
  showGhosts: true,
  _animationTimeouts: new Map(),

  setGhostState: (path, state, metadata = {}) => {
    set((prev) => {
      const newMap = new Map(prev.ghostMap);
      newMap.set(path, {
        state,
        operationId: metadata.operationId || '',
        linkedPath: metadata.linkedPath,
        isVirtual: metadata.isVirtual,
      });
      return { ghostMap: newMap };
    });
  },

  clearGhostState: (path) => {
    set((prev) => {
      const newMap = new Map(prev.ghostMap);
      newMap.delete(path);
      return { ghostMap: newMap };
    });
  },

  buildGhostMapFromPlan: (plan: OrganizePlan) => {
    const ghostMap: GhostStateMap = new Map();

    for (const op of plan.operations) {
      switch (op.type) {
        case 'create_folder':
          if (op.path) {
            ghostMap.set(op.path, {
              state: 'creating',
              operationId: op.opId,
              isVirtual: true,
            });
          }
          break;

        case 'move':
          if (op.source && op.destination) {
            // Source: mark as being moved away
            ghostMap.set(op.source, {
              state: 'source',
              operationId: op.opId,
              linkedPath: op.destination,
            });
            // Destination: mark as ghost (will appear)
            ghostMap.set(op.destination, {
              state: 'destination',
              operationId: op.opId,
              linkedPath: op.source,
              isVirtual: true,
            });
          }
          break;

        case 'rename':
          if (op.path && op.newName) {
            const parentPath = getParentPath(op.path);
            const newPath = `${parentPath}/${op.newName}`;

            // Old path: mark as source
            ghostMap.set(op.path, {
              state: 'source',
              operationId: op.opId,
              linkedPath: newPath,
            });
            // New path: mark as destination
            ghostMap.set(newPath, {
              state: 'destination',
              operationId: op.opId,
              linkedPath: op.path,
              isVirtual: true,
            });
          }
          break;

        case 'trash':
          if (op.path) {
            ghostMap.set(op.path, {
              state: 'deleting',
              operationId: op.opId,
            });
          }
          break;

        case 'copy':
          if (op.source && op.destination) {
            // Source stays normal, destination is ghost
            ghostMap.set(op.destination, {
              state: 'destination',
              operationId: op.opId,
              linkedPath: op.source,
              isVirtual: true,
            });
          }
          break;
      }
    }

    set({ ghostMap, showGhosts: true });
  },

  completeAnimation: (path) => {
    set((prev) => {
      const newAnimations = new Set(prev.activeAnimations);
      newAnimations.delete(path);
      const newMap = new Map(prev.ghostMap);
      newMap.delete(path);
      return {
        activeAnimations: newAnimations,
        ghostMap: newMap,
      };
    });
  },

  toggleGhosts: () => {
    set((prev) => ({ showGhosts: !prev.showGhosts }));
  },

  reset: () => {
    // Clear all pending animation timeouts to prevent memory leaks
    const { _animationTimeouts } = get();
    for (const timeoutId of _animationTimeouts.values()) {
      clearTimeout(timeoutId);
    }

    set({
      ghostMap: new Map(),
      activeAnimations: new Set(),
      showGhosts: true,
      _animationTimeouts: new Map(),
    });
  },

  markOperationCompleted: (operationId) => {
    const pathsToAnimate: string[] = [];

    set((prev) => {
      const newMap = new Map(prev.ghostMap);
      const newAnimations = new Set(prev.activeAnimations);

      // Find all paths associated with this operation
      for (const [path, metadata] of newMap) {
        if (metadata.operationId === operationId) {
          // Change state to completed and add to active animations
          newMap.set(path, { ...metadata, state: 'completed' });
          newAnimations.add(path);
          pathsToAnimate.push(path);
        }
      }

      return {
        ghostMap: newMap,
        activeAnimations: newAnimations,
      };
    });

    // Schedule cleanup after animation completes (outside of set() to avoid nested calls)
    const ANIMATION_DURATION_MS = 400;
    for (const path of pathsToAnimate) {
      // Cancel any existing timeout for this path to prevent duplicate callbacks
      const existingTimeout = get()._animationTimeouts.get(path);
      if (existingTimeout) {
        clearTimeout(existingTimeout);
      }

      const timeoutId = setTimeout(() => {
        // Guard: verify path is still tracked (store may have been reset)
        const currentState = get();
        const trackedTimeout = currentState._animationTimeouts.get(path);

        // Only proceed if this timeout is still the active one for this path
        // and the path still exists in the ghost map
        if (trackedTimeout !== timeoutId || !currentState.ghostMap.has(path)) {
          return;
        }

        // Clean up timeout tracking and complete animation
        set((prev) => {
          const newTimeouts = new Map(prev._animationTimeouts);
          newTimeouts.delete(path);
          return { _animationTimeouts: newTimeouts };
        });
        get().completeAnimation(path);
      }, ANIMATION_DURATION_MS);

      // Track the timeout for cleanup on reset
      set((prev) => {
        const newTimeouts = new Map(prev._animationTimeouts);
        newTimeouts.set(path, timeoutId);
        return { _animationTimeouts: newTimeouts };
      });
    }
  },
}));
