import Markdown from 'react-markdown';
import { User, Bot } from 'lucide-react';
import type { ChatMessage } from '../../stores/chat-store';
import { ThoughtAccordion } from './ThoughtAccordion';

interface MessageItemProps {
  message: ChatMessage;
}

export function MessageItem({ message }: MessageItemProps) {
  const isUser = message.role === 'user';
  const isAssistant = message.role === 'assistant';

  return (
    <div className={`flex gap-3 ${isUser ? 'flex-row-reverse' : ''}`}>
      {/* Avatar */}
      <div
        className={`flex-shrink-0 w-7 h-7 rounded-full flex items-center justify-center ${
          isUser
            ? 'bg-blue-500/20'
            : 'bg-orange-500/20'
        }`}
      >
        {isUser ? (
          <User size={14} className="text-blue-400" />
        ) : (
          <Bot size={14} className="text-orange-400" />
        )}
      </div>

      {/* Content */}
      <div className={`flex-1 min-w-0 ${isUser ? 'text-right' : ''}`}>
        <div
          className={`inline-block max-w-full text-sm rounded-lg px-3 py-2 ${
            isUser
              ? 'bg-blue-500/80 text-white'
              : 'bg-white/5 text-gray-100'
          }`}
        >
          {/* Message content with markdown */}
          <div className="prose prose-sm dark:prose-invert max-w-none break-words">
            {message.content ? (
              <Markdown
                components={{
                  // Customize link styling
                  a: ({ href, children }) => (
                    <a
                      href={href}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-blue-400 hover:underline"
                    >
                      {children}
                    </a>
                  ),
                  // Code blocks
                  code: ({ className, children }) => {
                    const isInline = !className;
                    return isInline ? (
                      <code className="bg-black/30 px-1 rounded text-xs">
                        {children}
                      </code>
                    ) : (
                      <code className="block bg-black/30 p-2 rounded text-xs overflow-x-auto">
                        {children}
                      </code>
                    );
                  },
                  // Paragraphs
                  p: ({ children }) => <p className="mb-2 last:mb-0">{children}</p>,
                  // Lists
                  ul: ({ children }) => <ul className="list-disc pl-4 mb-2">{children}</ul>,
                  ol: ({ children }) => <ol className="list-decimal pl-4 mb-2">{children}</ol>,
                }}
              >
                {message.content}
              </Markdown>
            ) : message.isStreaming ? (
              <span className="inline-block w-2 h-4 bg-current animate-pulse" />
            ) : null}
          </div>

          {/* Streaming cursor */}
          {message.isStreaming && message.content && (
            <span className="inline-block w-2 h-4 bg-current animate-pulse ml-0.5" />
          )}
        </div>

        {/* Thoughts (tool usage) for assistant messages */}
        {isAssistant && message.thoughts && message.thoughts.length > 0 && (
          <ThoughtAccordion thoughts={message.thoughts} />
        )}

        {/* Timestamp */}
        <div className={`text-[10px] text-gray-400 mt-1 ${isUser ? 'text-right' : ''}`}>
          {new Date(message.timestamp).toLocaleTimeString([], {
            hour: '2-digit',
            minute: '2-digit',
          })}
        </div>
      </div>
    </div>
  );
}
