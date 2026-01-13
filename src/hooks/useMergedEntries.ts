import { useMemo } from 'react';
import { useVfsStore } from '../stores/vfs-store';
import { useGhostStore } from '../stores/ghost-store';
import { useOrganizeStore } from '../stores/organize-store';
import type { FileEntry } from '../types/file';
import type { GhostFileEntry, GhostState } from '../types/ghost';
import type { OrganizePhase } from '../stores/organize';

/**
 * Hook that merges real file entries with ghost entries based on the current
 * simulation state.
 *
 * During the simulation phase of organization:
 * - Files being moved away are shown semi-transparent with strikethrough
 * - Destination files are shown with green ghost styling
 * - New folders are shown with pulsing green styling
 * - Files being deleted are shown fading with red styling
 *
 * @param realEntries - The actual file entries from the filesystem
 * @param currentPath - The current directory path
 * @returns Merged entries with ghost state applied
 */
export function useMergedEntries(
  realEntries: FileEntry[],
  currentPath: string
): GhostFileEntry[] {
  const vfsIsActive = useVfsStore((state) => state.isActive);
  const getMergedEntries = useVfsStore((state) => state.getMergedEntries);
  const ghostMap = useGhostStore((state) => state.ghostMap);
  const showGhosts = useGhostStore((state) => state.showGhosts);
  const phase = useOrganizeStore((state) => state.phase) as OrganizePhase;

  return useMemo(() => {
    // If VFS is active and we're in a phase where ghost preview should show, use VFS store for merging
    const showVfsGhosts = phase === 'simulation' || phase === 'review' || phase === 'committing';
    if (vfsIsActive && showVfsGhosts) {
      return getMergedEntries(realEntries, currentPath);
    }

    // If ghosts are disabled or no ghost map, return normal entries
    if (!showGhosts || ghostMap.size === 0) {
      return realEntries.map((entry) => ({
        ...entry,
        ghostState: 'normal' as GhostState,
      }));
    }

    // Stable timestamp for this render pass
    // eslint-disable-next-line react-hooks/purity -- Timestamp captured once per memo computation
    const now = Date.now();

    // Apply ghost states from the ghost map
    const result: GhostFileEntry[] = [];
    const processedPaths = new Set<string>();

    // Process real entries
    for (const entry of realEntries) {
      const ghostInfo = ghostMap.get(entry.path);

      if (ghostInfo) {
        // This entry has a ghost state
        result.push({
          ...entry,
          ghostState: ghostInfo.state,
          operationId: ghostInfo.operationId,
          linkedPath: ghostInfo.linkedPath,
          ghostSince: now,
        });
      } else {
        // Normal entry
        result.push({
          ...entry,
          ghostState: 'normal',
        });
      }

      processedPaths.add(entry.path);
    }

    // Add virtual entries (destinations) that belong in this directory
    for (const [path, ghostInfo] of ghostMap) {
      if (processedPaths.has(path)) continue;
      if (!ghostInfo.isVirtual) continue;

      // Check if this virtual entry belongs in the current directory
      const parentPath = getParentPath(path);
      if (parentPath !== currentPath) continue;

      // Create a virtual file entry
      const name = path.split('/').pop() || '';
      const hasExtension = name.includes('.') && !name.startsWith('.');

      result.push({
        name,
        path,
        isDirectory: !hasExtension,
        isFile: hasExtension,
        isSymlink: false,
        size: 0,
        modifiedAt: now,
        createdAt: now,
        extension: hasExtension ? name.split('.').pop() || null : null,
        mimeType: null,
        isHidden: name.startsWith('.'),
        ghostState: ghostInfo.state,
        operationId: ghostInfo.operationId,
        linkedPath: ghostInfo.linkedPath,
        ghostSince: now,
        isVirtual: true,
      });
    }

    // Sort: directories first, then alphabetically by name
    result.sort((a, b) => {
      if (a.isDirectory && !b.isDirectory) return -1;
      if (!a.isDirectory && b.isDirectory) return 1;
      return a.name.localeCompare(b.name);
    });

    return result;
  }, [realEntries, currentPath, vfsIsActive, getMergedEntries, ghostMap, showGhosts, phase]);
}

/**
 * Extracts the parent directory from a path.
 */
function getParentPath(path: string): string {
  const parts = path.split('/');
  parts.pop();
  return parts.join('/') || '/';
}
