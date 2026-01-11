import { X, File, Folder, Image } from 'lucide-react';
import { useChatStore, formatSize, MAX_CONTEXT_ITEMS } from '../../stores/chat-store';
import type { ContextItem } from '../../stores/chat-store';

function ContextChip({ item, onRemove }: { item: ContextItem; onRemove: () => void }) {
  const Icon = item.type === 'folder'
    ? Folder
    : item.type === 'image'
      ? Image
      : File;

  const strategyLabel = item.strategy === 'hologram'
    ? 'summary'
    : item.strategy === 'vision'
      ? 'image'
      : 'text';

  return (
    <div
      className="flex items-center gap-1.5 px-2 py-1 bg-white/5 rounded-full text-xs group text-gray-300"
      title={`${item.path} (${strategyLabel})`}
    >
      <Icon size={12} className={
        item.type === 'folder'
          ? 'text-blue-400'
          : item.type === 'image'
            ? 'text-purple-400'
            : 'text-gray-400'
      } />
      <span className="max-w-24 truncate">{item.name}</span>
      {item.size !== undefined && (
        <span className="text-gray-500 text-[10px]">
          {formatSize(item.size)}
        </span>
      )}
      <button
        onClick={(e) => {
          e.stopPropagation();
          onRemove();
        }}
        className="p-0.5 rounded-full hover:bg-white/10 opacity-0 group-hover:opacity-100 transition-opacity"
      >
        <X size={10} />
      </button>
    </div>
  );
}

export function ContextStack() {
  // Use individual selectors to prevent unnecessary re-renders
  const activeContext = useChatStore((s) => s.activeContext);
  const removeContext = useChatStore((s) => s.removeContext);
  const clearContext = useChatStore((s) => s.clearContext);

  if (activeContext.length === 0) return null;

  return (
    <div className="px-3 py-2 pt-10 border-b border-white/5 bg-white/[0.02]">
      <div className="flex items-center justify-between mb-1.5">
        <span className="text-[10px] uppercase tracking-wider text-gray-500">
          Context{' '}
          <span className={activeContext.length >= MAX_CONTEXT_ITEMS ? 'text-orange-400' : ''}>
            ({activeContext.length}/{MAX_CONTEXT_ITEMS})
          </span>
        </span>
        {activeContext.length > 1 && (
          <button
            onClick={clearContext}
            className="text-[10px] text-gray-500 hover:text-gray-300 transition-colors"
          >
            Clear all
          </button>
        )}
      </div>
      <div className="flex flex-wrap gap-1.5">
        {activeContext.map((item) => (
          <ContextChip
            key={item.id}
            item={item}
            onRemove={() => removeContext(item.id)}
          />
        ))}
      </div>
    </div>
  );
}
