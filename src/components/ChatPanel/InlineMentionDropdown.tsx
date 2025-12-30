import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { File, Folder, Loader2 } from 'lucide-react';
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

  if (!isMentionOpen || !anchorRef.current) return null;

  // Calculate position above the anchor
  const anchorRect = anchorRef.current.getBoundingClientRect();
  const dropdownStyle: React.CSSProperties = {
    position: 'fixed',
    bottom: window.innerHeight - anchorRect.top + 8,
    left: anchorRect.left,
    width: anchorRect.width,
    maxHeight: 320,
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
      className="bg-[#1e1e1e] border border-white/10 rounded-lg shadow-xl overflow-hidden flex flex-col"
    >
      {/* Header */}
      <div className="px-3 py-2 border-b border-white/5 text-xs text-gray-400">
        {mentionQuery ? (
          <>Searching for "{mentionQuery}"...</>
        ) : (
          <>Type to search files and folders</>
        )}
      </div>

      {/* Results */}
      <div ref={listRef} className="overflow-y-auto flex-1 py-1">
        {isMentionLoading ? (
          <div className="flex items-center justify-center gap-2 py-4 text-sm text-gray-400">
            <Loader2 size={14} className="animate-spin" />
            <span>Searching...</span>
          </div>
        ) : flatList.length === 0 ? (
          <div className="px-3 py-4 text-sm text-gray-500 text-center">
            {mentionQuery ? 'No matches found' : 'No files in current directory'}
          </div>
        ) : (
          <>
            {/* Folders section */}
            {folders.length > 0 && (
              <div className="mb-1">
                <div className="px-3 py-1 text-[10px] uppercase tracking-wider text-gray-500 font-medium">
                  Folders
                </div>
                {folders.map((item, idx) => {
                  const globalIndex = idx;
                  const isSelected = globalIndex === selectedMentionIndex;
                  return (
                    <div
                      key={item.path}
                      ref={isSelected ? selectedRef : null}
                      onClick={() => onSelect(item)}
                      className={`
                        flex items-center gap-2 px-3 py-1.5 cursor-pointer text-sm
                        ${isSelected ? 'bg-orange-500/20 text-orange-300' : 'text-gray-300 hover:bg-white/5'}
                      `}
                    >
                      <Folder size={14} className="text-blue-400 flex-shrink-0" />
                      <span className="truncate flex-1">{item.name}</span>
                      <span className="text-[10px] text-gray-500 bg-white/5 px-1.5 py-0.5 rounded">
                        hologram
                      </span>
                    </div>
                  );
                })}
              </div>
            )}

            {/* Files section */}
            {files.length > 0 && (
              <div>
                <div className="px-3 py-1 text-[10px] uppercase tracking-wider text-gray-500 font-medium">
                  Files
                </div>
                {files.map((item, idx) => {
                  const globalIndex = folders.length + idx;
                  const isSelected = globalIndex === selectedMentionIndex;
                  return (
                    <div
                      key={item.path}
                      ref={isSelected ? selectedRef : null}
                      onClick={() => onSelect(item)}
                      className={`
                        flex items-center gap-2 px-3 py-1.5 cursor-pointer text-sm
                        ${isSelected ? 'bg-orange-500/20 text-orange-300' : 'text-gray-300 hover:bg-white/5'}
                      `}
                    >
                      <File size={14} className="text-gray-500 flex-shrink-0" />
                      <span className="truncate">{item.name}</span>
                    </div>
                  );
                })}
              </div>
            )}
          </>
        )}
      </div>

      {/* Footer with keyboard hints */}
      <div className="px-3 py-1.5 border-t border-white/5 text-[10px] text-gray-500 flex items-center gap-3">
        <span>
          <kbd className="px-1 py-0.5 bg-white/5 rounded">↑↓</kbd> navigate
        </span>
        <span>
          <kbd className="px-1 py-0.5 bg-white/5 rounded">Enter</kbd> select
        </span>
        <span>
          <kbd className="px-1 py-0.5 bg-white/5 rounded">Esc</kbd> close
        </span>
      </div>
    </div>
  );

  return createPortal(content, document.body);
}
