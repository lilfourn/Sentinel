interface StreamingIndicatorProps {
  isStreaming: boolean;
  children: React.ReactNode;
}

/**
 * Wraps content during streaming.
 */
export function StreamingIndicator({ isStreaming: _isStreaming, children }: StreamingIndicatorProps) {
  return (
    <div className="relative">
      {children}
    </div>
  );
}

/**
 * Shimmering status text for loading/streaming states.
 */
export function ShimmerText({ text }: { text: string }) {
  return (
    <span className="shimmer-text text-gray-500">
      {text}
    </span>
  );
}

/**
 * Animated dots that bounce in sequence (for thinking/loading states).
 */
export function ThinkingDots() {
  return (
    <span className="inline-flex items-center gap-0.5 ml-1.5" aria-label="Loading">
      <span className="w-1 h-1 bg-purple-400 rounded-full thinking-dot-1" />
      <span className="w-1 h-1 bg-purple-400 rounded-full thinking-dot-2" />
      <span className="w-1 h-1 bg-purple-400 rounded-full thinking-dot-3" />
    </span>
  );
}
