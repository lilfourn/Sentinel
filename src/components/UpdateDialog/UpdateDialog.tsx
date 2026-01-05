import { Download, X, CheckCircle, AlertCircle, Loader2 } from 'lucide-react';
import { useUpdater } from '../../hooks/useUpdater';
import { cn } from '../../lib/utils';

export function UpdateDialog() {
  const {
    status,
    updateInfo,
    downloadProgress,
    error,
    downloadAndInstall,
    dismissUpdate,
  } = useUpdater();

  // Don't render if no update available or already handled
  if (status !== 'available' && status !== 'downloading' && status !== 'ready' && status !== 'error') {
    return null;
  }

  return (
    <div className="fixed bottom-4 right-4 z-50 w-80 bg-[#2a2a2a]/95 backdrop-blur-xl rounded-xl shadow-2xl border border-white/10 overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between p-3 border-b border-white/10">
        <div className="flex items-center gap-2">
          {status === 'downloading' && (
            <Loader2 size={16} className="text-blue-400 animate-spin" />
          )}
          {status === 'ready' && (
            <CheckCircle size={16} className="text-green-400" />
          )}
          {status === 'error' && (
            <AlertCircle size={16} className="text-red-400" />
          )}
          {status === 'available' && (
            <Download size={16} className="text-orange-400" />
          )}
          <span className="text-sm font-medium text-gray-100">
            {status === 'available' && 'Update Available'}
            {status === 'downloading' && 'Downloading...'}
            {status === 'ready' && 'Update Ready'}
            {status === 'error' && 'Update Failed'}
          </span>
        </div>
        <button
          onClick={dismissUpdate}
          className="p-1 rounded hover:bg-white/10 text-gray-400 transition-colors"
        >
          <X size={14} />
        </button>
      </div>

      {/* Content */}
      <div className="p-3 space-y-3">
        {updateInfo && (
          <div className="text-xs text-gray-400 space-y-1">
            <p>
              <span className="text-gray-300">New version:</span>{' '}
              <span className="text-orange-400 font-medium">{updateInfo.version}</span>
            </p>
            <p>
              <span className="text-gray-300">Current:</span>{' '}
              <span className="text-gray-500">{updateInfo.currentVersion}</span>
            </p>
          </div>
        )}

        {/* Release notes preview */}
        {updateInfo?.releaseNotes && status === 'available' && (
          <div className="max-h-24 overflow-y-auto text-xs text-gray-400 bg-black/30 rounded-lg p-2 border border-white/5">
            {updateInfo.releaseNotes.slice(0, 200)}
            {updateInfo.releaseNotes.length > 200 && '...'}
          </div>
        )}

        {/* Download progress */}
        {status === 'downloading' && (
          <div className="space-y-2">
            <div className="w-full h-1.5 bg-white/10 rounded-full overflow-hidden">
              <div
                className="h-full bg-gradient-to-r from-orange-500 to-purple-500 transition-all duration-300"
                style={{ width: `${Math.min(downloadProgress, 100)}%` }}
              />
            </div>
            <p className="text-xs text-gray-500 text-center">
              {downloadProgress}% downloaded
            </p>
          </div>
        )}

        {/* Error message */}
        {error && (
          <p className="text-xs text-red-400 bg-red-500/10 rounded-lg p-2 border border-red-500/20">
            {error}
          </p>
        )}

        {/* Actions */}
        {status === 'available' && (
          <button
            onClick={downloadAndInstall}
            className={cn(
              'w-full flex items-center justify-center gap-2 py-2 px-4 text-sm font-medium rounded-lg transition-all',
              'bg-gradient-to-r from-orange-500 to-purple-500 text-white',
              'hover:from-orange-600 hover:to-purple-600 hover:shadow-lg hover:shadow-orange-500/25'
            )}
          >
            <Download size={14} />
            Download & Install
          </button>
        )}

        {status === 'ready' && (
          <p className="text-xs text-green-400 text-center py-1">
            Update installed! Restart to apply changes.
          </p>
        )}
      </div>
    </div>
  );
}
