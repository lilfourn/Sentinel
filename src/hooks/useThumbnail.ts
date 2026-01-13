import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { isThumbnailSupported } from '../lib/utils';

// Simple in-memory cache for thumbnails
const thumbnailCache = new Map<string, string>();

interface UseThumbnailResult {
  thumbnail: string | null;
  loading: boolean;
  error: string | null;
}

export function useThumbnail(
  path: string | null,
  extension: string | null,
  size: number = 96
): UseThumbnailResult {
  const [thumbnail, setThumbnail] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const abortRef = useRef(false);

  useEffect(() => {
    abortRef.current = false;

    // Reset state when path changes
    // eslint-disable-next-line react-hooks/set-state-in-effect -- Intentional reset on dependency change
    setThumbnail(null);
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setError(null);

    // Skip if no path or unsupported type
    if (!path || !isThumbnailSupported(extension)) {
      setLoading(false);
      return;
    }

    // Check cache first
    const cacheKey = `${path}:${size}`;
    const cached = thumbnailCache.get(cacheKey);
    if (cached) {
      setThumbnail(cached);
      setLoading(false);
      return;
    }

    // Fetch thumbnail from Rust backend
    setLoading(true);

    invoke<string>('get_thumbnail', { path, size })
      .then((base64) => {
        if (abortRef.current) return;

        // Cache the result
        thumbnailCache.set(cacheKey, base64);
        setThumbnail(base64);
        setLoading(false);
      })
      .catch((err) => {
        if (abortRef.current) return;

        setError(String(err));
        setLoading(false);
      });

    return () => {
      abortRef.current = true;
    };
  }, [path, extension, size]);

  return { thumbnail, loading, error };
}

// Utility to preload thumbnails for a list of paths
export async function preloadThumbnails(
  paths: { path: string; extension: string | null }[],
  size: number = 96
): Promise<void> {
  const supported = paths.filter((p) => isThumbnailSupported(p.extension));

  // Process in batches of 5 to avoid overwhelming the system
  const batchSize = 5;
  for (let i = 0; i < supported.length; i += batchSize) {
    const batch = supported.slice(i, i + batchSize);
    await Promise.allSettled(
      batch.map(async ({ path }) => {
        const cacheKey = `${path}:${size}`;
        if (thumbnailCache.has(cacheKey)) return;

        try {
          const base64 = await invoke<string>('get_thumbnail', { path, size });
          thumbnailCache.set(cacheKey, base64);
        } catch {
          // Ignore errors during preload
        }
      })
    );
  }
}

// Clear the in-memory cache
export function clearThumbnailMemoryCache(): void {
  thumbnailCache.clear();
}
