import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Loader2 } from 'lucide-react';
import { useChatStore, type MentionItem } from '../../stores/chat-store';

interface InlineMentionDropdownProps {
  anchorRef: React.RefObject<HTMLDivElement | null>;
  onSelect: (item: MentionItem) => void;
}

export function InlineMentionDropdown({ anchorRef, onSelect }: InlineMentionDropdownProps) {
  const {
    isMentionOpen,
    mentionQuery,
    mentionResults,
    selectedMentionIndex,
    isMentionLoading,
    closeMention,
  } = useChatStore();

  const listRef = useRef<HTMLDivElement>(null);
  const selectedRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState<{ bottom: number; left: number; width: number } | null>(null);

  // Calculate position in layout effect (safe to read refs)
  useLayoutEffect(() => {
    if (!isMentionOpen || !anchorRef.current) {
      // eslint-disable-next-line react-hooks/set-state-in-effect -- Reset position when closed
      setPosition(null);
      return;
    }
    const rect = anchorRef.current.getBoundingClientRect();
    setPosition({
      bottom: window.innerHeight - rect.top + 8,
      left: rect.left,
      width: Math.min(rect.width, 360),
    });
  }, [isMentionOpen, anchorRef]);

  // Scroll selected item into view
  useEffect(() => {
    if (selectedRef.current && listRef.current) {
      selectedRef.current.scrollIntoView({ block: 'nearest' });
    }
  }, [selectedMentionIndex]);

  // Click outside to close
  useEffect(() => {
    if (!isMentionOpen) return;

    const handleClickOutside = (e: MouseEvent) => {
      const target = e.target as Node;
      if (
        anchorRef.current &&
        !anchorRef.current.contains(target) &&
        listRef.current &&
        !listRef.current.contains(target)
      ) {
        closeMention();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [isMentionOpen, closeMention, anchorRef]);

  if (!isMentionOpen || !position) return null;

  const dropdownStyle: React.CSSProperties = {
    position: 'fixed',
    bottom: position.bottom,
    left: position.left,
    width: position.width,
    maxHeight: 280,
    zIndex: 100,
  };

  // Split results into folders and files
  const folders = mentionResults.filter((item) => item.isDirectory);
  const files = mentionResults.filter((item) => !item.isDirectory);

  // Build flat list for index tracking
  const flatList = [...folders, ...files];

  const content = (
    <div
      style={dropdownStyle}
      className="bg-[#161616] border border-white/[0.06] rounded-lg shadow-2xl overflow-hidden flex flex-col backdrop-blur-xl"
      role="listbox"
      aria-label="File and folder suggestions"
      aria-expanded={isMentionOpen}
      aria-activedescendant={flatList.length > 0 ? `mention-option-${selectedMentionIndex}` : undefined}
    >
      {/* Search indicator */}
      <div className="px-3 py-2 text-[11px] text-gray-500 border-b border-white/[0.04]">
        {mentionQuery ? `"${mentionQuery}"` : 'Search files & folders'}
      </div>

      {/* Results */}
      <div ref={listRef} className="overflow-y-auto flex-1">
        {isMentionLoading ? (
          <div className="flex items-center justify-center gap-2 py-6 text-xs text-gray-500" role="status" aria-live="polite">
            <Loader2 size={12} className="animate-spin" aria-hidden="true" />
          </div>
        ) : flatList.length === 0 ? (
          <div className="px-3 py-6 text-xs text-gray-600 text-center">
            {mentionQuery ? 'No results' : 'Empty'}
          </div>
        ) : (
          <>
            {/* Folders section */}
            {folders.length > 0 && (
              <div>
                <div className="px-3 py-1.5 text-[10px] uppercase tracking-wide text-gray-600">
                  Folders
                </div>
                {folders.map((item, idx) => {
                  const globalIndex = idx;
                  const isSelected = globalIndex === selectedMentionIndex;
                  return (
                    <div
                      key={item.path}
                      id={`mention-option-${globalIndex}`}
                      ref={isSelected ? selectedRef : null}
                      onClick={() => onSelect(item)}
                      role="option"
                      aria-selected={isSelected}
                      className={`
                        px-3 py-1.5 cursor-pointer text-[13px] truncate
                        ${isSelected ? 'bg-white/[0.06] text-gray-200' : 'text-gray-400 hover:bg-white/[0.03] hover:text-gray-300'}
                      `}
                    >
                      {item.name}
                    </div>
                  );
                })}
              </div>
            )}

            {/* Files section */}
            {files.length > 0 && (
              <div>
                <div className="px-3 py-1.5 text-[10px] uppercase tracking-wide text-gray-600">
                  Files
                </div>
                {files.map((item, idx) => {
                  const globalIndex = folders.length + idx;
                  const isSelected = globalIndex === selectedMentionIndex;
                  return (
                    <div
                      key={item.path}
                      id={`mention-option-${globalIndex}`}
                      ref={isSelected ? selectedRef : null}
                      onClick={() => onSelect(item)}
                      role="option"
                      aria-selected={isSelected}
                      className={`
                        flex items-center px-3 py-1.5 cursor-pointer text-[13px]
                        ${isSelected ? 'bg-white/[0.06] text-gray-200' : 'text-gray-400 hover:bg-white/[0.03] hover:text-gray-300'}
                      `}
                    >
                      <span className="truncate">{item.name}</span>
                    </div>
                  );
                })}
              </div>
            )}
          </>
        )}
      </div>

      {/* Minimal footer */}
      <div className="px-3 py-1.5 border-t border-white/[0.04] text-[10px] text-gray-600 flex items-center gap-4">
        <span><kbd className="text-gray-500">↑↓</kbd> nav</span>
        <span><kbd className="text-gray-500">↵</kbd> select</span>
        <span><kbd className="text-gray-500">esc</kbd> close</span>
      </div>
    </div>
  );

  return createPortal(content, document.body);
}
