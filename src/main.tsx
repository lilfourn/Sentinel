import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { ClerkProvider } from "@clerk/clerk-react";
import { dark } from "@clerk/themes";
import { ConvexProviderWithClerk } from "convex/react-clerk";
import { ConvexReactClient } from "convex/react";
import { useAuth } from "@clerk/clerk-react";
import { AuthSync } from "./components/auth/AuthSync";
import { UsageSync } from "./components/UsageSync";
import { queryClient } from "./lib/query-client";
import { DesktopAuthProvider, useDesktopAuth } from "./contexts/DesktopAuthContext";
import { isTauriProduction } from "./lib/desktop-auth";
import App from "./App";

// === DIAGNOSTIC: Check Tauri availability ===
const TAURI_AVAILABLE = typeof window !== 'undefined' && '__TAURI__' in window;
console.warn('[DIAGNOSTIC] Tauri available:', TAURI_AVAILABLE);
console.warn('[DIAGNOSTIC] window.__TAURI__:', (window as unknown as { __TAURI__?: unknown }).__TAURI__);
console.warn('[DIAGNOSTIC] Current URL:', window.location.href);
console.warn('[DIAGNOSTIC] isTauriProduction():', isTauriProduction());

// Test invoke if Tauri is available
if (TAURI_AVAILABLE) {
  import('@tauri-apps/api/core').then(({ invoke }) => {
    console.warn('[DIAGNOSTIC] Testing invoke...');
    invoke('get_home_directory')
      .then((result) => console.warn('[DIAGNOSTIC] invoke SUCCESS:', result))
      .catch((err) => console.error('[DIAGNOSTIC] invoke FAILED:', err));
  });

  import('@tauri-apps/api/event').then(({ emit, listen }) => {
    console.warn('[DIAGNOSTIC] Testing events...');
    listen('test-event', (e) => console.warn('[DIAGNOSTIC] Event received:', e))
      .then(() => {
        emit('test-event', { test: true });
        console.warn('[DIAGNOSTIC] Event emitted');
      })
      .catch((err) => console.error('[DIAGNOSTIC] Event setup FAILED:', err));
  });
} else {
  console.error('[DIAGNOSTIC] Tauri NOT available - IPC will not work!');
}
// === END DIAGNOSTIC ===

// Clerk publishable key
const PUBLISHABLE_KEY = import.meta.env.VITE_CLERK_PUBLISHABLE_KEY;

// Convex client (only if URL is configured)
const CONVEX_URL = import.meta.env.VITE_CONVEX_URL;
const convex = CONVEX_URL ? new ConvexReactClient(CONVEX_URL) : null;

// Check if we should use desktop auth (production Tauri build)
const USE_DESKTOP_AUTH = isTauriProduction();

// Clerk appearance config - matches app's glassmorphic dark theme
const clerkAppearance = {
  baseTheme: dark,
  variables: {
    colorPrimary: "#cc5500",
    colorTextOnPrimaryBackground: "#ffffff",
    colorBackground: "rgba(30, 30, 30, 0.95)",
    colorInputBackground: "rgba(45, 45, 45, 0.9)",
    colorInputText: "#f5f5f7",
    colorText: "#f5f5f7",
    colorTextSecondary: "#a1a1a6",
    colorDanger: "#ff453a",
    borderRadius: "10px",
  },
  elements: {
    // Card and modal backgrounds
    card: {
      backgroundColor: "rgba(30, 30, 30, 0.95)",
      backdropFilter: "blur(20px)",
      border: "1px solid rgba(255, 255, 255, 0.1)",
      boxShadow: "0 8px 32px rgba(0, 0, 0, 0.4)",
    },
    modalContent: {
      backgroundColor: "rgba(30, 30, 30, 0.95)",
      backdropFilter: "blur(20px)",
    },
    // Header styling
    headerTitle: { color: "#f5f5f7" },
    headerSubtitle: { color: "#a1a1a6" },
    // Form elements
    formButtonPrimary: {
      backgroundColor: "#cc5500",
      "&:hover": { backgroundColor: "#e86a1a" },
    },
    formFieldInput: {
      backgroundColor: "rgba(45, 45, 45, 0.9)",
      borderColor: "rgba(255, 255, 255, 0.1)",
      color: "#f5f5f7",
    },
    formFieldLabel: { color: "#a1a1a6" },
    // Navigation and profile sections
    navbarButton: {
      color: "#d1d1d6",
      "&:hover": { backgroundColor: "rgba(255, 255, 255, 0.1)" },
    },
    profileSectionPrimaryButton: { color: "#cc5500" },
    accordionTriggerButton: { color: "#f5f5f7" },
    // Popover (UserButton dropdown)
    userButtonPopoverCard: {
      backgroundColor: "rgba(30, 30, 30, 0.95)",
      backdropFilter: "blur(20px)",
      border: "1px solid rgba(255, 255, 255, 0.1)",
    },
    userButtonPopoverActionButton: {
      color: "#d1d1d6",
      "&:hover": { backgroundColor: "rgba(255, 255, 255, 0.1)" },
    },
    userButtonPopoverActionButtonText: { color: "#d1d1d6" },
    userButtonPopoverActionButtonIcon: { color: "#a1a1a6" },
    userButtonPopoverFooter: { display: "none" },
    // User profile page
    pageScrollBox: { backgroundColor: "transparent" },
    page: { backgroundColor: "transparent" },
    profilePage: { backgroundColor: "transparent" },
  },
};

// Convex provider for Clerk auth (development mode)
function ConvexClientProviderWithClerk({ children }: { children: React.ReactNode }) {
  if (!convex) {
    return <>{children}</>;
  }
  return (
    <ConvexProviderWithClerk client={convex} useAuth={useAuth}>
      <AuthSync />
      <UsageSync />
      {children}
    </ConvexProviderWithClerk>
  );
}

// Custom hook for desktop auth that matches Clerk's useAuth interface for Convex
function useDesktopAuthForConvex() {
  const { isLoaded, isSignedIn, getToken } = useDesktopAuth();

  return {
    isLoaded,
    isSignedIn,
    getToken: async (_options?: { template?: string; skipCache?: boolean }) => {
      // Desktop auth doesn't support templates, return the stored token
      return getToken();
    },
    orgId: undefined,
    orgRole: undefined,
  };
}

// Convex provider for desktop auth (production mode)
function ConvexClientProviderWithDesktopAuth({ children }: { children: React.ReactNode }) {
  if (!convex) {
    return <>{children}</>;
  }

  return (
    <ConvexProviderWithClerk client={convex} useAuth={useDesktopAuthForConvex}>
      <UsageSync />
      {children}
    </ConvexProviderWithClerk>
  );
}

// App with Clerk auth (development mode)
function AppWithClerkAuth() {
  return (
    <ClerkProvider
      publishableKey={PUBLISHABLE_KEY}
      afterSignOutUrl="/"
      appearance={clerkAppearance}
      allowedRedirectOrigins={[
        "http://localhost:1420",    // Development
        "http://localhost:12753",   // Production desktop app (localhost plugin)
      ]}
    >
      <QueryClientProvider client={queryClient}>
        <ConvexClientProviderWithClerk>
          <App />
        </ConvexClientProviderWithClerk>
      </QueryClientProvider>
    </ClerkProvider>
  );
}

// App with desktop auth (production Tauri mode)
function AppWithDesktopAuth() {
  return (
    <DesktopAuthProvider>
      <QueryClientProvider client={queryClient}>
        <ConvexClientProviderWithDesktopAuth>
          <App />
        </ConvexClientProviderWithDesktopAuth>
      </QueryClientProvider>
    </DesktopAuthProvider>
  );
}

// App without auth
function AppWithoutAuth() {
  return (
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  );
}

// Main app with providers
function Root() {
  // If Clerk is not configured, run without auth
  if (!PUBLISHABLE_KEY) {
    console.info("Running without auth. Set VITE_CLERK_PUBLISHABLE_KEY to enable.");
    return (
      <StrictMode>
        <AppWithoutAuth />
      </StrictMode>
    );
  }

  // Use desktop auth in production Tauri builds
  if (USE_DESKTOP_AUTH) {
    console.info("Using desktop auth (production Tauri build)");
    return (
      <StrictMode>
        <AppWithDesktopAuth />
      </StrictMode>
    );
  }

  // Use Clerk auth in development
  console.info("Using Clerk auth (development mode)");
  return (
    <StrictMode>
      <AppWithClerkAuth />
    </StrictMode>
  );
}

createRoot(document.getElementById("root")!).render(<Root />);
