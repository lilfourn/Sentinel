import { UserButton } from "@clerk/clerk-react";
import { useUnifiedAuth, useIsDesktopAuth } from "../../hooks/useUnifiedAuth";

/**
 * User menu button for the toolbar
 * Shows user avatar with dropdown for account management
 * Uses unified auth to work with both Clerk and desktop auth modes
 *
 * IMPORTANT: This component must stop event propagation to prevent
 * Tauri's data-tauri-drag-region from intercepting clicks on the UserButton.
 * Without this, clicks on the avatar trigger window dragging instead of
 * opening Clerk's account dropdown.
 */
export function UserMenu() {
  const { isSignedIn, user, signOut } = useUnifiedAuth();
  const isDesktopAuth = useIsDesktopAuth();

  if (!isSignedIn) {
    return null;
  }

  // Stop propagation to prevent Tauri drag region from capturing clicks
  const stopDragPropagation = (e: React.MouseEvent | React.PointerEvent) => {
    e.stopPropagation();
  };

  // Desktop auth mode - custom user menu (Clerk components don't work without ClerkProvider)
  if (isDesktopAuth) {
    return (
      <div
        className="flex items-center gap-2"
        onMouseDown={stopDragPropagation}
        onPointerDown={stopDragPropagation}
      >
        <span className="text-xs text-gray-400 hidden sm:block">
          {user?.firstName || user?.email}
        </span>
        <button
          onClick={() => signOut()}
          className="w-7 h-7 rounded-full bg-gray-600 flex items-center justify-center text-xs text-white hover:bg-gray-500 transition-colors"
          title={user?.email || "Sign out"}
        >
          {user?.firstName?.[0]?.toUpperCase() || user?.email?.[0]?.toUpperCase() || "U"}
        </button>
      </div>
    );
  }

  // Clerk mode - use Clerk's UserButton component
  // Wrap in div that stops propagation to allow Clerk's popover to work
  return (
    <div
      className="flex items-center gap-2"
      onMouseDown={stopDragPropagation}
      onPointerDown={stopDragPropagation}
    >
      <span className="text-xs text-gray-400 hidden sm:block">
        {user?.firstName || user?.email}
      </span>
      <UserButton
        appearance={{
          elements: {
            avatarBox: "w-7 h-7",
          },
        }}
      />
    </div>
  );
}
