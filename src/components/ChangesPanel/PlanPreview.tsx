import { useState } from 'react';
import {
  FolderPlus,
  FileInput,
  ChevronRight,
  ChevronDown,
  Folder,
  File,
  ArrowRight,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import type { OrganizePlan, OrganizeOperation } from '../../stores/organize-store';

interface PlanPreviewProps {
  plan: OrganizePlan;
  onEditClick?: () => void;
  className?: string;
}

interface FolderSummary {
  path: string;
  name: string;
  fileCount: number;
  files: string[];
}

/**
 * Preview component that shows a high-level summary of planned changes.
 * Displays:
 * - New folders to be created
 * - File distribution by destination folder
 * - Operation counts by type
 */
export function PlanPreview({ plan, onEditClick, className }: PlanPreviewProps) {
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());

  // Analyze operations to build summary
  const analysis = analyzePlan(plan.operations);

  const toggleFolder = (path: string) => {
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  return (
    <div className={cn('space-y-3', className)}>
      {/* Operation Summary */}
      <div className="flex items-center gap-3 text-[10px] text-gray-500">
        {analysis.folderCount > 0 && (
          <span className="flex items-center gap-1">
            <FolderPlus size={10} />
            {analysis.folderCount} new folders
          </span>
        )}
        {analysis.moveCount > 0 && (
          <span className="flex items-center gap-1">
            <FileInput size={10} />
            {analysis.moveCount} files to move
          </span>
        )}
        {analysis.renameCount > 0 && (
          <span className="flex items-center gap-1">
            <ArrowRight size={10} />
            {analysis.renameCount} renames
          </span>
        )}
      </div>

      {/* Folder Structure Preview */}
      <div className="rounded-lg bg-black/20 border border-white/5 overflow-hidden">
        <div className="px-2 py-1.5 border-b border-white/5 bg-white/[0.02] flex items-center justify-between">
          <span className="text-[10px] font-medium text-gray-400 uppercase tracking-wide">
            New Structure
          </span>
          {onEditClick && (
            <button
              onClick={onEditClick}
              className="text-[10px] text-orange-400 hover:text-orange-300 transition-colors"
            >
              Edit Plan
            </button>
          )}
        </div>

        <div className="max-h-48 overflow-y-auto">
          {analysis.folders.length === 0 ? (
            <div className="px-3 py-2 text-xs text-gray-500">
              No folder changes
            </div>
          ) : (
            <div className="py-1">
              {analysis.folders.map((folder) => (
                <FolderPreviewItem
                  key={folder.path}
                  folder={folder}
                  isExpanded={expandedFolders.has(folder.path)}
                  onToggle={() => toggleFolder(folder.path)}
                />
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Top destinations summary */}
      {analysis.topDestinations.length > 0 && (
        <div className="text-[10px] text-gray-500">
          <span className="font-medium text-gray-400">Top destinations: </span>
          {analysis.topDestinations.slice(0, 3).map((dest, i) => (
            <span key={dest.name}>
              {i > 0 && ', '}
              {dest.name} ({dest.count})
            </span>
          ))}
          {analysis.topDestinations.length > 3 && (
            <span>, +{analysis.topDestinations.length - 3} more</span>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Individual folder item in the preview tree
 */
function FolderPreviewItem({
  folder,
  isExpanded,
  onToggle,
}: {
  folder: FolderSummary;
  isExpanded: boolean;
  onToggle: () => void;
}) {
  const hasFiles = folder.files.length > 0;
  const displayFiles = folder.files.slice(0, 5);
  const remainingCount = folder.files.length - 5;

  return (
    <div>
      <button
        onClick={hasFiles ? onToggle : undefined}
        className={cn(
          'w-full flex items-center gap-1.5 px-2 py-1 text-left hover:bg-white/[0.03] transition-colors',
          hasFiles && 'cursor-pointer',
          !hasFiles && 'cursor-default'
        )}
      >
        {hasFiles ? (
          isExpanded ? (
            <ChevronDown size={10} className="text-gray-500 flex-shrink-0" />
          ) : (
            <ChevronRight size={10} className="text-gray-500 flex-shrink-0" />
          )
        ) : (
          <span className="w-2.5" />
        )}
        <Folder size={12} className="text-orange-500/70 flex-shrink-0" />
        <span className="text-xs text-gray-300 truncate flex-1">{folder.name}</span>
        {folder.fileCount > 0 && (
          <span className="text-[10px] text-gray-500 tabular-nums">
            {folder.fileCount} files
          </span>
        )}
      </button>

      {isExpanded && hasFiles && (
        <div className="pl-6 border-l border-white/5 ml-3">
          {displayFiles.map((file, i) => (
            <div
              key={i}
              className="flex items-center gap-1.5 px-2 py-0.5 text-[10px] text-gray-500"
            >
              <File size={10} className="flex-shrink-0 opacity-50" />
              <span className="truncate">{file}</span>
            </div>
          ))}
          {remainingCount > 0 && (
            <div className="px-2 py-0.5 text-[10px] text-gray-600 italic">
              +{remainingCount} more files
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Analyze plan operations to build a summary
 */
function analyzePlan(operations: OrganizeOperation[]) {
  const folderOps = operations.filter((op) => op.type === 'create_folder');
  const moveOps = operations.filter((op) => op.type === 'move');
  const renameOps = operations.filter((op) => op.type === 'rename');

  // Build folder summaries with file counts
  const folderMap = new Map<string, FolderSummary>();

  // First, add all folders to be created
  for (const op of folderOps) {
    if (op.path) {
      const name = op.path.split('/').pop() || op.path;
      folderMap.set(op.path, {
        path: op.path,
        name,
        fileCount: 0,
        files: [],
      });
    }
  }

  // Count files going to each destination folder
  const destCounts = new Map<string, number>();

  for (const op of moveOps) {
    if (op.destination) {
      // Get the destination folder (parent of the destination path)
      const destFolder = op.destination.split('/').slice(0, -1).join('/');
      const fileName = op.source?.split('/').pop() || 'file';

      // Update folder summary if we're tracking it
      const folder = folderMap.get(destFolder);
      if (folder) {
        folder.fileCount++;
        folder.files.push(fileName);
      }

      // Track destination counts
      const folderName = destFolder.split('/').pop() || destFolder;
      destCounts.set(folderName, (destCounts.get(folderName) || 0) + 1);
    }
  }

  // Sort folders by file count (most files first), then by name
  const folders = Array.from(folderMap.values()).sort((a, b) => {
    if (b.fileCount !== a.fileCount) return b.fileCount - a.fileCount;
    return a.name.localeCompare(b.name);
  });

  // Build top destinations list
  const topDestinations = Array.from(destCounts.entries())
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count);

  return {
    folderCount: folderOps.length,
    moveCount: moveOps.length,
    renameCount: renameOps.length,
    folders,
    topDestinations,
  };
}

/**
 * Compact version for smaller spaces
 */
export function PlanPreviewCompact({ plan }: { plan: OrganizePlan }) {
  const analysis = analyzePlan(plan.operations);

  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] text-gray-500">
      {analysis.folderCount > 0 && (
        <span className="flex items-center gap-1">
          <FolderPlus size={10} className="text-orange-500/70" />
          {analysis.folderCount} folders
        </span>
      )}
      {analysis.moveCount > 0 && (
        <span className="flex items-center gap-1">
          <FileInput size={10} className="text-blue-500/70" />
          {analysis.moveCount} moves
        </span>
      )}
      {analysis.topDestinations.length > 0 && (
        <span className="text-gray-600">
          to {analysis.topDestinations[0]?.name}
          {analysis.topDestinations.length > 1 &&
            ` +${analysis.topDestinations.length - 1}`}
        </span>
      )}
    </div>
  );
}
