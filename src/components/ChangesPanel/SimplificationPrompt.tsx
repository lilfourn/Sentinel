import { FolderTree, CheckCircle, Loader2 } from 'lucide-react';
import { cn } from '../../lib/utils';

interface SimplificationPromptProps {
  onAccept: () => void;
  onReject: () => void;
  folderName: string;
  isLoading?: boolean;
}

export function SimplificationPrompt({
  onAccept,
  onReject,
  folderName,
  isLoading = false,
}: SimplificationPromptProps) {
  return (
    <div className="p-3 mt-2 rounded-lg bg-white/[0.03] border border-white/10">
      <div className="mb-3">
        <p className="text-xs text-gray-300 font-medium">
          No content changes needed
        </p>
        <p className="text-[10px] text-gray-500 mt-1">
          Would you like to simplify the folder structure?
        </p>
      </div>

      <div className="space-y-2">
        <p className="text-[10px] text-gray-400">
          This will analyze <span className="text-gray-300">{folderName}</span> for:
        </p>
        <ul className="text-[10px] text-gray-500 space-y-1 pl-3">
          <li>• Deeply nested folders (depth &gt; 3)</li>
          <li>• Sparse folders with few files</li>
          <li>• Overly long path names</li>
        </ul>
      </div>

      <div className="flex gap-2 mt-4">
        <button
          onClick={onAccept}
          disabled={isLoading}
          className={cn(
            'flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium transition-colors',
            'bg-orange-500/20 text-orange-300 hover:bg-orange-500/30',
            'disabled:opacity-50 disabled:cursor-not-allowed'
          )}
        >
          {isLoading ? (
            <>
              <Loader2 size={12} className="animate-spin" />
              Analyzing...
            </>
          ) : (
            <>
              <FolderTree size={12} />
              Simplify Structure
            </>
          )}
        </button>
        <button
          onClick={onReject}
          disabled={isLoading}
          className={cn(
            'flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium transition-colors',
            'bg-white/5 text-gray-400 hover:bg-white/10 hover:text-gray-300',
            'disabled:opacity-50 disabled:cursor-not-allowed'
          )}
        >
          <CheckCircle size={12} />
          No, Complete
        </button>
      </div>
    </div>
  );
}
