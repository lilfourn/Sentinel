import { create } from 'zustand';
import type { FileEntry } from '../types/file';
import type { GhostFileEntry, GhostState } from '../types/ghost';
import type { VfsEvent, ConflictPayload, IndexingProgressPayload } from '../types/vfs';
import { isIndexingProgressPayload, isConflictPayload } from '../types/vfs';
import type { OrganizePlan } from './organize/plan-store';

/**
 * Result from the backend vfs_validate_plan command.
 * Provides structured validation with plan hash for sync validation.
 */
export interface VfsValidationResult {
  /** Whether the plan is valid (no errors) */
  valid: boolean;
  /** List of validation errors */
  errors: string[];
  /** Hash of the plan for sync validation */
  planHash: string;
  /** Plan ID for reference */
  planId: string;
}

/**
 * Runtime type guard for VfsValidationResult.
 * Validates that backend responses match the expected shape.
 */
export function isVfsValidationResult(value: unknown): value is VfsValidationResult {
  if (typeof value !== 'object' || value === null) return false;
  const obj = value as Record<string, unknown>;
  return (
    typeof obj.valid === 'boolean' &&
    Array.isArray(obj.errors) &&
    obj.errors.every((e) => typeof e === 'string') &&
    typeof obj.planHash === 'string' &&
    typeof obj.planId === 'string'
  );
}

/**
 * Validates and parses a backend validation result.
 * Throws descriptive error if shape doesn't match.
 */
export function parseVfsValidationResult(value: unknown): VfsValidationResult {
  if (!isVfsValidationResult(value)) {
    const received = JSON.stringify(value, null, 2);
    throw new Error(
      `Invalid VfsValidationResult from backend. ` +
      `Expected {valid: boolean, errors: string[], planHash: string, planId: string}. ` +
      `Received: ${received.slice(0, 200)}${received.length > 200 ? '...' : ''}`
    );
  }
  return value;
}

interface VfsState {
  /** Whether the VFS simulation is active */
  isActive: boolean;
  /** Virtual entries that don't exist on disk yet (destinations, new folders) */
  virtualEntries: Map<string, GhostFileEntry>;
  /** Paths that should be removed from display (sources of moves) */
  removedPaths: Set<string>;
  /** Mapping from source paths to destination paths */
  movedEntries: Map<string, string>;
  /** Current indexing progress (null if not indexing) */
  indexingProgress: IndexingProgressPayload | null;
  /** List of conflicts detected during simulation */
  conflicts: ConflictPayload[];
  /** Target folder being organized */
  targetFolder: string | null;
  /** Plan ID of the currently simulated plan (for VFS-WAL sync validation) */
  simulatedPlanId: string | null;
  /** Cached validation result from backend (includes plan hash for sync) */
  validationResult: VfsValidationResult | null;
}

interface VfsActions {
  /** Initialize the VFS for a target folder */
  initializeVfs: (targetFolder: string) => void;
  /** Handle a VFS event from the backend */
  handleVfsEvent: (event: VfsEvent) => void;
  /** Apply an organization plan to create the virtual view */
  applyPlan: (plan: OrganizePlan) => void;
  /** Get entries with ghost state merged in */
  getMergedEntries: (realEntries: FileEntry[], currentPath: string) => GhostFileEntry[];
  /** Reset the VFS state */
  reset: () => void;
  /** Add a conflict */
  addConflict: (conflict: ConflictPayload) => void;
  /** Clear conflicts */
  clearConflicts: () => void;
  /** Get the currently simulated plan ID (for VFS-WAL sync validation) */
  getSimulatedPlanId: () => string | null;
  /** Set validation result from backend */
  setValidationResult: (result: VfsValidationResult | null) => void;
  /** Get the cached plan hash (for sync validation) */
  getPlanHash: () => string | null;
}

/**
 * Creates a virtual file entry from operation metadata.
 */
function createVirtualEntry(
  path: string,
  isDirectory: boolean,
  ghostState: GhostState,
  operationId: string,
  linkedPath?: string
): GhostFileEntry {
  const name = path.split('/').pop() || '';
  return {
    name,
    path,
    isDirectory,
    isFile: !isDirectory,
    isSymlink: false,
    size: 0,
    modifiedAt: Date.now(),
    createdAt: Date.now(),
    extension: isDirectory ? null : name.split('.').pop() || null,
    mimeType: null,
    isHidden: name.startsWith('.'),
    ghostState,
    operationId,
    linkedPath,
    ghostSince: Date.now(),
    isVirtual: true,
  };
}

/**
 * Extracts the parent directory from a path.
 */
function getParentPath(path: string): string {
  const parts = path.split('/');
  parts.pop();
  return parts.join('/') || '/';
}

export const useVfsStore = create<VfsState & VfsActions>((set, get) => ({
  // Initial state
  isActive: false,
  virtualEntries: new Map(),
  removedPaths: new Set(),
  movedEntries: new Map(),
  indexingProgress: null,
  conflicts: [],
  targetFolder: null,
  simulatedPlanId: null,
  validationResult: null,

  initializeVfs: (targetFolder: string) => {
    set({
      isActive: true,
      targetFolder,
      virtualEntries: new Map(),
      removedPaths: new Set(),
      movedEntries: new Map(),
      indexingProgress: null,
      conflicts: [],
      simulatedPlanId: null,
      validationResult: null,
    });
  },

  handleVfsEvent: (event: VfsEvent) => {
    switch (event.type) {
      case 'indexing_progress':
        if (isIndexingProgressPayload(event.payload)) {
          set({ indexingProgress: event.payload });
        } else {
          console.warn('[VFS] Invalid indexing_progress payload:', event.payload);
        }
        break;
      case 'indexing_complete':
        set({ indexingProgress: null });
        break;
      case 'conflict_detected':
        if (isConflictPayload(event.payload)) {
          get().addConflict(event.payload);
        } else {
          console.warn('[VFS] Invalid conflict_detected payload:', event.payload);
        }
        break;
      case 'operation_complete':
        // Could update individual operation states here
        break;
      case 'rollback_progress':
        // Handle rollback updates
        break;
    }
  },

  applyPlan: (plan: OrganizePlan) => {
    const virtualEntries = new Map<string, GhostFileEntry>();
    const removedPaths = new Set<string>();
    const movedEntries = new Map<string, string>();

    for (const op of plan.operations) {
      switch (op.type) {
        case 'create_folder':
          if (op.path) {
            virtualEntries.set(
              op.path,
              createVirtualEntry(op.path, true, 'creating', op.opId)
            );
          }
          break;

        case 'move':
          if (op.source && op.destination) {
            // Mark source as being moved away
            removedPaths.add(op.source);
            movedEntries.set(op.source, op.destination);

            // Determine if this is a file or folder based on source path
            // For now, assume file unless path ends with / or has no extension
            const name = op.source.split('/').pop() || '';
            const hasExtension = name.includes('.') && !name.startsWith('.');
            const isDirectory = !hasExtension;

            // Create virtual entry at destination
            virtualEntries.set(
              op.destination,
              createVirtualEntry(
                op.destination,
                isDirectory,
                'destination',
                op.opId,
                op.source
              )
            );
          }
          break;

        case 'rename':
          if (op.path && op.newName) {
            const parentPath = getParentPath(op.path);
            const newPath = `${parentPath}/${op.newName}`;

            // Mark old path as removed
            removedPaths.add(op.path);
            movedEntries.set(op.path, newPath);

            // Create virtual entry at new path
            const name = op.path.split('/').pop() || '';
            const hasExtension = name.includes('.') && !name.startsWith('.');
            const isDirectory = !hasExtension;

            virtualEntries.set(
              newPath,
              createVirtualEntry(newPath, isDirectory, 'destination', op.opId, op.path)
            );
          }
          break;

        case 'trash':
          if (op.path) {
            removedPaths.add(op.path);
            // We don't add to virtualEntries - just mark as removed
          }
          break;

        case 'copy':
          if (op.source && op.destination) {
            // Source stays, destination is virtual
            const name = op.source.split('/').pop() || '';
            const hasExtension = name.includes('.') && !name.startsWith('.');
            const isDirectory = !hasExtension;

            virtualEntries.set(
              op.destination,
              createVirtualEntry(
                op.destination,
                isDirectory,
                'destination',
                op.opId,
                op.source
              )
            );
          }
          break;
      }
    }

    set({
      isActive: true,
      virtualEntries,
      removedPaths,
      movedEntries,
      targetFolder: plan.targetFolder,
      simulatedPlanId: plan.planId, // Store planId for VFS-WAL sync validation
    });
  },

  getMergedEntries: (realEntries: FileEntry[], currentPath: string): GhostFileEntry[] => {
    const { isActive, virtualEntries, removedPaths, movedEntries } = get();

    // If VFS is not active, return real entries with normal ghost state
    if (!isActive) {
      return realEntries.map((entry) => ({
        ...entry,
        ghostState: 'normal' as GhostState,
      }));
    }

    const result: GhostFileEntry[] = [];

    // Process real entries - filter out removed, mark sources
    for (const entry of realEntries) {
      if (removedPaths.has(entry.path)) {
        // This entry is being moved/deleted - show as source
        const destinationPath = movedEntries.get(entry.path);
        result.push({
          ...entry,
          ghostState: destinationPath ? 'source' : 'deleting',
          linkedPath: destinationPath,
          ghostSince: Date.now(),
        });
      } else {
        // Normal entry
        result.push({
          ...entry,
          ghostState: 'normal',
        });
      }
    }

    // Add virtual entries that belong in this directory
    for (const [path, virtualEntry] of virtualEntries) {
      const parentPath = getParentPath(path);
      if (parentPath === currentPath) {
        result.push(virtualEntry);
      }
    }

    // Sort: directories first, then alphabetically by name
    result.sort((a, b) => {
      if (a.isDirectory && !b.isDirectory) return -1;
      if (!a.isDirectory && b.isDirectory) return 1;
      return a.name.localeCompare(b.name);
    });

    return result;
  },

  reset: () => {
    set({
      isActive: false,
      virtualEntries: new Map(),
      removedPaths: new Set(),
      movedEntries: new Map(),
      indexingProgress: null,
      conflicts: [],
      targetFolder: null,
      simulatedPlanId: null,
      validationResult: null,
    });
  },

  addConflict: (conflict: ConflictPayload) => {
    set((state) => ({
      conflicts: [...state.conflicts, conflict],
    }));
  },

  clearConflicts: () => {
    set({ conflicts: [] });
  },

  getSimulatedPlanId: () => get().simulatedPlanId,

  setValidationResult: (result: VfsValidationResult | null) => {
    set({ validationResult: result });
  },

  getPlanHash: () => get().validationResult?.planHash ?? null,
}));
