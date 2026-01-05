import type { OrganizeOperation } from '../stores/organize-store';

/**
 * Extended operation with editing state for the plan edit modal.
 * Wraps OrganizeOperation with enable/disable and modification tracking.
 */
export interface EditableOperation extends OrganizeOperation {
  /** Whether this operation is enabled (will be executed) */
  enabled: boolean;
  /** Whether this operation was modified by the user */
  isModified: boolean;
  /** Original destination before edits (for move/copy operations) */
  originalDestination?: string;
  /** Original newName before edits (for rename operations) */
  originalNewName?: string;
}

/**
 * Group of operations targeting the same destination folder.
 * Used for bulk toggling of related operations.
 */
export interface OperationGroup {
  /** Unique identifier for this group */
  groupId: string;
  /** Target folder path (destination folder for moves, or folder path for creates) */
  targetFolder: string;
  /** Display name of the target folder */
  displayName: string;
  /** Operations in this group */
  operations: EditableOperation[];
  /** Whether all operations in this group are enabled */
  allEnabled: boolean;
  /** Whether some (but not all) operations are enabled (for indeterminate checkbox) */
  partialEnabled: boolean;
  /** Total count of operations */
  totalCount: number;
  /** Count of enabled operations */
  enabledCount: number;
}

/**
 * Validation error for plan edits.
 * Catches issues like orphaned moves when a folder creation is disabled.
 */
export interface ValidationError {
  /** Type of validation error */
  type: 'orphaned_move' | 'duplicate_destination' | 'missing_folder';
  /** Operation ID that has the error */
  operationId: string;
  /** Human-readable error message */
  message: string;
  /** Related operation ID (e.g., the disabled folder for orphaned_move) */
  relatedOperationId?: string;
}

/**
 * Statistics for the plan edit modal header.
 */
export interface PlanEditStats {
  /** Total number of operations */
  total: number;
  /** Number of enabled operations */
  enabled: number;
  /** Number of disabled operations */
  disabled: number;
  /** Number of modified operations */
  modified: number;
}
