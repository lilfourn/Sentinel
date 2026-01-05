import { useState, useEffect, useCallback } from 'react';
import { readFile } from '@tauri-apps/plugin-fs';
import { ChevronLeft, ChevronRight, X, Info, Loader2 } from 'lucide-react';
import type { PhotoEntry } from '../../types/photo';
import { cn, formatFileSize, formatAbsoluteDate } from '../../lib/utils';

interface LightboxProps {
  photos: PhotoEntry[];
  currentIndex: number;
  onClose: () => void;
  onNext: () => void;
  onPrev: () => void;
}

export function Lightbox({ photos, currentIndex, onClose, onNext, onPrev }: LightboxProps) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [showInfo, setShowInfo] = useState(false);
  const currentPhoto = photos[currentIndex];

  // Load full-resolution image
  useEffect(() => {
    if (!currentPhoto) return;

    let cancelled = false;
    setLoading(true);

    async function loadImage() {
      try {
        const bytes = await readFile(currentPhoto.path);
        const blob = new Blob([bytes]);
        const url = URL.createObjectURL(blob);
        if (!cancelled) {
          setImageUrl(url);
          setLoading(false);
        }
      } catch (error) {
        console.error('Failed to load image:', error);
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    loadImage();

    return () => {
      cancelled = true;
      if (imageUrl) URL.revokeObjectURL(imageUrl);
    };
  }, [currentPhoto?.path]);

  // Keyboard navigation
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === 'ArrowRight') onNext();
      if (e.key === 'ArrowLeft') onPrev();
      if (e.key === 'Escape') onClose();
      if (e.key === 'i') setShowInfo((s) => !s);
    },
    [onNext, onPrev, onClose]
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  if (!currentPhoto) return null;

  return (
    <div className="fixed inset-0 z-50 bg-black/95 backdrop-blur-xl flex items-center justify-center">
      {/* Top bar */}
      <div className="absolute top-0 left-0 right-0 h-16 flex items-center justify-between px-4 bg-gradient-to-b from-black/50 to-transparent z-10">
        <div className="text-white/70 text-sm">
          {currentIndex + 1} of {photos.length}
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={() => setShowInfo((s) => !s)}
            className={cn(
              'p-2.5 rounded-full transition-all',
              showInfo
                ? 'bg-white/20 text-white'
                : 'text-white/70 hover:text-white hover:bg-white/10'
            )}
          >
            <Info size={18} />
          </button>
          <button
            onClick={onClose}
            className="p-2.5 rounded-full text-white/70 hover:text-white hover:bg-white/10 transition-all"
          >
            <X size={18} />
          </button>
        </div>
      </div>

      {/* Navigation arrows */}
      {currentIndex > 0 && (
        <button
          onClick={onPrev}
          className="absolute left-4 top-1/2 -translate-y-1/2 p-3 rounded-full text-white/50 hover:text-white hover:bg-white/10 transition-all"
        >
          <ChevronLeft size={32} />
        </button>
      )}
      {currentIndex < photos.length - 1 && (
        <button
          onClick={onNext}
          className="absolute right-4 top-1/2 -translate-y-1/2 p-3 rounded-full text-white/50 hover:text-white hover:bg-white/10 transition-all"
        >
          <ChevronRight size={32} />
        </button>
      )}

      {/* Main image */}
      <div className={cn('flex-1 flex items-center justify-center h-full p-20', showInfo && 'pr-80')}>
        {loading ? (
          <div className="flex flex-col items-center gap-3">
            <Loader2 className="animate-spin text-white/50" size={32} />
            <span className="text-white/50 text-sm">Loading...</span>
          </div>
        ) : imageUrl ? (
          <img
            src={imageUrl}
            alt={currentPhoto.name}
            className="max-h-full max-w-full object-contain rounded-lg shadow-2xl"
          />
        ) : (
          <div className="text-white/50">Failed to load image</div>
        )}
      </div>

      {/* Photo info panel */}
      <div
        className={cn(
          'absolute right-0 top-0 bottom-0 w-72 bg-black/80 backdrop-blur-xl text-white p-6 pt-20 overflow-y-auto',
          'transition-transform duration-300 ease-out',
          showInfo ? 'translate-x-0' : 'translate-x-full'
        )}
      >
        <div className="space-y-6">
          <div>
            <p className="text-xs text-white/40 uppercase tracking-wider mb-1">Filename</p>
            <p className="text-sm font-medium truncate">{currentPhoto.name}</p>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <p className="text-xs text-white/40 uppercase tracking-wider mb-1">Date</p>
              <p className="text-sm">{formatAbsoluteDate(currentPhoto.createdAt || currentPhoto.modifiedAt)}</p>
            </div>
            <div>
              <p className="text-xs text-white/40 uppercase tracking-wider mb-1">Size</p>
              <p className="text-sm">{formatFileSize(currentPhoto.size)}</p>
            </div>
          </div>

          <div>
            <p className="text-xs text-white/40 uppercase tracking-wider mb-1">Type</p>
            <p className="text-sm">{currentPhoto.extension?.toUpperCase() || 'Unknown'}</p>
          </div>

          <div>
            <p className="text-xs text-white/40 uppercase tracking-wider mb-1">Location</p>
            <p className="text-xs text-white/70 break-all leading-relaxed">{currentPhoto.path}</p>
          </div>
        </div>
      </div>
    </div>
  );
}
