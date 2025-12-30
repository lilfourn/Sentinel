import { useState, useEffect } from 'react';
import { Command } from 'cmdk';
import { File, Folder, Search, X } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useNavigationStore } from '../../stores/navigation-store';

interface MentionItem {
  path: string;
  name: string;
  isDirectory: boolean;
}

interface MentionPopoverProps {
  isOpen: boolean;
  onClose: () => void;
  onSelect: (item: { path: string; name: string; type: 'file' | 'folder' }) => void;
}

export function MentionPopover({ isOpen, onClose, onSelect }: MentionPopoverProps) {
  const currentPath = useNavigationStore((s) => s.currentPath);
  const [items, setItems] = useState<MentionItem[]>([]);
  const [search, setSearch] = useState('');
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (isOpen && currentPath) {
      setLoading(true);
      setSearch('');

      // Try to use the backend command, fallback to reading directory
      invoke<MentionItem[]>('list_files_for_mention', {
        directory: currentPath,
        query: '',
        limit: 30,
      })
        .then(setItems)
        .catch(() => {
          // Fallback: use read_directory if list_files_for_mention doesn't exist yet
          invoke<{ entries: Array<{ path: string; name: string; isDirectory: boolean }> }>(
            'read_directory',
            { path: currentPath }
          )
            .then((result) => {
              const sorted = result.entries
                .slice(0, 30)
                .sort((a, b) => {
                  // Directories first
                  if (a.isDirectory && !b.isDirectory) return -1;
                  if (!a.isDirectory && b.isDirectory) return 1;
                  return a.name.localeCompare(b.name);
                });
              setItems(
                sorted.map((e) => ({
                  path: e.path,
                  name: e.name,
                  isDirectory: e.isDirectory,
                }))
              );
            })
            .catch(console.error);
        })
        .finally(() => setLoading(false));
    }
  }, [isOpen, currentPath]);

  // Filter items based on search
  const filteredItems = search
    ? items.filter((item) =>
        item.name.toLowerCase().includes(search.toLowerCase())
      )
    : items;

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 backdrop-blur-sm">
      <Command
        className="w-96 max-h-96 bg-white dark:bg-gray-800 rounded-xl shadow-2xl border border-gray-200 dark:border-gray-700 overflow-hidden"
        loop
      >
        {/* Header */}
        <div className="flex items-center justify-between px-3 py-2 border-b border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-2 text-sm font-medium text-gray-700 dark:text-gray-300">
            <Search size={14} />
            <span>Add to context</span>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded hover:bg-gray-200 dark:hover:bg-gray-700"
          >
            <X size={14} />
          </button>
        </div>

        {/* Search input */}
        <Command.Input
          value={search}
          onValueChange={setSearch}
          placeholder="Search files and folders..."
          className="w-full px-3 py-2 text-sm bg-transparent border-b border-gray-200 dark:border-gray-700 focus:outline-none"
          autoFocus
        />

        {/* Results */}
        <Command.List className="max-h-64 overflow-y-auto p-2">
          {loading && (
            <Command.Loading className="px-3 py-2 text-sm text-gray-500">
              Loading...
            </Command.Loading>
          )}

          <Command.Empty className="px-3 py-4 text-sm text-gray-500 text-center">
            No files found
          </Command.Empty>

          {/* Directories */}
          {filteredItems.filter((i) => i.isDirectory).length > 0 && (
            <Command.Group heading="Folders" className="text-xs text-gray-400 px-2 py-1">
              {filteredItems
                .filter((item) => item.isDirectory)
                .map((item) => (
                  <Command.Item
                    key={item.path}
                    value={item.name}
                    onSelect={() => {
                      onSelect({
                        path: item.path,
                        name: item.name,
                        type: 'folder',
                      });
                    }}
                    className="flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer text-sm hover:bg-gray-100 dark:hover:bg-gray-700 data-[selected=true]:bg-orange-100 dark:data-[selected=true]:bg-orange-900/30"
                  >
                    <Folder size={14} className="text-blue-500 flex-shrink-0" />
                    <span className="truncate">{item.name}</span>
                    <span className="ml-auto text-[10px] text-gray-400 bg-gray-100 dark:bg-gray-700 px-1.5 py-0.5 rounded">
                      hologram
                    </span>
                  </Command.Item>
                ))}
            </Command.Group>
          )}

          {/* Files */}
          {filteredItems.filter((i) => !i.isDirectory).length > 0 && (
            <Command.Group heading="Files" className="text-xs text-gray-400 px-2 py-1 mt-2">
              {filteredItems
                .filter((item) => !item.isDirectory)
                .map((item) => (
                  <Command.Item
                    key={item.path}
                    value={item.name}
                    onSelect={() => {
                      onSelect({
                        path: item.path,
                        name: item.name,
                        type: 'file',
                      });
                    }}
                    className="flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer text-sm hover:bg-gray-100 dark:hover:bg-gray-700 data-[selected=true]:bg-orange-100 dark:data-[selected=true]:bg-orange-900/30"
                  >
                    <File size={14} className="text-gray-500 flex-shrink-0" />
                    <span className="truncate">{item.name}</span>
                  </Command.Item>
                ))}
            </Command.Group>
          )}
        </Command.List>

        {/* Footer */}
        <div className="px-3 py-2 border-t border-gray-200 dark:border-gray-700 text-[10px] text-gray-400">
          <kbd className="px-1 py-0.5 bg-gray-100 dark:bg-gray-700 rounded">↑↓</kbd> navigate{' '}
          <kbd className="px-1 py-0.5 bg-gray-100 dark:bg-gray-700 rounded ml-2">Enter</kbd> select{' '}
          <kbd className="px-1 py-0.5 bg-gray-100 dark:bg-gray-700 rounded ml-2">Esc</kbd> close
        </div>
      </Command>
    </div>
  );
}
