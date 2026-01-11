/**
 * Secure Storage Module
 *
 * Provides secure storage for sensitive data like auth tokens using
 * Tauri's store plugin. Data is stored in an encrypted file on disk
 * rather than in localStorage which is accessible to any script.
 *
 * Falls back to localStorage if store plugin is not available (web mode).
 */

import { Store } from "@tauri-apps/plugin-store";
import { isTauri } from "./desktop-auth";

// Store filename - stored in app data directory
const STORE_NAME = ".auth.dat";

// Storage keys
const KEYS = {
  token: "auth_token",
  user: "auth_user",
  expiresAt: "auth_expires_at",
} as const;

// Singleton store instance
let store: Store | null = null;

/**
 * Get or create the secure store instance
 */
async function getStore(): Promise<Store | null> {
  if (!isTauri()) {
    return null;
  }

  if (!store) {
    try {
      store = await Store.load(STORE_NAME);
    } catch (error) {
      console.error("[SecureStorage] Failed to load store:", error);
      return null;
    }
  }
  return store;
}

/**
 * Store a value securely
 */
export async function secureSet(key: string, value: string): Promise<boolean> {
  const s = await getStore();
  if (s) {
    try {
      await s.set(key, value);
      await s.save();
      return true;
    } catch (error) {
      console.error("[SecureStorage] Failed to set value:", error);
    }
  }
  // Fallback to localStorage
  try {
    localStorage.setItem(key, value);
    return true;
  } catch {
    return false;
  }
}

/**
 * Get a value from secure storage
 */
export async function secureGet(key: string): Promise<string | null> {
  const s = await getStore();
  if (s) {
    try {
      const value = await s.get<string>(key);
      return value ?? null;
    } catch (error) {
      console.error("[SecureStorage] Failed to get value:", error);
    }
  }
  // Fallback to localStorage
  return localStorage.getItem(key);
}

/**
 * Delete a value from secure storage
 */
export async function secureDelete(key: string): Promise<boolean> {
  const s = await getStore();
  if (s) {
    try {
      await s.delete(key);
      await s.save();
    } catch (error) {
      console.error("[SecureStorage] Failed to delete value:", error);
    }
  }
  // Also clear from localStorage (migration cleanup)
  try {
    localStorage.removeItem(key);
    return true;
  } catch {
    return false;
  }
}

/**
 * Store auth token securely
 */
export async function storeToken(token: string): Promise<boolean> {
  return secureSet(KEYS.token, token);
}

/**
 * Get auth token from secure storage
 */
export async function getToken(): Promise<string | null> {
  return secureGet(KEYS.token);
}

/**
 * Store user data securely
 */
export async function storeUser(userData: object): Promise<boolean> {
  return secureSet(KEYS.user, JSON.stringify(userData));
}

/**
 * Get user data from secure storage
 */
export async function getUser<T>(): Promise<T | null> {
  const data = await secureGet(KEYS.user);
  if (!data) return null;
  try {
    return JSON.parse(data) as T;
  } catch {
    return null;
  }
}

/**
 * Store token expiry timestamp
 */
export async function storeExpiry(expiresAt: number): Promise<boolean> {
  return secureSet(KEYS.expiresAt, expiresAt.toString());
}

/**
 * Get token expiry timestamp
 */
export async function getExpiry(): Promise<number | null> {
  const data = await secureGet(KEYS.expiresAt);
  if (!data) return null;
  const parsed = parseInt(data, 10);
  return isNaN(parsed) ? null : parsed;
}

/**
 * Clear all auth data from secure storage
 */
export async function clearAllAuth(): Promise<void> {
  await secureDelete(KEYS.token);
  await secureDelete(KEYS.user);
  await secureDelete(KEYS.expiresAt);

  // Also clear legacy localStorage keys for migration
  localStorage.removeItem("sentinel_auth_token");
  localStorage.removeItem("sentinel_auth_user");
  localStorage.removeItem("sentinel_auth_token_expires");
}

/**
 * Check if token is expired
 */
export async function isTokenExpired(): Promise<boolean> {
  const expiresAt = await getExpiry();
  if (!expiresAt) return false; // No expiry = not expired
  return Date.now() > expiresAt;
}

/**
 * Migrate from localStorage to secure storage (one-time)
 */
export async function migrateFromLocalStorage(): Promise<void> {
  const s = await getStore();
  if (!s) return; // Can't migrate without store

  // Check if already migrated
  const existingToken = await s.get<string>(KEYS.token);
  if (existingToken) return; // Already have data in secure store

  // Migrate token
  const token = localStorage.getItem("sentinel_auth_token");
  if (token) {
    await storeToken(token);
    localStorage.removeItem("sentinel_auth_token");
  }

  // Migrate user
  const user = localStorage.getItem("sentinel_auth_user");
  if (user) {
    await secureSet(KEYS.user, user);
    localStorage.removeItem("sentinel_auth_user");
  }

  // Migrate expiry
  const expiry = localStorage.getItem("sentinel_auth_token_expires");
  if (expiry) {
    await secureSet(KEYS.expiresAt, expiry);
    localStorage.removeItem("sentinel_auth_token_expires");
  }

  console.log("[SecureStorage] Migration from localStorage complete");
}
