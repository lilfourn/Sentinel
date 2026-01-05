/**
 * VFS Event Types - events emitted from the backend VFS system.
 */
export type VfsEventType =
  | 'indexing_progress'   // Progress update during folder indexing
  | 'indexing_complete'   // Indexing finished
  | 'file_moved'          // A file was moved in the VFS
  | 'operation_complete'  // A single operation completed
  | 'conflict_detected'   // A conflict was found during simulation
  | 'rollback_progress';  // Progress update during rollback

/**
 * Base VFS event structure.
 */
export interface VfsEvent {
  /** Type of event */
  type: VfsEventType;
  /** When this event occurred */
  timestamp: number;
  /** Event-specific data */
  payload: Record<string, unknown>;
}

/**
 * Payload for indexing progress events.
 */
export interface IndexingProgressPayload {
  /** Number of files scanned so far */
  scanned: number;
  /** Total estimated files to scan */
  total: number;
  /** Current path being scanned */
  currentPath: string;
}

/**
 * Payload for conflict detection events.
 */
export interface ConflictPayload {
  /** ID of the operation that caused the conflict */
  operationId: string;
  /** Path where conflict occurred */
  path: string;
  /** Type of conflict */
  conflictType: 'name_collision' | 'missing_source' | 'permission_denied';
  /** Human-readable description */
  message?: string;
}

/**
 * Payload for operation complete events.
 */
export interface OperationCompletePayload {
  /** ID of the completed operation */
  operationId: string;
  /** Whether the operation succeeded */
  success: boolean;
  /** Error message if failed */
  error?: string;
}

/**
 * Payload for rollback progress events.
 */
export interface RollbackProgressPayload {
  /** Number of operations rolled back */
  completed: number;
  /** Total operations to roll back */
  total: number;
  /** Current operation being rolled back */
  currentOperationId?: string;
}

// ============================================================================
// Type Guards for Runtime Validation
// ============================================================================

const VALID_CONFLICT_TYPES = ['name_collision', 'missing_source', 'permission_denied'] as const;

/**
 * Runtime type guard for IndexingProgressPayload.
 */
export function isIndexingProgressPayload(value: unknown): value is IndexingProgressPayload {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.scanned === 'number' &&
    typeof v.total === 'number' &&
    typeof v.currentPath === 'string'
  );
}

/**
 * Runtime type guard for ConflictPayload.
 */
export function isConflictPayload(value: unknown): value is ConflictPayload {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.operationId === 'string' &&
    typeof v.path === 'string' &&
    typeof v.conflictType === 'string' &&
    VALID_CONFLICT_TYPES.includes(v.conflictType as typeof VALID_CONFLICT_TYPES[number]) &&
    (v.message === undefined || typeof v.message === 'string')
  );
}

/**
 * DiffNode represents a node in the before/after diff tree.
 */
export interface DiffNode {
  /** File or folder name */
  name: string;
  /** Full path */
  path: string;
  /** Whether this is a directory */
  isDirectory: boolean;
  /** Change type for this node */
  changeType: 'added' | 'removed' | 'moved' | 'unchanged';
  /** For moved items, where it came from (on proposed side) or where it went (on current side) */
  linkedPath?: string;
  /** Child nodes (for directories) */
  children?: DiffNode[];
  /** Whether this folder is expanded in the UI */
  isExpanded?: boolean;
}
