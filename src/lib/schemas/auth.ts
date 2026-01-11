/**
 * Zod schemas for authentication data validation
 *
 * Provides runtime type safety for auth payloads received from
 * external sources (deep links, localStorage, etc.)
 */

import { z } from "zod";

/**
 * Schema for auth callback payload from deep link
 * Validates data received from the web auth page
 */
export const authCallbackSchema = z.object({
  token: z.string().min(1, "Token is required"),
  userId: z.string().min(1, "User ID is required"),
  email: z
    .string()
    .email("Invalid email format")
    .optional()
    .or(z.literal("")),
  firstName: z.string().optional(),
  lastName: z.string().optional(),
  imageUrl: z
    .string()
    .url("Invalid image URL")
    .optional()
    .or(z.literal("")),
  expiresAt: z.number().positive("Expiry must be a positive timestamp").optional(),
  state: z.string().min(1, "State is required for CSRF protection"),
});

export type AuthCallbackPayload = z.infer<typeof authCallbackSchema>;

/**
 * Schema for stored user data
 * Validates user data retrieved from storage
 */
export const storedUserSchema = z.object({
  id: z.string().min(1),
  email: z.string().email().nullable(),
  firstName: z.string().nullable(),
  lastName: z.string().nullable(),
  imageUrl: z.string().url().nullable().or(z.literal("")),
});

export type StoredUser = z.infer<typeof storedUserSchema>;

/**
 * Parse and validate auth callback data
 * Returns validated payload or null if invalid
 */
export function parseAuthCallbackData(data: unknown): AuthCallbackPayload | null {
  const result = authCallbackSchema.safeParse(data);
  if (!result.success) {
    console.error("[Auth] Validation failed:", result.error.format());
    return null;
  }
  return result.data;
}

/**
 * Parse and validate stored user data
 * Returns validated user or null if invalid
 */
export function parseStoredUser(data: unknown): StoredUser | null {
  const result = storedUserSchema.safeParse(data);
  if (!result.success) {
    console.error("[Auth] Stored user validation failed:", result.error.format());
    return null;
  }
  return result.data;
}
