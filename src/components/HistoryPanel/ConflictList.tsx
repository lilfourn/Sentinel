/**
 * ConflictList - Displays file conflicts detected during undo preflight.
 */

import { AlertTriangle, FileX, FileWarning, Ban } from 'lucide-react';
import type { ConflictInfo } from '../../types/history';

interface ConflictListProps {
  modifiedFiles: ConflictInfo[];
  missingFiles: string[];
  blockingFiles: string[];
}

export function ConflictList({
  modifiedFiles,
  missingFiles,
  blockingFiles,
}: ConflictListProps) {
  const hasConflicts =
    modifiedFiles.length > 0 ||
    missingFiles.length > 0 ||
    blockingFiles.length > 0;

  if (!hasConflicts) {
    return null;
  }

  return (
    <div className="space-y-3">
      {/* Modified files */}
      {modifiedFiles.length > 0 && (
        <div className="rounded-lg border border-amber-500/20 bg-amber-500/5 p-3">
          <div className="flex items-center gap-2 text-amber-600 dark:text-amber-400 text-sm font-medium mb-2">
            <FileWarning size={14} />
            <span>Modified Files ({modifiedFiles.length})</span>
          </div>
          <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">
            These files have been changed since organization
          </p>
          <ul className="space-y-1 max-h-24 overflow-y-auto">
            {modifiedFiles.map((conflict, i) => (
              <li
                key={i}
                className="text-xs text-gray-700 dark:text-gray-300 truncate flex items-center gap-1.5"
              >
                <AlertTriangle size={10} className="text-amber-500 dark:text-amber-400 flex-shrink-0" />
                {conflict.path.split('/').pop()}
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Missing files */}
      {missingFiles.length > 0 && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-3">
          <div className="flex items-center gap-2 text-red-600 dark:text-red-400 text-sm font-medium mb-2">
            <FileX size={14} />
            <span>Missing Files ({missingFiles.length})</span>
          </div>
          <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">
            These files have been deleted and cannot be restored
          </p>
          <ul className="space-y-1 max-h-24 overflow-y-auto">
            {missingFiles.map((path, i) => (
              <li
                key={i}
                className="text-xs text-gray-700 dark:text-gray-300 truncate flex items-center gap-1.5"
              >
                <FileX size={10} className="text-red-500 dark:text-red-400 flex-shrink-0" />
                {path.split('/').pop()}
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Blocking files */}
      {blockingFiles.length > 0 && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-3">
          <div className="flex items-center gap-2 text-red-600 dark:text-red-400 text-sm font-medium mb-2">
            <Ban size={14} />
            <span>Blocking Files ({blockingFiles.length})</span>
          </div>
          <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">
            Files exist at original locations and would be overwritten
          </p>
          <ul className="space-y-1 max-h-24 overflow-y-auto">
            {blockingFiles.map((path, i) => (
              <li
                key={i}
                className="text-xs text-gray-700 dark:text-gray-300 truncate flex items-center gap-1.5"
              >
                <Ban size={10} className="text-red-500 dark:text-red-400 flex-shrink-0" />
                {path.split('/').pop()}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
