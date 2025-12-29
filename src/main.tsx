import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ClerkProvider } from "@clerk/clerk-react";
import { dark } from "@clerk/themes";
import { ConvexProviderWithClerk } from "convex/react-clerk";
import { ConvexReactClient } from "convex/react";
import { useAuth } from "@clerk/clerk-react";
import { AuthSync } from "./components/auth/AuthSync";
import App from "./App";

// TanStack Query client for useDirectory hook
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000 * 60,
      retry: 1,
    },
  },
});

// Clerk publishable key
const PUBLISHABLE_KEY = import.meta.env.VITE_CLERK_PUBLISHABLE_KEY;

// Convex client (only if URL is configured)
const CONVEX_URL = import.meta.env.VITE_CONVEX_URL;
const convex = CONVEX_URL ? new ConvexReactClient(CONVEX_URL) : null;

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

// Inner component that uses useAuth (must be inside ClerkProvider)
function ConvexClientProvider({ children }: { children: React.ReactNode }) {
  if (!convex) {
    return <>{children}</>;
  }
  return (
    <ConvexProviderWithClerk client={convex} useAuth={useAuth}>
      <AuthSync />
      {children}
    </ConvexProviderWithClerk>
  );
}

// Main app with providers
function Root() {
  // If Clerk is not configured, run without auth
  if (!PUBLISHABLE_KEY) {
    console.info("Running without Clerk auth. Set VITE_CLERK_PUBLISHABLE_KEY to enable.");
    return (
      <StrictMode>
        <QueryClientProvider client={queryClient}>
          <App />
        </QueryClientProvider>
      </StrictMode>
    );
  }

  return (
    <StrictMode>
      <ClerkProvider publishableKey={PUBLISHABLE_KEY} afterSignOutUrl="/" appearance={clerkAppearance}>
        <QueryClientProvider client={queryClient}>
          <ConvexClientProvider>
            <App />
          </ConvexClientProvider>
        </QueryClientProvider>
      </ClerkProvider>
    </StrictMode>
  );
}

createRoot(document.getElementById("root")!).render(<Root />);
