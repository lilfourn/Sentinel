import { useState, memo, type ReactNode } from 'react';
import {
  File,
  FolderPlus,
  ArrowRight,
  Trash2,
  Copy,
  Edit2,
  Check,
  X,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import { useOrganizeStore } from '../../stores/organize-store';
import type { EditableOperation } from '../../types/plan-edit';

interface OperationEditRowProps {
  operation: EditableOperation;
  searchTerm: string;
}

export const OperationEditRow = memo(function OperationEditRow({
  operation,
  searchTerm,
}: OperationEditRowProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [editValue, setEditValue] = useState('');

  const toggleOperation = useOrganizeStore((s) => s.toggleOperation);
  const updateOperationDestination = useOrganizeStore(
    (s) => s.updateOperationDestination
  );
  const updateOperationNewName = useOrganizeStore((s) => s.updateOperationNewName);

  // Get display info based on operation type
  const { icon, sourceName, destName, canEdit } = getDisplayInfo(operation);

  // Highlight search matches
  const highlightMatch = (text: string): ReactNode => {
    if (!searchTerm) return text;
    const regex = new RegExp(`(${escapeRegex(searchTerm)})`, 'gi');
    const parts = text.split(regex);
    return parts.map((part, i) =>
      regex.test(part) ? (
        <mark
          key={i}
          className="bg-orange-500/30 text-orange-200 rounded px-0.5"
        >
          {part}
        </mark>
      ) : (
        part
      )
    );
  };

  const startEdit = () => {
    if (operation.type === 'rename') {
      setEditValue(operation.newName || sourceName);
    } else if (operation.destination) {
      setEditValue(operation.destination.split('/').pop() || '');
    }
    setIsEditing(true);
  };

  const saveEdit = () => {
    if (operation.type === 'rename') {
      updateOperationNewName(operation.opId, editValue);
    } else if (operation.type === 'move' || operation.type === 'copy') {
      // Reconstruct full destination path
      const destFolder = operation.destination?.split('/').slice(0, -1).join('/');
      const newDest = `${destFolder}/${editValue}`;
      updateOperationDestination(operation.opId, newDest);
    }
    setIsEditing(false);
  };

  const cancelEdit = () => {
    setIsEditing(false);
    setEditValue('');
  };

  return (
    <div
      className={cn(
        'group flex items-center gap-2 px-3 py-2 pl-10',
        'hover:bg-white/[0.02] transition-colors',
        !operation.enabled && 'opacity-50'
      )}
    >
      {/* Checkbox */}
      <input
        type="checkbox"
        checked={operation.enabled}
        onChange={() => toggleOperation(operation.opId)}
        className="w-4 h-4 rounded border-gray-600 text-orange-500 focus:ring-orange-500 focus:ring-offset-0 bg-gray-800 cursor-pointer"
      />

      {/* Icon */}
      <span className="flex-shrink-0 text-gray-500">{icon}</span>

      {/* Source name */}
      <span
        className={cn(
          'text-sm truncate',
          operation.enabled ? 'text-gray-300' : 'text-gray-500 line-through'
        )}
      >
        {highlightMatch(sourceName)}
      </span>

      {/* Arrow and destination for move/copy/rename */}
      {destName !== null && (
        <>
          <ArrowRight size={12} className="text-gray-600 flex-shrink-0" />

          {isEditing ? (
            <div className="flex items-center gap-1 flex-1 min-w-0">
              <input
                type="text"
                value={editValue}
                onChange={(e) => setEditValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') saveEdit();
                  if (e.key === 'Escape') cancelEdit();
                }}
                className="flex-1 min-w-0 px-2 py-0.5 text-sm bg-gray-800 border border-orange-500/50 rounded text-gray-200 focus:outline-none focus:border-orange-500"
                autoFocus
              />
              <button
                onClick={saveEdit}
                className="p-1 rounded hover:bg-green-500/20 text-green-400"
              >
                <Check size={12} />
              </button>
              <button
                onClick={cancelEdit}
                className="p-1 rounded hover:bg-red-500/20 text-red-400"
              >
                <X size={12} />
              </button>
            </div>
          ) : (
            <span
              className={cn(
                'text-sm truncate flex-1',
                operation.enabled ? 'text-gray-400' : 'text-gray-600'
              )}
            >
              {highlightMatch(destName)}
            </span>
          )}
        </>
      )}

      {/* Edit button */}
      {operation.enabled && canEdit && !isEditing && (
        <button
          onClick={startEdit}
          className="p-1 rounded hover:bg-white/10 text-gray-500 hover:text-gray-300 opacity-0 group-hover:opacity-100 transition-opacity"
          title="Edit name"
        >
          <Edit2 size={12} />
        </button>
      )}

      {/* Modified indicator */}
      {operation.isModified && (
        <span
          className="w-1.5 h-1.5 rounded-full bg-orange-500 flex-shrink-0"
          title="Modified"
        />
      )}
    </div>
  );
});

function getDisplayInfo(op: EditableOperation): {
  icon: ReactNode;
  sourceName: string;
  destName: string | null;
  canEdit: boolean;
} {
  switch (op.type) {
    case 'create_folder':
      return {
        icon: <FolderPlus size={14} className="text-orange-400" />,
        sourceName: op.path?.split('/').pop() || '',
        destName: null,
        canEdit: false,
      };
    case 'move':
      return {
        icon: <File size={14} />,
        sourceName: op.source?.split('/').pop() || '',
        destName: op.destination?.split('/').pop() || '',
        canEdit: true,
      };
    case 'rename':
      return {
        icon: <File size={14} />,
        sourceName: op.path?.split('/').pop() || '',
        destName: op.newName || '',
        canEdit: true,
      };
    case 'trash':
      return {
        icon: <Trash2 size={14} className="text-red-400" />,
        sourceName: op.path?.split('/').pop() || '',
        destName: null,
        canEdit: false,
      };
    case 'copy':
      return {
        icon: <Copy size={14} />,
        sourceName: op.source?.split('/').pop() || '',
        destName: op.destination?.split('/').pop() || '',
        canEdit: true,
      };
  }
}

function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
