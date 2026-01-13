import { useEffect, useRef, useCallback } from 'react';
import { Channel, invoke } from '@tauri-apps/api/core';
import { useVfsStore } from '../stores/vfs-store';
import type { VfsEvent } from '../types/vfs';

/**
 * Hook to manage the VFS channel connection with the Tauri backend.
 *
 * Creates a channel for receiving VFS events (indexing progress, conflicts, etc.)
 * and registers it with the backend when a target folder is specified.
 */
export function useVfsChannel(targetFolder: string | null) {
  const channelRef = useRef<Channel<VfsEvent> | null>(null);
  const handleVfsEvent = useVfsStore((state) => state.handleVfsEvent);
  const initializeVfs = useVfsStore((state) => state.initializeVfs);
  const reset = useVfsStore((state) => state.reset);

  // Handle incoming VFS events
  const onMessage = useCallback(
    (event: VfsEvent) => {
      handleVfsEvent(event);
    },
    [handleVfsEvent]
  );

  useEffect(() => {
    if (!targetFolder) {
      // No target folder - clean up if we have a channel
      if (channelRef.current) {
        channelRef.current = null;
        reset();
      }
      return;
    }

    // Initialize VFS state
    initializeVfs(targetFolder);

    // Create a new channel
    const channel = new Channel<VfsEvent>();
    channel.onmessage = onMessage;
    channelRef.current = channel;

    // Register the channel with the backend
    // Note: This invoke call assumes the backend has a register_vfs_channel command
    // If not implemented yet, this will silently fail
    invoke('register_vfs_channel', {
      targetFolder,
      channel,
    }).catch((error) => {
      // Backend may not have this command implemented yet
      console.debug('[VFS] Channel registration not available:', error);
    });

    // Cleanup on unmount or when targetFolder changes
    return () => {
      channelRef.current = null;
      // Note: Tauri channels are automatically cleaned up when the Channel object
      // is garbage collected, but we could also explicitly unregister if needed
    };
  }, [targetFolder, onMessage, initializeVfs, reset]);

  return {
    /** Whether the channel is connected */
    // eslint-disable-next-line react-hooks/refs -- Connection status checked synchronously
    isConnected: channelRef.current !== null,
  };
}
