import { X, MessageSquare, Loader2, AlertCircle, Trash2 } from 'lucide-react';
import { useChatStore, type ChatStatus } from '../../stores/chat-store';

interface ChatHeaderProps {
  onClose: () => void;
}

function StatusIndicator({ status }: { status: ChatStatus }) {
  switch (status) {
    case 'thinking':
      return (
        <div className="flex items-center gap-1.5 text-orange-500">
          <Loader2 size={12} className="animate-spin" />
          <span className="text-xs">Thinking...</span>
        </div>
      );
    case 'streaming':
      return (
        <div className="flex items-center gap-1.5 text-blue-500">
          <div className="w-2 h-2 rounded-full bg-blue-500 animate-pulse" />
          <span className="text-xs">Responding...</span>
        </div>
      );
    case 'error':
      return (
        <div className="flex items-center gap-1.5 text-red-500">
          <AlertCircle size={12} />
          <span className="text-xs">Error</span>
        </div>
      );
    default:
      return null;
  }
}

export function ChatHeader({ onClose }: ChatHeaderProps) {
  const { status, clearHistory, messages } = useChatStore();

  return (
    <div className="flex items-center justify-between px-3 py-2.5 border-b border-white/5">
      <div className="flex items-center gap-2">
        <div className="w-6 h-6 rounded-lg bg-gradient-to-br from-orange-500 to-orange-600 flex items-center justify-center shadow-sm">
          <MessageSquare size={13} className="text-white" />
        </div>
        <span className="text-sm font-medium text-gray-100">Chat</span>
        <StatusIndicator status={status} />
      </div>

      <div className="flex items-center gap-1">
        {/* Clear history button */}
        {messages.length > 0 && (
          <button
            onClick={clearHistory}
            className="p-1.5 rounded-md hover:bg-white/5 text-gray-500 hover:text-gray-300 transition-colors"
            title="Clear chat history"
          >
            <Trash2 size={14} />
          </button>
        )}

        {/* Close button */}
        <button
          onClick={onClose}
          className="p-1.5 rounded-md hover:bg-white/5 text-gray-500 hover:text-gray-300 transition-colors"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
