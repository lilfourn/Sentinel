import { useEffect, useRef } from 'react';
import { MessageSquare } from 'lucide-react';
import { useChatStore } from '../../stores/chat-store';
import { MessageItem } from './MessageItem';

export function MessageList() {
  const { messages } = useChatStore();
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when new messages arrive or content updates
  useEffect(() => {
    if (bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages]);

  if (messages.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-6 pt-10 text-center">
        <div className="w-12 h-12 rounded-full bg-orange-500/20 flex items-center justify-center mb-3">
          <MessageSquare size={24} className="text-orange-500" />
        </div>
        <h4 className="text-sm font-medium text-gray-100 mb-1">
          Sentinel Chat
        </h4>
        <p className="text-xs text-gray-400 max-w-xs">
          Ask questions about your files, search semantically, or drag folders here for context.
        </p>
        <div className="mt-4 space-y-1 text-xs text-gray-500">
          <p>Try asking:</p>
          <p className="italic text-gray-400">"Find all tax documents from 2024"</p>
          <p className="italic text-gray-400">"What's in the Downloads folder?"</p>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="flex-1 overflow-y-auto p-3 pt-10 space-y-4"
    >
      {messages.map((message) => (
        <MessageItem key={message.id} message={message} />
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
