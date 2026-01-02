import { useState, memo } from 'react';
import Markdown from 'react-markdown';
import { Copy, ThumbsUp, ThumbsDown, RotateCcw, Brain, ChevronRight, File, Folder, Image } from 'lucide-react';
import type { ChatMessage, ContextItem } from '../../stores/chat-store';
import { ThoughtAccordion } from './ThoughtAccordion';
import { StreamingIndicator, ThinkingDots, ShimmerText } from './StreamingIndicator';

// Allowed URL protocols to prevent XSS via javascript: URLs
const ALLOWED_URL_PROTOCOLS = ['http:', 'https:', 'mailto:', 'tel:'];

/**
 * Sanitize URL to prevent XSS attacks
 * Returns the URL if safe, undefined if dangerous
 */
function sanitizeUrl(url: string | undefined): string | undefined {
  if (!url) return undefined;

  const trimmed = url.trim();

  // Allow relative URLs (no protocol)
  if (!trimmed.includes(':')) return url;

  // Check against allowlist
  const lowerUrl = trimmed.toLowerCase();
  for (const protocol of ALLOWED_URL_PROTOCOLS) {
    if (lowerUrl.startsWith(protocol)) {
      return url;
    }
  }

  // Block dangerous protocols (javascript:, data:, vbscript:, etc.)
  return undefined;
}

interface MessageItemProps {
  message: ChatMessage;
}

/**
 * Custom comparison function for React.memo
 * Only re-render if meaningful message properties change
 */
function arePropsEqual(prev: MessageItemProps, next: MessageItemProps): boolean {
  const p = prev.message;
  const n = next.message;

  return (
    p.id === n.id &&
    p.content === n.content &&
    p.thinking === n.thinking &&
    p.isStreaming === n.isStreaming &&
    p.isThinking === n.isThinking &&
    p.thoughts?.length === n.thoughts?.length &&
    p.contextItems?.length === n.contextItems?.length &&
    // Check last thought update (for streaming thoughts)
    p.thoughts?.[p.thoughts.length - 1]?.output === n.thoughts?.[n.thoughts.length - 1]?.output
  );
}

/**
 * Display attached files/folders for a message
 */
function AttachmentChips({ items }: { items: ContextItem[] }) {
  if (!items || items.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-1 mt-1.5 justify-end">
      {items.map((item) => {
        const Icon = item.type === 'folder' ? Folder : item.type === 'image' ? Image : File;
        const iconColor = item.type === 'folder'
          ? 'text-blue-400'
          : item.type === 'image'
            ? 'text-purple-400'
            : 'text-gray-400';

        return (
          <div
            key={item.id}
            className="flex items-center gap-1 px-2 py-0.5 bg-white/5 rounded text-[11px] text-gray-400"
            title={item.path}
          >
            <Icon size={10} className={iconColor} />
            <span className="max-w-32 truncate">{item.name}</span>
          </div>
        );
      })}
    </div>
  );
}

export const MessageItem = memo(function MessageItem({ message }: MessageItemProps) {
  const isUser = message.role === 'user';
  const isAssistant = message.role === 'assistant';
  const [isThinkingExpanded, setIsThinkingExpanded] = useState(false);

  const handleCopy = () => {
    if (message.content) {
      navigator.clipboard.writeText(message.content);
    }
  };

  const hasThinking = message.thinking && message.thinking.length > 0;

  if (isUser) {
    return (
      <div className="flex flex-col items-end">
        {/* User message with dark pill background */}
        <div className="inline-block max-w-[85%] text-sm rounded-2xl px-4 py-2.5 bg-[#1e1e1e] text-gray-100">
          {message.content}
        </div>
        {/* Attached files */}
        {message.contextItems && message.contextItems.length > 0 && (
          <AttachmentChips items={message.contextItems} />
        )}
        {/* Timestamp */}
        <div className="text-[11px] text-gray-500 mt-1.5 mr-1">
          {new Date(message.timestamp).toLocaleTimeString([], {
            hour: 'numeric',
            minute: '2-digit',
            hour12: true,
          })}
        </div>
      </div>
    );
  }

  // Assistant message
  return (
    <div className="flex flex-col items-start">
      {/* Extended thinking indicator/accordion */}
      {isAssistant && (message.isThinking || hasThinking) && (
        <div className="mb-3 w-full">
          <button
            onClick={() => !message.isThinking && setIsThinkingExpanded(!isThinkingExpanded)}
            className={`
              flex items-center gap-2 text-xs transition-colors
              ${message.isThinking
                ? 'text-purple-400 cursor-default'
                : 'text-purple-400 hover:text-purple-300 cursor-pointer'
              }
            `}
            disabled={message.isThinking}
          >
            {message.isThinking ? (
              <>
                <Brain size={12} className="thinking-pulse" />
                <span>Thinking</span>
                <ThinkingDots />
              </>
            ) : (
              <>
                <span className="transition-transform duration-200" style={{ transform: isThinkingExpanded ? 'rotate(90deg)' : 'rotate(0deg)' }}>
                  <ChevronRight size={12} />
                </span>
                <Brain size={12} />
                <span>Extended thinking</span>
                <span className="text-purple-500/60 ml-1">
                  ({message.thinking?.length.toLocaleString()} chars)
                </span>
              </>
            )}
          </button>

          {/* Expanded thinking content with smooth transition */}
          <div
            className={`
              overflow-hidden transition-all duration-300 ease-out
              ${isThinkingExpanded && hasThinking ? 'max-h-64 opacity-100 mt-2' : 'max-h-0 opacity-0'}
            `}
          >
            <div className="p-3 bg-purple-900/20 border border-purple-500/20 rounded-lg">
              <pre className="text-xs text-purple-200/80 whitespace-pre-wrap font-mono overflow-auto max-h-56">
                {message.thinking}
              </pre>
            </div>
          </div>
        </div>
      )}

      {/* Thoughts (tool usage) accordion - shown above response */}
      {isAssistant && message.thoughts && message.thoughts.length > 0 && (
        <div className="mb-2 w-full">
          <ThoughtAccordion thoughts={message.thoughts} />
        </div>
      )}

      {/* Assistant message - plain text, no bubble */}
      <div className="max-w-full text-sm text-gray-100">
        <StreamingIndicator isStreaming={!!message.isStreaming && !!message.content}>
          <div className="prose prose-sm prose-invert max-w-none break-words">
            {message.content ? (
              <Markdown
                components={{
                  // Customize link styling with XSS protection
                  a: ({ href, children }) => {
                    const safeHref = sanitizeUrl(href);
                    if (!safeHref) {
                      // Render as plain text if URL is blocked
                      return <span className="text-gray-400">{children}</span>;
                    }
                    return (
                      <a
                        href={safeHref}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-blue-400 hover:underline"
                      >
                        {children}
                      </a>
                    );
                  },
                  // Code blocks
                  code: ({ className, children }) => {
                    const isInline = !className;
                    return isInline ? (
                      <code className="bg-white/10 px-1.5 py-0.5 rounded text-xs">
                        {children}
                      </code>
                    ) : (
                      <code className="block bg-white/5 p-3 rounded-lg text-xs overflow-x-auto">
                        {children}
                      </code>
                    );
                  },
                  // Paragraphs
                  p: ({ children }) => <p className="mb-3 last:mb-0 leading-relaxed">{children}</p>,
                  // Lists
                  ul: ({ children }) => <ul className="list-disc pl-4 mb-3 space-y-1">{children}</ul>,
                  ol: ({ children }) => <ol className="list-decimal pl-4 mb-3 space-y-1">{children}</ol>,
                }}
              >
                {message.content}
              </Markdown>
            ) : message.isStreaming ? (
              <ShimmerText text="Thinking..." />
            ) : null}
          </div>
        </StreamingIndicator>
      </div>

      {/* Action buttons for assistant messages */}
      {!message.isStreaming && message.content && (
        <div className="flex items-center gap-1 mt-2">
          <button
            onClick={handleCopy}
            className="p-1.5 rounded hover:bg-white/5 text-gray-500 hover:text-gray-300 transition-colors"
            title="Copy"
          >
            <Copy size={14} />
          </button>
          <button
            disabled
            className="p-1.5 rounded text-gray-600 cursor-not-allowed opacity-50"
            title="Coming soon"
          >
            <ThumbsUp size={14} />
          </button>
          <button
            disabled
            className="p-1.5 rounded text-gray-600 cursor-not-allowed opacity-50"
            title="Coming soon"
          >
            <ThumbsDown size={14} />
          </button>
          <button
            disabled
            className="p-1.5 rounded text-gray-600 cursor-not-allowed opacity-50"
            title="Coming soon"
          >
            <RotateCcw size={14} />
          </button>
        </div>
      )}
    </div>
  );
}, arePropsEqual);
