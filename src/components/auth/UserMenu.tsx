import { useState, useRef, useEffect, type ReactNode } from "react";
import { UserButton } from "@clerk/clerk-react";
import { User, LogOut } from "lucide-react";
import { useUnifiedAuth, useIsDesktopAuth } from "../../hooks/useUnifiedAuth";
import { openAccountInBrowser } from "../../lib/desktop-auth";

// Stop propagation to prevent Tauri drag region from capturing clicks
function stopPropagation(e: React.MouseEvent | React.PointerEvent): void {
  e.stopPropagation();
}

/**
 * User menu button for the toolbar.
 * Works with both Clerk and desktop auth modes.
 * Stops event propagation to prevent Tauri's drag region from intercepting clicks.
 */
export function UserMenu(): ReactNode {
  const { isSignedIn, user, signOut } = useUnifiedAuth();
  const isDesktopAuth = useIsDesktopAuth();
  const [isOpen, setIsOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;

    function handleClickOutside(e: MouseEvent): void {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    }

    function handleEscape(e: KeyboardEvent): void {
      if (e.key === "Escape") setIsOpen(false);
    }

    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleEscape);

    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [isOpen]);

  if (!isSignedIn) return null;

  const displayName = user?.firstName || user?.email;

  function handleManageAccount(): void {
    setIsOpen(false);
    openAccountInBrowser();
  }

  function handleSignOut(): void {
    setIsOpen(false);
    signOut();
  }

  const avatarInitial = user?.firstName?.[0]?.toUpperCase() || user?.email?.[0]?.toUpperCase() || "U";

  // Desktop auth mode - custom user menu with dropdown
  if (isDesktopAuth) {
    return (
      <div
        ref={menuRef}
        className="relative flex items-center gap-2"
        onMouseDown={stopPropagation}
        onPointerDown={stopPropagation}
      >
        <span className="text-xs text-gray-400 hidden sm:block">{displayName}</span>
        <button
          onClick={() => setIsOpen(!isOpen)}
          className="w-7 h-7 rounded-full bg-gray-600 flex items-center justify-center text-xs text-white hover:bg-gray-500 transition-colors"
          title={user?.email || "Account"}
        >
          {avatarInitial}
        </button>

        {isOpen && (
          <div className="absolute top-full right-0 mt-2 min-w-[160px] py-1 rounded-lg glass-context-menu animate-in fade-in-0 zoom-in-95 duration-100 z-50">
            <button
              onClick={handleManageAccount}
              className="w-full flex items-center gap-3 px-3 py-2 text-sm text-left text-gray-200 hover:bg-white/10 transition-colors"
            >
              <User size={14} />
              <span>Manage Account</span>
            </button>
            <div className="h-px my-1 mx-2 bg-white/10" />
            <button
              onClick={handleSignOut}
              className="w-full flex items-center gap-3 px-3 py-2 text-sm text-left text-red-400 hover:bg-red-500/15 transition-colors"
            >
              <LogOut size={14} />
              <span>Sign Out</span>
            </button>
          </div>
        )}
      </div>
    );
  }

  // Clerk mode - use Clerk's UserButton component
  return (
    <div
      className="flex items-center gap-2"
      onMouseDown={stopPropagation}
      onPointerDown={stopPropagation}
    >
      <span className="text-xs text-gray-400 hidden sm:block">{displayName}</span>
      <UserButton appearance={{ elements: { avatarBox: "w-7 h-7" } }} />
    </div>
  );
}
