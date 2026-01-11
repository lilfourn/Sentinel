/**
 * Desktop Auth Context
 *
 * Provides authentication state and methods for Tauri desktop apps.
 * Uses external browser OAuth flow instead of Clerk's browser SDK.
 *
 * This context provides a similar interface to Clerk's useAuth hook
 * to minimize changes needed in existing components.
 */

import React, { createContext, useContext, useEffect, useState, useCallback, useRef } from "react";
import { onOpenUrl } from "@tauri-apps/plugin-deep-link";
import {
  type DesktopAuthState,
  type DesktopUser,
  getStoredAuthState,
  openAuthInBrowser,
  storeAuthData,
  clearAuthData,
  parseAuthCallback,
  getStoredToken,
  isTauriProduction,
} from "../lib/desktop-auth";

interface DesktopAuthContextValue extends DesktopAuthState {
  signIn: () => Promise<void>;
  signOut: () => Promise<void>;
  getToken: () => Promise<string | null>;
  isDesktopAuth: boolean;
}

const DesktopAuthContext = createContext<DesktopAuthContextValue | null>(null);

interface DesktopAuthProviderProps {
  children: React.ReactNode;
}

export function DesktopAuthProvider({ children }: DesktopAuthProviderProps) {
  const [authState, setAuthState] = useState<DesktopAuthState>({
    isLoaded: false,
    isSignedIn: false,
    user: null,
    token: null,
  });

  const hasInitialized = useRef(false);

  // Initialize auth state from storage
  useEffect(() => {
    if (hasInitialized.current) return;
    hasInitialized.current = true;

    const storedState = getStoredAuthState();
    setAuthState(storedState);
  }, []);

  // Listen for deep link auth callbacks
  useEffect(() => {
    const handleAuthCallback = (urls: string[]) => {
      for (const url of urls) {
        console.log("[DesktopAuth] Received URL:", url);

        if (url.includes("auth-callback")) {
          // Check if it's a sign-out callback
          if (url.includes("action=signed-out")) {
            console.log("[DesktopAuth] Sign-out callback received");
            clearAuthData();
            setAuthState({
              isLoaded: true,
              isSignedIn: false,
              user: null,
              token: null,
            });
            return;
          }

          // Parse sign-in callback
          const payload = parseAuthCallback(url);

          if (payload && payload.token && payload.userId) {
            console.log("[DesktopAuth] Sign-in callback received, user:", payload.userId);
            storeAuthData(payload);

            const user: DesktopUser = {
              id: payload.userId,
              email: payload.email || null,
              firstName: payload.firstName || null,
              lastName: payload.lastName || null,
              imageUrl: payload.imageUrl || null,
            };

            setAuthState({
              isLoaded: true,
              isSignedIn: true,
              user,
              token: payload.token,
            });
          } else {
            console.error("[DesktopAuth] Invalid auth callback payload");
          }
        }
      }
    };

    // Register deep link listener
    const unlisten = onOpenUrl(handleAuthCallback);

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const signIn = useCallback(async () => {
    console.log("[DesktopAuth] Opening browser for sign-in");
    await openAuthInBrowser();
  }, []);

  const signOut = useCallback(async () => {
    console.log("[DesktopAuth] Signing out");
    // Clear local state immediately
    clearAuthData();
    setAuthState({
      isLoaded: true,
      isSignedIn: false,
      user: null,
      token: null,
    });
    // Optionally open browser to sign out of Clerk web session too
    // await openSignOutInBrowser();
  }, []);

  const getToken = useCallback(async (): Promise<string | null> => {
    return getStoredToken();
  }, []);

  const value: DesktopAuthContextValue = {
    ...authState,
    signIn,
    signOut,
    getToken,
    isDesktopAuth: true,
  };

  return (
    <DesktopAuthContext.Provider value={value}>
      {children}
    </DesktopAuthContext.Provider>
  );
}

/**
 * Hook to access desktop auth context
 */
export function useDesktopAuth(): DesktopAuthContextValue {
  const context = useContext(DesktopAuthContext);

  if (!context) {
    throw new Error("useDesktopAuth must be used within a DesktopAuthProvider");
  }

  return context;
}

/**
 * Check if desktop auth should be used
 * Returns true in production Tauri builds
 */
export function shouldUseDesktopAuth(): boolean {
  return isTauriProduction();
}
