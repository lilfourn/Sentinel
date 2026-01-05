import type { EditableOperation, ValidationError, OperationGroup } from '../../types/plan-edit';

/**
 * Validate plan edits and return any errors.
 * Key validation: if a create_folder is disabled, all moves to that folder should be flagged.
 */
export function validatePlanEdits(operations: EditableOperation[]): ValidationError[] {
  const errors: ValidationError[] = [];

  // Build set of enabled and disabled folder creates
  const enabledFolders = new Set<string>();
  const disabledFolders = new Map<string, string>(); // path -> opId

  for (const op of operations) {
    if (op.type === 'create_folder' && op.path) {
      if (op.enabled) {
        enabledFolders.add(op.path);
      } else {
        disabledFolders.set(op.path, op.opId);
      }
    }
  }

  // Check for orphaned moves (enabled moves targeting disabled folder creates)
  for (const op of operations) {
    if (op.enabled && (op.type === 'move' || op.type === 'copy') && op.destination) {
      const destFolder = op.destination.split('/').slice(0, -1).join('/');

      if (disabledFolders.has(destFolder)) {
        const fileName = op.source?.split('/').pop() || 'file';
        errors.push({
          type: 'orphaned_move',
          operationId: op.opId,
          message: `"${fileName}" targets folder that won't be created`,
          relatedOperationId: disabledFolders.get(destFolder),
        });
      }
    }
  }

  // Check for duplicate destinations (multiple enabled ops targeting same path)
  const destinations = new Map<string, string[]>(); // destination -> [opIds]
  for (const op of operations) {
    if (op.enabled && op.destination) {
      const existing = destinations.get(op.destination) || [];
      existing.push(op.opId);
      destinations.set(op.destination, existing);
    }
  }

  for (const [dest, opIds] of destinations) {
    if (opIds.length > 1) {
      const destName = dest.split('/').pop() || dest;
      for (const opId of opIds) {
        errors.push({
          type: 'duplicate_destination',
          operationId: opId,
          message: `Multiple files targeting "${destName}"`,
        });
      }
    }
  }

  return errors;
}

/**
 * Build operation groups from a flat list of editable operations.
 * Groups operations by their target/destination folder.
 */
export function buildOperationGroups(operations: EditableOperation[]): OperationGroup[] {
  const groupMap = new Map<string, OperationGroup>();

  for (const op of operations) {
    let targetFolder: string | undefined;

    if (op.type === 'create_folder') {
      targetFolder = op.path;
    } else if (op.type === 'move' || op.type === 'copy') {
      targetFolder = op.destination?.split('/').slice(0, -1).join('/');
    } else if (op.type === 'rename') {
      targetFolder = op.path?.split('/').slice(0, -1).join('/');
    } else if (op.type === 'trash') {
      targetFolder = op.path?.split('/').slice(0, -1).join('/');
    }

    if (!targetFolder) continue;

    if (!groupMap.has(targetFolder)) {
      groupMap.set(targetFolder, {
        groupId: `group-${targetFolder}`,
        targetFolder,
        displayName: targetFolder.split('/').pop() || targetFolder,
        operations: [],
        allEnabled: true,
        partialEnabled: false,
        totalCount: 0,
        enabledCount: 0,
      });
    }

    const group = groupMap.get(targetFolder)!;
    group.operations.push(op);
    group.totalCount++;
    if (op.enabled) group.enabledCount++;
  }

  // Calculate enabled states for each group
  for (const group of groupMap.values()) {
    group.allEnabled = group.enabledCount === group.totalCount;
    group.partialEnabled = group.enabledCount > 0 && group.enabledCount < group.totalCount;
  }

  // Sort groups: those with create_folder ops first, then by file count
  return Array.from(groupMap.values()).sort((a, b) => {
    // Groups with create_folder first
    const aHasCreate = a.operations.some((op) => op.type === 'create_folder');
    const bHasCreate = b.operations.some((op) => op.type === 'create_folder');
    if (aHasCreate && !bHasCreate) return -1;
    if (!aHasCreate && bHasCreate) return 1;

    // Then by file count (descending)
    if (b.totalCount !== a.totalCount) return b.totalCount - a.totalCount;

    // Then alphabetically
    return a.displayName.localeCompare(b.displayName);
  });
}

/**
 * Filter operations based on search term.
 * Searches file names and folder names.
 */
export function filterOperationsBySearch(
  operations: EditableOperation[],
  searchTerm: string
): EditableOperation[] {
  if (!searchTerm.trim()) return operations;

  const term = searchTerm.toLowerCase();

  return operations.filter((op) => {
    const sourceName = op.source?.split('/').pop()?.toLowerCase() || '';
    const destName = op.destination?.split('/').pop()?.toLowerCase() || '';
    const pathName = op.path?.split('/').pop()?.toLowerCase() || '';
    const newName = op.newName?.toLowerCase() || '';

    return (
      sourceName.includes(term) ||
      destName.includes(term) ||
      pathName.includes(term) ||
      newName.includes(term)
    );
  });
}

/**
 * Get display info for an operation.
 */
export function getOperationDisplayInfo(op: EditableOperation): {
  icon: 'folder-plus' | 'file-input' | 'arrow-right' | 'trash' | 'copy';
  sourceName: string;
  destName: string | null;
  typeLabel: string;
} {
  switch (op.type) {
    case 'create_folder':
      return {
        icon: 'folder-plus',
        sourceName: op.path?.split('/').pop() || '',
        destName: null,
        typeLabel: 'Create folder',
      };
    case 'move':
      return {
        icon: 'file-input',
        sourceName: op.source?.split('/').pop() || '',
        destName: op.destination?.split('/').pop() || null,
        typeLabel: 'Move',
      };
    case 'rename':
      return {
        icon: 'arrow-right',
        sourceName: op.path?.split('/').pop() || '',
        destName: op.newName || null,
        typeLabel: 'Rename',
      };
    case 'trash':
      return {
        icon: 'trash',
        sourceName: op.path?.split('/').pop() || '',
        destName: null,
        typeLabel: 'Delete',
      };
    case 'copy':
      return {
        icon: 'copy',
        sourceName: op.source?.split('/').pop() || '',
        destName: op.destination?.split('/').pop() || null,
        typeLabel: 'Copy',
      };
  }
}
