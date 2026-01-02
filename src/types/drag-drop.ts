import type { FileEntry } from './file';

/** Represents the current drag operation state */
export interface DragState {
  /** Items being dragged */
  items: FileEntry[];
  /** Source directory path */
  sourceDirectory: string;
  /** Whether Alt/Option is held (copy mode) */
  isCopy: boolean;
  /** Current mouse position */
  position: { x: number; y: number };
}

/** Represents a drop target */
export interface DropTarget {
  path: string;
  isValid: boolean;
  reason?: DropInvalidReason;
}

/** Reasons why a drop target is invalid */
export type DropInvalidReason =
  | 'cycle_self' // Dropping into itself
  | 'cycle_descendant' // Dropping into a descendant
  | 'target_selected' // Target is one of the dragged items
  | 'name_collision' // File already exists at destination
  | 'permission_denied' // No write permission
  | 'not_directory' // Target is not a directory
  | 'protected_path' // System protected path
  | 'symlink_loop'; // Symlink loop detected

/** Backend drag-drop error structure (matches Rust DragDropError) */
export interface DragDropError {
  type:
    | 'CYCLE_DETECTED_SELF'
    | 'CYCLE_DETECTED_DESCENDANT'
    | 'TARGET_IS_SELECTED'
    | 'NAME_COLLISION'
    | 'PERMISSION_DENIED'
    | 'SOURCE_NOT_FOUND'
    | 'TARGET_NOT_DIRECTORY'
    | 'PROTECTED_PATH'
    | 'SYMLINK_LOOP'
    | 'IO_ERROR';
  path?: string;
  source?: string;
  target?: string;
  name?: string;
  destination?: string;
  message?: string;
}

/** Map backend error type to frontend reason */
export function mapErrorToReason(
  errorType?: string
): DropInvalidReason | undefined {
  const mapping: Record<string, DropInvalidReason> = {
    CYCLE_DETECTED_SELF: 'cycle_self',
    CYCLE_DETECTED_DESCENDANT: 'cycle_descendant',
    TARGET_IS_SELECTED: 'target_selected',
    NAME_COLLISION: 'name_collision',
    PERMISSION_DENIED: 'permission_denied',
    TARGET_NOT_DIRECTORY: 'not_directory',
    PROTECTED_PATH: 'protected_path',
    SYMLINK_LOOP: 'symlink_loop',
  };
  return errorType ? mapping[errorType] : undefined;
}

/** Human-readable messages for drop invalid reasons */
export const DROP_INVALID_MESSAGES: Record<DropInvalidReason, string> = {
  cycle_self: 'Cannot drop folder into itself',
  cycle_descendant: 'Cannot drop folder into its own subfolder',
  target_selected: 'Cannot drop into a selected item',
  name_collision: 'Item with same name already exists',
  permission_denied: 'Permission denied',
  not_directory: 'Can only drop into folders',
  protected_path: 'Cannot modify protected folder',
  symlink_loop: 'Cannot drop: symlink loop detected',
};
