/**
 * Desktop Authentication Module
 *
 * Handles OAuth flow for Tauri desktop apps using external browser authentication.
 * This bypasses Clerk's browser SDK limitations with localhost origins.
 *
 * Flow:
 * 1. User clicks "Sign In" -> opens system browser to web auth page
 * 2. User authenticates via Clerk on the web
 * 3. Web page redirects to sentinel://auth-callback#token=... (fragment for security)
 * 4. Tauri handles deep link and stores the session
 */

import { openUrl } from "@tauri-apps/plugin-opener";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { authCallbackSchema, parseStoredUser, type AuthCallbackPayload } from "./schemas/auth";
import * as secureStorage from "./secure-storage";

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

// Re-export the Zod-validated type
export type { AuthCallbackPayload } from "./schemas/auth";

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
 * Generate a cryptographically secure auth state token
 */
export function generateAuthState(): string {
  const array = new Uint8Array(32);
  crypto.getRandomValues(array);
  const state = Array.from(array, (b) => b.toString(16).padStart(2, "0")).join("");
  sessionStorage.setItem("sentinel_auth_state", state);
  return state;
}

/**
 * Validate auth state to prevent CSRF attacks
 * State is one-time use - cleared after validation
 */
export function validateAuthState(receivedState: string | null): boolean {
  const storedState = sessionStorage.getItem("sentinel_auth_state");
  sessionStorage.removeItem("sentinel_auth_state"); // One-time use
  return storedState !== null && receivedState !== null && storedState === receivedState;
}

/**
 * Open the system browser for authentication
 */
export async function openAuthInBrowser(): Promise<void> {
  const authUrl = new URL(AUTH_CONFIG.webAuthUrl);
  // Generate crypto-secure state for CSRF protection
  const state = generateAuthState();
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
 * Store auth data securely using Tauri store plugin
 * Falls back to localStorage if store is not available
 */
export async function storeAuthData(payload: AuthCallbackPayload): Promise<void> {
  const user: DesktopUser = {
    id: payload.userId,
    email: payload.email || null,
    firstName: payload.firstName || null,
    lastName: payload.lastName || null,
    imageUrl: payload.imageUrl || null,
  };

  await secureStorage.storeToken(payload.token);
  await secureStorage.storeUser(user);

  if (payload.expiresAt) {
    await secureStorage.storeExpiry(payload.expiresAt);
  }
}

/**
 * Clear auth data (sign out)
 */
export async function clearAuthData(): Promise<void> {
  await secureStorage.clearAllAuth();
  sessionStorage.removeItem("sentinel_auth_state");
}

/**
 * Get stored auth token from secure storage
 */
export async function getStoredToken(): Promise<string | null> {
  // Check if token is expired
  if (await secureStorage.isTokenExpired()) {
    await clearAuthData();
    return null;
  }

  return secureStorage.getToken();
}

/**
 * Get stored user data from secure storage with Zod validation
 */
export async function getStoredUser(): Promise<DesktopUser | null> {
  try {
    const userData = await secureStorage.getUser<unknown>();
    if (!userData) return null;

    // Validate with Zod schema
    const validated = parseStoredUser(userData);
    if (!validated) return null;

    return {
      id: validated.id,
      email: validated.email,
      firstName: validated.firstName,
      lastName: validated.lastName,
      imageUrl: validated.imageUrl,
    };
  } catch {
    console.error("[Auth] Failed to get stored user data");
    return null;
  }
}

/**
 * Get current auth state from storage
 */
export async function getStoredAuthState(): Promise<DesktopAuthState> {
  const token = await getStoredToken();
  const user = await getStoredUser();

  return {
    isLoaded: true,
    isSignedIn: !!token && !!user,
    user,
    token,
  };
}

/**
 * Migrate auth data from localStorage to secure storage (call on app startup)
 */
export async function migrateAuthStorage(): Promise<void> {
  await secureStorage.migrateFromLocalStorage();
}

/**
 * Parse auth callback URL - supports both hash fragments (secure) and query params (legacy)
 * Hash fragments are preferred as they are not sent to servers or logged
 * Uses Zod validation for runtime type safety
 */
export function parseAuthCallback(url: string): AuthCallbackPayload | null {
  try {
    const urlObj = new URL(url);

    // Try hash fragment first (new secure method)
    // Fall back to query params for backwards compatibility with deployed web-auth page
    let params: URLSearchParams;
    if (urlObj.hash && urlObj.hash.length > 1) {
      params = new URLSearchParams(urlObj.hash.slice(1));
    } else {
      // Fallback to query params (legacy, less secure)
      console.warn("[Auth] Using legacy query params - deploy updated web-auth page for better security");
      params = urlObj.searchParams;
    }

    // Build raw data object for validation
    const rawData = {
      token: params.get("token") ?? "",
      userId: params.get("userId") ?? "",
      email: params.get("email") || undefined,
      firstName: params.get("firstName") || undefined,
      lastName: params.get("lastName") || undefined,
      imageUrl: params.get("imageUrl") || undefined,
      expiresAt: params.get("expiresAt") ? parseInt(params.get("expiresAt")!, 10) : undefined,
      state: params.get("state") ?? "",
    };

    // Validate with Zod schema
    const result = authCallbackSchema.safeParse(rawData);
    if (!result.success) {
      console.error("[Auth] Validation failed:", result.error.format());
      return null;
    }

    // Verify state to prevent CSRF attacks (after validation confirms state exists)
    if (!validateAuthState(result.data.state)) {
      console.error("[Auth] CSRF validation failed - state mismatch");
      return null;
    }

    return result.data;
  } catch (error) {
    console.error("[Auth] Failed to parse auth callback URL:", error);
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
  return listen<string>("deep-link://new-url", async (event) => {
    const url = event.payload;
    console.log("Received deep link:", url);

    if (url.includes("auth-callback")) {
      const payload = parseAuthCallback(url);

      if (payload) {
        // Check for sign-out action
        if (url.includes("action=signed-out")) {
          await clearAuthData();
          onSuccess({ ...payload, token: "", userId: "" });
        } else {
          await storeAuthData(payload);
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
  return await getStoredToken();
}
