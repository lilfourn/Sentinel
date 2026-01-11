import { useEffect, useRef, useCallback, Component, type ReactNode } from 'react';
import { MessageSquare, AlertTriangle } from 'lucide-react';
import { useChatStore } from '../../stores/chat-store';
import { useShallow } from 'zustand/react/shallow';
import { MessageItem } from './MessageItem';

// Threshold in pixels - if user is within this distance from bottom, auto-scroll
const SCROLL_THRESHOLD_PX = 100;

/**
 * Error boundary to catch rendering errors in the message list
 * Prevents the entire chat from crashing if a message fails to render
 */
interface ErrorBoundaryState {
  hasError: boolean;
  error?: Error;
}

class MessageListErrorBoundary extends Component<
  { children: ReactNode },
  ErrorBoundaryState
> {
  constructor(props: { children: ReactNode }) {
    super(props);
    this.state = { hasError: false };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('[MessageList] Render error:', error, errorInfo);
  }

  handleRetry = () => {
    this.setState({ hasError: false, error: undefined });
  };

  render() {
    if (this.state.hasError) {
      return (
        <div className="flex-1 flex flex-col items-center justify-center p-6 text-center">
          <div className="w-12 h-12 rounded-full bg-red-500/20 flex items-center justify-center mb-3">
            <AlertTriangle size={24} className="text-red-400" />
          </div>
          <h4 className="text-sm font-medium text-gray-100 mb-1">
            Something went wrong
          </h4>
          <p className="text-xs text-gray-400 max-w-xs mb-4">
            An error occurred while rendering messages.
          </p>
          <button
            onClick={this.handleRetry}
            className="px-4 py-2 text-xs bg-white/10 hover:bg-white/20 rounded-lg text-gray-200 transition-colors"
          >
            Try again
          </button>
          {this.state.error && (
            <p className="mt-3 text-[10px] text-gray-500 font-mono max-w-xs truncate">
              {this.state.error.message}
            </p>
          )}
        </div>
      );
    }

    return this.props.children;
  }
}

export function MessageList() {
  // Use selector to only subscribe to messages array, preventing re-renders on other state changes
  const messages = useChatStore(useShallow((state) => state.messages));
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  // Track if user is near the bottom (should auto-scroll)
  const isNearBottomRef = useRef(true);

  // Handle scroll to track if user is near bottom
  const handleScroll = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;

    const distanceFromBottom =
      container.scrollHeight - container.scrollTop - container.clientHeight;
    isNearBottomRef.current = distanceFromBottom < SCROLL_THRESHOLD_PX;
  }, []);

  // Attach scroll listener
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    container.addEventListener('scroll', handleScroll, { passive: true });
    return () => container.removeEventListener('scroll', handleScroll);
  }, [handleScroll]);

  // Auto-scroll to bottom only when near bottom
  useEffect(() => {
    if (isNearBottomRef.current && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages]);

  if (messages.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-6 pt-10 text-center select-none">
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
    <MessageListErrorBoundary>
      <div
        ref={containerRef}
        className="flex-1 overflow-y-auto p-4 pt-14 space-y-6"
      >
        {messages.map((message) => (
          <MessageItem key={message.id} message={message} />
        ))}
        <div ref={bottomRef} />
      </div>
    </MessageListErrorBoundary>
  );
}
