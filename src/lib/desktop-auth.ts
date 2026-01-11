/**
 * Desktop Authentication Module
 *
 * Handles OAuth flow for Tauri desktop apps using external browser authentication.
 * This bypasses Clerk's browser SDK limitations with localhost origins.
 *
 * Flow:
 * 1. User clicks "Sign In" -> opens system browser to web auth page
 * 2. User authenticates via Clerk on the web
 * 3. Web page redirects to sentinel://auth-callback?token=...
 * 4. Tauri handles deep link and stores the session
 */

import { openUrl } from "@tauri-apps/plugin-opener";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Auth configuration
const AUTH_CONFIG = {
  // Web auth page URL - user needs to deploy this
  webAuthUrl: "https://app-sentinel.dev/desktop-auth",
  // Deep link scheme
  callbackScheme: "sentinel",
  // Token storage key
  storageKey: "sentinel_auth_token",
  // User data storage key
  userStorageKey: "sentinel_auth_user",
};

export interface DesktopUser {
  id: string;
  email: string | null;
  firstName: string | null;
  lastName: string | null;
  imageUrl: string | null;
}

export interface DesktopAuthState {
  isLoaded: boolean;
  isSignedIn: boolean;
  user: DesktopUser | null;
  token: string | null;
}

export interface AuthCallbackPayload {
  token: string;
  userId: string;
  email?: string;
  firstName?: string;
  lastName?: string;
  imageUrl?: string;
  expiresAt?: number;
}

/**
 * Check if running in Tauri environment
 */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Check if running in production Tauri build (not dev server)
 */
export function isTauriProduction(): boolean {
  if (!isTauri()) return false;
  // In dev mode, we're served from localhost:1420
  // In production, we're served from localhost:12753 (localhost plugin) or tauri://
  const origin = window.location.origin;
  return !origin.includes(":1420");
}

/**
 * Open the system browser for authentication
 */
export async function openAuthInBrowser(): Promise<void> {
  const authUrl = new URL(AUTH_CONFIG.webAuthUrl);
  // Add a state parameter for security (prevents CSRF)
  const state = crypto.randomUUID();
  sessionStorage.setItem("auth_state", state);
  authUrl.searchParams.set("state", state);
  authUrl.searchParams.set("callback", `${AUTH_CONFIG.callbackScheme}://auth-callback`);

  await openUrl(authUrl.toString());
}

/**
 * Open browser for sign out
 */
export async function openSignOutInBrowser(): Promise<void> {
  const signOutUrl = new URL(AUTH_CONFIG.webAuthUrl);
  signOutUrl.searchParams.set("action", "sign-out");
  signOutUrl.searchParams.set("callback", `${AUTH_CONFIG.callbackScheme}://auth-callback`);

  await openUrl(signOutUrl.toString());
}

/**
 * Store auth data securely
 */
export function storeAuthData(payload: AuthCallbackPayload): void {
  const user: DesktopUser = {
    id: payload.userId,
    email: payload.email || null,
    firstName: payload.firstName || null,
    lastName: payload.lastName || null,
    imageUrl: payload.imageUrl || null,
  };

  localStorage.setItem(AUTH_CONFIG.storageKey, payload.token);
  localStorage.setItem(AUTH_CONFIG.userStorageKey, JSON.stringify(user));

  if (payload.expiresAt) {
    localStorage.setItem(`${AUTH_CONFIG.storageKey}_expires`, payload.expiresAt.toString());
  }
}

/**
 * Clear auth data (sign out)
 */
export function clearAuthData(): void {
  localStorage.removeItem(AUTH_CONFIG.storageKey);
  localStorage.removeItem(AUTH_CONFIG.userStorageKey);
  localStorage.removeItem(`${AUTH_CONFIG.storageKey}_expires`);
  sessionStorage.removeItem("auth_state");
}

/**
 * Get stored auth token
 */
export function getStoredToken(): string | null {
  const token = localStorage.getItem(AUTH_CONFIG.storageKey);
  const expiresAt = localStorage.getItem(`${AUTH_CONFIG.storageKey}_expires`);

  // Check if token is expired
  if (expiresAt && Date.now() > parseInt(expiresAt, 10)) {
    clearAuthData();
    return null;
  }

  return token;
}

/**
 * Get stored user data
 */
export function getStoredUser(): DesktopUser | null {
  const userData = localStorage.getItem(AUTH_CONFIG.userStorageKey);
  if (!userData) return null;

  try {
    return JSON.parse(userData) as DesktopUser;
  } catch {
    return null;
  }
}

/**
 * Get current auth state from storage
 */
export function getStoredAuthState(): DesktopAuthState {
  const token = getStoredToken();
  const user = getStoredUser();

  return {
    isLoaded: true,
    isSignedIn: !!token && !!user,
    user,
    token,
  };
}

/**
 * Parse auth callback URL parameters
 */
export function parseAuthCallback(url: string): AuthCallbackPayload | null {
  try {
    const urlObj = new URL(url);
    const params = urlObj.searchParams;

    const token = params.get("token");
    const userId = params.get("userId");

    if (!token || !userId) {
      console.error("Missing required auth callback parameters");
      return null;
    }

    // Verify state to prevent CSRF
    const state = params.get("state");
    const storedState = sessionStorage.getItem("auth_state");
    if (state && storedState && state !== storedState) {
      console.error("Auth state mismatch - possible CSRF attack");
      return null;
    }

    return {
      token,
      userId,
      email: params.get("email") || undefined,
      firstName: params.get("firstName") || undefined,
      lastName: params.get("lastName") || undefined,
      imageUrl: params.get("imageUrl") || undefined,
      expiresAt: params.get("expiresAt") ? parseInt(params.get("expiresAt")!, 10) : undefined,
    };
  } catch (error) {
    console.error("Failed to parse auth callback URL:", error);
    return null;
  }
}

/**
 * Listen for deep link auth callbacks
 * Returns an unlisten function to clean up the listener
 */
export async function listenForAuthCallback(
  onSuccess: (payload: AuthCallbackPayload) => void,
  onError: (error: string) => void
): Promise<UnlistenFn> {
  return listen<string>("deep-link://new-url", (event) => {
    const url = event.payload;
    console.log("Received deep link:", url);

    if (url.includes("auth-callback")) {
      const payload = parseAuthCallback(url);

      if (payload) {
        // Check for sign-out action
        if (url.includes("action=signed-out")) {
          clearAuthData();
          onSuccess({ ...payload, token: "", userId: "" });
        } else {
          storeAuthData(payload);
          onSuccess(payload);
        }
      } else {
        onError("Failed to parse authentication response");
      }
    }
  });
}

/**
 * Get a token for API calls (compatible with Clerk's getToken interface)
 */
export async function getToken(): Promise<string | null> {
  return getStoredToken();
}
