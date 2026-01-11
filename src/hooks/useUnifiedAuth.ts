/**
 * Unified Auth Hook
 *
 * Provides a consistent auth interface that works with both:
 * - Clerk (for web and development)
 * - Desktop Auth (for production Tauri builds)
 *
 * Components using this hook don't need to know which auth system is active.
 */

import { useAuth as useClerkAuth, useUser as useClerkUser } from "@clerk/clerk-react";
import { useDesktopAuth } from "../contexts/DesktopAuthContext";
import { isTauriProduction } from "../lib/desktop-auth";

export interface UnifiedUser {
  id: string;
  email: string | null;
  firstName: string | null;
  lastName: string | null;
  fullName: string | null;
  imageUrl: string | null;
}

export interface UnifiedAuthState {
  isLoaded: boolean;
  isSignedIn: boolean;
  userId: string | null;
  user: UnifiedUser | null;
  signIn: () => Promise<void>;
  signOut: () => Promise<void>;
  getToken: () => Promise<string | null>;
}

/**
 * Hook that provides unified auth state regardless of auth provider
 */
export function useUnifiedAuth(): UnifiedAuthState {
  // In production Tauri builds, use desktop auth
  if (isTauriProduction()) {
    return useDesktopAuthAdapter();
  }

  // Otherwise, use Clerk
  return useClerkAuthAdapter();
}

/**
 * Adapter for Clerk auth to unified interface
 */
function useClerkAuthAdapter(): UnifiedAuthState {
  const { isLoaded, isSignedIn, userId, signOut, getToken } = useClerkAuth();
  const { user: clerkUser } = useClerkUser();

  const user: UnifiedUser | null = clerkUser
    ? {
        id: clerkUser.id,
        email: clerkUser.primaryEmailAddress?.emailAddress || null,
        firstName: clerkUser.firstName,
        lastName: clerkUser.lastName,
        fullName: clerkUser.fullName,
        imageUrl: clerkUser.imageUrl,
      }
    : null;

  return {
    isLoaded,
    isSignedIn: isSignedIn ?? false,
    userId: userId ?? null,
    user,
    signIn: async () => {
      // Clerk handles sign-in through UI components
      // This is a no-op - redirect to sign-in page if needed
      window.location.href = "/sign-in";
    },
    signOut: async () => {
      await signOut();
    },
    getToken: async () => {
      return getToken();
    },
  };
}

/**
 * Adapter for desktop auth to unified interface
 */
function useDesktopAuthAdapter(): UnifiedAuthState {
  const desktopAuth = useDesktopAuth();

  const user: UnifiedUser | null = desktopAuth.user
    ? {
        id: desktopAuth.user.id,
        email: desktopAuth.user.email,
        firstName: desktopAuth.user.firstName,
        lastName: desktopAuth.user.lastName,
        fullName:
          desktopAuth.user.firstName && desktopAuth.user.lastName
            ? `${desktopAuth.user.firstName} ${desktopAuth.user.lastName}`
            : desktopAuth.user.firstName || desktopAuth.user.lastName || null,
        imageUrl: desktopAuth.user.imageUrl,
      }
    : null;

  return {
    isLoaded: desktopAuth.isLoaded,
    isSignedIn: desktopAuth.isSignedIn,
    userId: desktopAuth.user?.id ?? null,
    user,
    signIn: desktopAuth.signIn,
    signOut: desktopAuth.signOut,
    getToken: desktopAuth.getToken,
  };
}

/**
 * Check if using desktop auth mode
 */
export function useIsDesktopAuth(): boolean {
  return isTauriProduction();
}
