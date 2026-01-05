/**
 * Types for organization history and multi-level undo.
 */

/**
 * Summary of folder history for quick display
 */
export interface HistorySummary {
  folderPath: string;
  sessionCount: number;
  totalOperations: number;
  lastOrganized: string; // ISO date string
}

/**
 * Lightweight session summary for listing
 */
export interface SessionSummary {
  sessionId: string;
  userInstruction: string;
  planDescription: string;
  executedAt: string; // ISO date string
  filesAffected: number;
  operationCount: number;
  undone: boolean;
}

/**
 * Operation record types
 */
export type OperationType =
  | 'create_folder'
  | 'move'
  | 'rename'
  | 'quarantine'
  | 'copy'
  | 'delete_folder';

/**
 * File checksum for integrity verification
 */
export interface FileChecksum {
  sha256: string;
  size: number;
  mtime: number;
  isDirectory: boolean;
}

/**
 * Single operation with checksums
 */
export interface HistoryOperation {
  id: string;
  sequence: number;
  operation: OperationRecord;
  undoOperation: OperationRecord;
  sourceChecksums: Record<string, FileChecksum>;
  resultChecksums: Record<string, FileChecksum>;
}

/**
 * Operation record (discriminated union)
 */
export type OperationRecord =
  | { type: 'create_folder'; path: string }
  | { type: 'move'; source: string; destination: string }
  | { type: 'rename'; path: string; newName: string }
  | { type: 'quarantine'; path: string; quarantinePath: string }
  | { type: 'copy'; source: string; destination: string }
  | { type: 'delete_folder'; path: string };

/**
 * Full session with all operations
 */
export interface HistorySession {
  sessionId: string;
  userInstruction: string;
  planDescription: string;
  executedAt: string;
  targetFolder: string;
  operations: HistoryOperation[];
  filesAffected: number;
  undone: boolean;
}

/**
 * Conflict types during undo
 */
export type ConflictType = 'modified' | 'deleted' | 'blocking';

/**
 * Information about a file conflict
 */
export interface ConflictInfo {
  path: string;
  expectedSha256: string;
  currentSha256: string | null;
  conflictType: ConflictType;
}

/**
 * Result of preflight check before undo
 */
export interface UndoPreflightResult {
  canProceed: boolean;
  modifiedFiles: ConflictInfo[];
  missingFiles: string[];
  blockingFiles: string[];
  safeOperations: number;
  conflictedOperations: number;
  totalOperations: number;
}

/**
 * How to resolve conflicts during undo
 */
export type ConflictResolution = 'abort' | 'skip' | 'force' | 'backup';

/**
 * Result of undo execution
 */
export interface UndoResult {
  success: boolean;
  operationsUndone: number;
  operationsSkipped: number;
  errors: string[];
}

/**
 * Folder index entry (from global index)
 */
export interface FolderIndexEntry {
  folderPath: string;
  folderHash: string;
  sessionCount: number;
  lastOrganized: string;
}
