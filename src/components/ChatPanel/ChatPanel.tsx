import { useState, useEffect } from 'react';
import { SquarePen, X } from 'lucide-react';
import { useChatStore, getContextStrategy } from '../../stores/chat-store';
import { MessageList } from './MessageList';
import { ContextStack } from './ContextStack';
import { ChatInput } from './ChatInput';

interface ChatPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

export function ChatPanel({ isOpen, onClose }: ChatPanelProps) {
  const { addContext } = useChatStore();
  const [isDragOver, setIsDragOver] = useState(false);

  // Sync external isOpen prop with store
  useEffect(() => {
    if (isOpen) {
      useChatStore.getState().open();
    } else {
      useChatStore.getState().close();
    }
  }, [isOpen]);

  if (!isOpen) {
    return null;
  }

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    // Check for sentinel custom data or files
    if (
      e.dataTransfer.types.includes('sentinel/path') ||
      e.dataTransfer.types.includes('Files')
    ) {
      e.dataTransfer.dropEffect = 'link';
      setIsDragOver(true);
    }
  };

  const handleDragLeave = (e: React.DragEvent) => {
    // Only set false if we're leaving the panel entirely
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setIsDragOver(false);
    }
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);

    // Handle internal sentinel drag
    const path = e.dataTransfer.getData('sentinel/path');
    if (path) {
      const type = e.dataTransfer.getData('sentinel/type') as 'file' | 'folder';
      const name = e.dataTransfer.getData('sentinel/name') || path.split('/').pop() || 'Unknown';
      const sizeStr = e.dataTransfer.getData('sentinel/size');
      const size = sizeStr ? parseInt(sizeStr, 10) : undefined;
      const mimeType = e.dataTransfer.getData('sentinel/mime') || undefined;

      const strategy = getContextStrategy(type, mimeType);

      addContext({
        type,
        path,
        name,
        strategy,
        size,
        mimeType,
      });
      return;
    }

    // Handle external file drops (from Finder)
    const files = e.dataTransfer.files;
    if (files.length > 0) {
      // Note: For security, browsers don't expose full path for external drops
      // This would need Tauri's file drop handling for full path access
      console.log('[ChatPanel] External file drop - would need Tauri file drop API');
    }
  };

  return (
    <div
      className={`
        relative w-[420px] flex-shrink-0 h-full overflow-hidden flex flex-col
        glass-sidebar
        border-l border-white/5
        transition-all duration-200
        ${isDragOver ? 'ring-2 ring-inset ring-orange-500 bg-orange-50/50 dark:bg-orange-900/20' : ''}
      `}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {/* Top right controls */}
      <div className="absolute top-2 right-2 z-20 flex items-center gap-1">
        <button
          onClick={() => useChatStore.getState().clearHistory()}
          className="p-1.5 rounded-md hover:bg-white/5 text-gray-500 hover:text-gray-300 transition-colors"
          title="New chat"
        >
          <SquarePen size={16} />
        </button>
        <button
          onClick={onClose}
          className="p-1.5 rounded-md hover:bg-white/5 text-gray-500 hover:text-gray-300 transition-colors"
          title="Close chat"
        >
          <X size={16} />
        </button>
      </div>

      {/* Context chips */}
      <ContextStack />

      {/* Drop zone overlay */}
      {isDragOver && (
        <div className="absolute inset-0 flex items-center justify-center bg-orange-500/10 pointer-events-none z-10 m-4 border-2 border-dashed border-orange-500 rounded-lg">
          <div className="text-center">
            <p className="text-orange-600 dark:text-orange-400 font-medium">Drop to add context</p>
            <p className="text-xs text-orange-500/70">Files or folders</p>
          </div>
        </div>
      )}

      {/* Messages */}
      <MessageList />

      {/* Input with inline mention dropdown */}
      <ChatInput />
    </div>
  );
}
