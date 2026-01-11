import { ReactNode } from "react";
import { SignedIn, SignedOut, SignIn, useAuth } from "@clerk/clerk-react";
import { Loader2, FolderTree, Sparkles, Shield, Zap, ExternalLink } from "lucide-react";
import { isTauriProduction } from "../../lib/desktop-auth";
import { useDesktopAuth } from "../../contexts/DesktopAuthContext";

interface AuthGuardProps {
  children: ReactNode;
}

/**
 * Auth guard that shows sign-in when not authenticated
 * Wraps protected content and handles loading states
 * Works with both Clerk (dev) and Desktop Auth (production)
 */
export function AuthGuard({ children }: AuthGuardProps) {
  // Use desktop auth in production Tauri builds
  if (isTauriProduction()) {
    return <DesktopAuthGuard>{children}</DesktopAuthGuard>;
  }

  // Use Clerk auth in development
  return <ClerkAuthGuard>{children}</ClerkAuthGuard>;
}

/**
 * Auth guard using Clerk (for development)
 */
function ClerkAuthGuard({ children }: AuthGuardProps) {
  const { isLoaded } = useAuth();

  // Show loading while Clerk initializes
  if (!isLoaded) {
    return <LoadingScreen message="Initializing" />;
  }

  return (
    <>
      <SignedIn>{children}</SignedIn>
      <SignedOut>
        <AuthPage />
      </SignedOut>
    </>
  );
}

/**
 * Auth guard using desktop auth (for production Tauri)
 */
function DesktopAuthGuard({ children }: AuthGuardProps) {
  const { isLoaded, isSignedIn, signIn } = useDesktopAuth();

  // Show loading while initializing
  if (!isLoaded) {
    return <LoadingScreen message="Initializing" />;
  }

  // Show sign-in page if not authenticated
  if (!isSignedIn) {
    return <DesktopAuthPage onSignIn={signIn} />;
  }

  return <>{children}</>;
}

/**
 * Loading screen component
 */
function LoadingScreen({ message }: { message: string }) {
  return (
    <div className="h-screen w-screen flex items-center justify-center bg-[#1E1E1E]">
      <div className="relative flex flex-col items-center gap-6">
        <div className="relative">
          <div className="absolute -inset-6 bg-[#f9943b]/10 rounded-full blur-2xl" />
          <img
            src="/sentinal-logo.svg"
            alt="Sentinel"
            className="relative w-14 h-14"
          />
        </div>
        <div className="flex items-center gap-3">
          <Loader2 className="w-4 h-4 animate-spin text-[#f9943b]" />
          <span className="text-[11px] text-zinc-500 uppercase tracking-[0.2em] font-medium">
            {message}
          </span>
        </div>
      </div>
    </div>
  );
}

/**
 * Desktop auth page - opens browser for sign-in
 */
function DesktopAuthPage({ onSignIn }: { onSignIn: () => Promise<void> }) {
  return (
    <div className="h-screen w-full grid grid-cols-1 lg:grid-cols-2 bg-[#1E1E1E]">
      {/* Left Panel - Auth Form */}
      <div className="relative flex items-center justify-center px-8 sm:px-12 lg:px-16 xl:px-20 py-12">
        {/* Subtle background gradient */}
        <div
          className="absolute inset-0 pointer-events-none"
          style={{
            background: 'radial-gradient(ellipse 80% 60% at 50% 50%, rgba(249, 148, 59, 0.03) 0%, transparent 50%)',
          }}
        />

        <div className="relative z-10 w-full max-w-[400px]">
          {/* Logo - visible on mobile only */}
          <div className="flex items-center gap-3 mb-10 lg:hidden">
            <img src="/sentinal-logo.svg" alt="Sentinel" className="w-10 h-10" />
            <div>
              <h1 className="text-lg font-semibold text-white">Sentinel</h1>
              <p className="text-[10px] text-zinc-500 uppercase tracking-[0.15em]">File System</p>
            </div>
          </div>

          {/* Desktop sign-in card */}
          <div className="bg-[#252525] shadow-2xl rounded-2xl p-8 border border-white/[0.06]">
            <h2 className="text-white text-2xl font-semibold tracking-tight mb-2">
              Welcome back
            </h2>
            <p className="text-zinc-400 text-sm mb-8">
              Sign in to continue to Sentinel
            </p>

            <button
              onClick={onSignIn}
              className="w-full flex items-center justify-center gap-3 bg-[#f9943b] hover:bg-[#ffa54d] text-white font-semibold text-[14px] h-12 rounded-lg shadow-lg shadow-[#f9943b]/20 hover:shadow-xl hover:shadow-[#f9943b]/30 border-0 transition-all duration-200 active:scale-[0.98]"
            >
              <ExternalLink className="w-4 h-4" />
              Continue in Browser
            </button>

            <p className="mt-6 text-center text-zinc-500 text-[12px]">
              You'll be redirected to sign in securely in your browser
            </p>
          </div>
        </div>
      </div>

      {/* Right Panel - Branding */}
      <div className="hidden lg:flex relative overflow-hidden bg-[#161616]">
        {/* Animated gradient background */}
        <div
          className="absolute inset-0"
          style={{
            background: `
              radial-gradient(ellipse 100% 100% at 100% 0%, rgba(249, 148, 59, 0.08) 0%, transparent 50%),
              radial-gradient(ellipse 80% 80% at 0% 100%, rgba(249, 148, 59, 0.05) 0%, transparent 50%)
            `,
          }}
        />

        {/* Grid pattern */}
        <div
          className="absolute inset-0 opacity-[0.03]"
          style={{
            backgroundImage: `
              linear-gradient(rgba(255,255,255,0.5) 1px, transparent 1px),
              linear-gradient(90deg, rgba(255,255,255,0.5) 1px, transparent 1px)
            `,
            backgroundSize: '60px 60px',
          }}
        />

        {/* Content */}
        <div className="relative z-10 flex flex-col justify-between w-full p-12 xl:p-16">
          {/* Top - Logo */}
          <div className="flex items-center gap-3">
            <img src="/sentinal-logo.svg" alt="Sentinel" className="w-12 h-12" />
            <div>
              <h1 className="text-xl font-semibold text-white tracking-tight">Sentinel</h1>
              <p className="text-[10px] text-zinc-500 uppercase tracking-[0.2em] font-medium">
                AI-Powered File System
              </p>
            </div>
          </div>

          {/* Center - Main messaging */}
          <div className="flex-1 flex flex-col justify-center py-12">
            <h2 className="text-[42px] xl:text-[48px] font-bold text-white leading-[1.1] tracking-tight mb-6">
              Your files,
              <br />
              <span className="text-[#f9943b]">intelligently</span>
              <br />
              organized.
            </h2>
            <p className="text-[16px] text-zinc-400 leading-relaxed max-w-[400px]">
              Let AI handle the chaos. Sentinel automatically organizes, renames,
              and structures your files so you can focus on what matters.
            </p>

            {/* Feature highlights */}
            <div className="mt-10 space-y-4">
              <FeatureItem
                icon={<Sparkles className="w-4 h-4" />}
                text="AI-powered auto-organization"
              />
              <FeatureItem
                icon={<FolderTree className="w-4 h-4" />}
                text="Smart folder structures"
              />
              <FeatureItem
                icon={<Zap className="w-4 h-4" />}
                text="Instant file renaming"
              />
              <FeatureItem
                icon={<Shield className="w-4 h-4" />}
                text="Secure local processing"
              />
            </div>
          </div>

          {/* Bottom - Decorative */}
          <div className="flex items-center gap-3 text-zinc-600">
            <div className="w-8 h-px bg-zinc-700" />
            <span className="text-[11px] uppercase tracking-[0.2em]">Built for power users</span>
          </div>
        </div>

        {/* Decorative elements */}
        <div className="absolute top-0 right-0 w-[500px] h-[500px] pointer-events-none">
          <div
            className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[300px] h-[300px] rounded-full"
            style={{
              background: 'radial-gradient(circle, rgba(249, 148, 59, 0.1) 0%, transparent 70%)',
              filter: 'blur(60px)',
            }}
          />
        </div>

        {/* Corner accent */}
        <div className="absolute bottom-0 right-0 w-64 h-64 pointer-events-none">
          <div className="absolute bottom-8 right-8 w-32 h-px bg-gradient-to-l from-[#f9943b]/20 to-transparent" />
          <div className="absolute bottom-8 right-8 w-px h-32 bg-gradient-to-t from-[#f9943b]/20 to-transparent" />
        </div>
      </div>
    </div>
  );
}

/**
 * Clerk appearance configuration with high contrast dark theme
 */
const clerkAppearance = {
  layout: {
    socialButtonsPlacement: "bottom" as const,
    socialButtonsVariant: "blockButton" as const,
    showOptionalFields: false,
  },
  variables: {
    colorPrimary: "#f9943b",
    colorBackground: "#252525",
    colorText: "#ffffff",
    colorTextSecondary: "#a1a1aa",
    colorInputText: "#ffffff",
    colorInputBackground: "#2a2a2a",
    borderRadius: "10px",
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
  },
  elements: {
    rootBox: "w-full",
    card: "bg-[#252525] shadow-2xl rounded-2xl p-8 border border-white/[0.06]",

    // Show Clerk's built-in header
    headerTitle: "text-white text-2xl font-semibold tracking-tight",
    headerSubtitle: "text-zinc-400 text-sm",

    main: "gap-5",
    form: "gap-5",
    formFieldRow: "gap-5",

    // High contrast labels
    formFieldLabel: "text-zinc-300 text-[12px] uppercase tracking-[0.1em] font-semibold mb-2",

    // High contrast inputs with better visibility
    formFieldInput: `
      bg-[#1a1a1a] border border-zinc-700 text-white placeholder:text-zinc-500
      rounded-lg h-12 px-4 text-[14px] transition-all duration-200
      focus:border-[#f9943b] focus:ring-2 focus:ring-[#f9943b]/30 focus:bg-[#1a1a1a]
      hover:border-zinc-600
    `,
    formFieldInputShowPasswordButton: "text-zinc-400 hover:text-white transition-colors",
    formFieldErrorText: "text-red-400 text-[12px] mt-2 font-medium",
    formFieldSuccessText: "text-emerald-400 text-[12px] mt-2 font-medium",

    // Primary button
    formButtonPrimary: `
      bg-[#f9943b] hover:bg-[#ffa54d]
      text-white font-semibold text-[14px] h-12 rounded-lg
      shadow-lg shadow-[#f9943b]/20
      hover:shadow-xl hover:shadow-[#f9943b]/30
      border-0 transition-all duration-200 active:scale-[0.98]
    `,

    // Social buttons with better contrast
    socialButtons: "gap-3",
    socialButtonsBlockButton: `
      bg-[#2a2a2a] border border-zinc-700 text-white font-medium text-[14px]
      h-12 rounded-lg transition-all duration-200
      hover:bg-[#333333] hover:border-zinc-600 active:scale-[0.98]
    `,
    socialButtonsBlockButtonText: "!text-white font-medium",
    socialButtonsBlockButtonArrow: "text-white",

    // Divider
    dividerLine: "bg-zinc-700",
    dividerText: "text-zinc-500 text-[11px] uppercase tracking-[0.15em] font-medium bg-[#252525] px-4",

    // Footer with sign up link
    footer: "bg-transparent pt-4",
    footerAction: "bg-transparent",
    footerActionText: "text-zinc-400 text-[13px]",
    footerActionLink: "text-[#f9943b] hover:text-[#ffa54d] font-semibold transition-colors",

    // Alternative actions
    formFieldAction: "text-[12px]",
    formFieldActionLink: "text-[#f9943b] hover:text-[#ffa54d] transition-colors font-medium",

    // Identity preview
    identityPreview: "bg-[#1a1a1a] border border-zinc-700 rounded-lg p-4",
    identityPreviewText: "text-white text-[14px]",
    identityPreviewEditButton: "text-[#f9943b] hover:text-[#ffa54d] text-[13px] font-medium",

    // OTP input
    otpCodeFieldInput: "bg-[#1a1a1a] border border-zinc-700 text-white rounded-lg text-lg font-mono",

    // Alerts
    alert: "bg-[#1a1a1a] border border-zinc-700 rounded-lg p-4",
    alertText: "text-zinc-300 text-[13px]",

    // Back button
    backLink: "text-zinc-400 hover:text-white text-[13px] font-medium transition-colors",

    // Internal links
    formHeaderTitle: "text-white",
    formHeaderSubtitle: "text-zinc-400",
  },
};

/**
 * Split-view authentication page (for Clerk)
 */
function AuthPage() {
  return (
    <div className="h-screen w-full grid grid-cols-1 lg:grid-cols-2 bg-[#1E1E1E]">
      {/* Left Panel - Auth Form */}
      <div className="relative flex items-center justify-center px-8 sm:px-12 lg:px-16 xl:px-20 py-12">
        {/* Subtle background gradient */}
        <div
          className="absolute inset-0 pointer-events-none"
          style={{
            background: 'radial-gradient(ellipse 80% 60% at 50% 50%, rgba(249, 148, 59, 0.03) 0%, transparent 50%)',
          }}
        />

        <div className="relative z-10 w-full max-w-[400px]">
          {/* Logo - visible on mobile only */}
          <div className="flex items-center gap-3 mb-10 lg:hidden">
            <img src="/sentinal-logo.svg" alt="Sentinel" className="w-10 h-10" />
            <div>
              <h1 className="text-lg font-semibold text-white">Sentinel</h1>
              <p className="text-[10px] text-zinc-500 uppercase tracking-[0.15em]">File System</p>
            </div>
          </div>

          {/* Auth form - Clerk handles sign in/sign up switching via footer links */}
          <SignIn appearance={clerkAppearance} />
        </div>
      </div>

      {/* Right Panel - Branding (same as desktop auth page) */}
      <BrandingPanel />
    </div>
  );
}

/**
 * Branding panel component (shared between auth pages)
 */
function BrandingPanel() {
  return (
    <div className="hidden lg:flex relative overflow-hidden bg-[#161616]">
      {/* Animated gradient background */}
      <div
        className="absolute inset-0"
        style={{
          background: `
            radial-gradient(ellipse 100% 100% at 100% 0%, rgba(249, 148, 59, 0.08) 0%, transparent 50%),
            radial-gradient(ellipse 80% 80% at 0% 100%, rgba(249, 148, 59, 0.05) 0%, transparent 50%)
          `,
        }}
      />

      {/* Grid pattern */}
      <div
        className="absolute inset-0 opacity-[0.03]"
        style={{
          backgroundImage: `
            linear-gradient(rgba(255,255,255,0.5) 1px, transparent 1px),
            linear-gradient(90deg, rgba(255,255,255,0.5) 1px, transparent 1px)
          `,
          backgroundSize: '60px 60px',
        }}
      />

      {/* Content */}
      <div className="relative z-10 flex flex-col justify-between w-full p-12 xl:p-16">
        {/* Top - Logo */}
        <div className="flex items-center gap-3">
          <img src="/sentinal-logo.svg" alt="Sentinel" className="w-12 h-12" />
          <div>
            <h1 className="text-xl font-semibold text-white tracking-tight">Sentinel</h1>
            <p className="text-[10px] text-zinc-500 uppercase tracking-[0.2em] font-medium">
              AI-Powered File System
            </p>
          </div>
        </div>

        {/* Center - Main messaging */}
        <div className="flex-1 flex flex-col justify-center py-12">
          <h2 className="text-[42px] xl:text-[48px] font-bold text-white leading-[1.1] tracking-tight mb-6">
            Your files,
            <br />
            <span className="text-[#f9943b]">intelligently</span>
            <br />
            organized.
          </h2>
          <p className="text-[16px] text-zinc-400 leading-relaxed max-w-[400px]">
            Let AI handle the chaos. Sentinel automatically organizes, renames,
            and structures your files so you can focus on what matters.
          </p>

          {/* Feature highlights */}
          <div className="mt-10 space-y-4">
            <FeatureItem
              icon={<Sparkles className="w-4 h-4" />}
              text="AI-powered auto-organization"
            />
            <FeatureItem
              icon={<FolderTree className="w-4 h-4" />}
              text="Smart folder structures"
            />
            <FeatureItem
              icon={<Zap className="w-4 h-4" />}
              text="Instant file renaming"
            />
            <FeatureItem
              icon={<Shield className="w-4 h-4" />}
              text="Secure local processing"
            />
          </div>
        </div>

        {/* Bottom - Decorative */}
        <div className="flex items-center gap-3 text-zinc-600">
          <div className="w-8 h-px bg-zinc-700" />
          <span className="text-[11px] uppercase tracking-[0.2em]">Built for power users</span>
        </div>
      </div>

      {/* Decorative elements */}
      <div className="absolute top-0 right-0 w-[500px] h-[500px] pointer-events-none">
        <div
          className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[300px] h-[300px] rounded-full"
          style={{
            background: 'radial-gradient(circle, rgba(249, 148, 59, 0.1) 0%, transparent 70%)',
            filter: 'blur(60px)',
          }}
        />
      </div>

      {/* Corner accent */}
      <div className="absolute bottom-0 right-0 w-64 h-64 pointer-events-none">
        <div className="absolute bottom-8 right-8 w-32 h-px bg-gradient-to-l from-[#f9943b]/20 to-transparent" />
        <div className="absolute bottom-8 right-8 w-px h-32 bg-gradient-to-t from-[#f9943b]/20 to-transparent" />
      </div>
    </div>
  );
}

/**
 * Feature item component for the branding panel
 */
function FeatureItem({ icon, text }: { icon: React.ReactNode; text: string }) {
  return (
    <div className="flex items-center gap-3 text-zinc-400">
      <div className="flex items-center justify-center w-8 h-8 rounded-lg bg-[#f9943b]/10 text-[#f9943b]">
        {icon}
      </div>
      <span className="text-[14px]">{text}</span>
    </div>
  );
}
