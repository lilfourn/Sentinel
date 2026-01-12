import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// @ts-expect-error process is a nodejs global
const isProd = process.env.NODE_ENV === 'production';

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  // Use relative paths for Tauri production builds
  base: './',

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,

  // === BUILD OPTIMIZATION ===
  build: {
    target: 'es2020',
    minify: 'esbuild',
    sourcemap: false,
    rollupOptions: {
      output: {
        // Manual chunk splitting for optimal caching
        manualChunks: {
          // Core React - rarely changes
          'react-vendor': ['react', 'react-dom'],
          // UI libraries
          'ui-vendor': ['lucide-react', 'class-variance-authority', 'clsx', 'tailwind-merge'],
          // State management
          'state-vendor': ['zustand', '@tanstack/react-query'],
          // Tauri APIs
          'tauri-vendor': [
            '@tauri-apps/api',
            '@tauri-apps/plugin-fs',
            '@tauri-apps/plugin-dialog',
            '@tauri-apps/plugin-opener',
            '@tauri-apps/plugin-process',
            '@tauri-apps/plugin-store',
          ],
        },
      },
    },
    // Chunk size warnings
    chunkSizeWarningLimit: 250,
    // CSS optimization
    cssCodeSplit: true,
    cssMinify: true,
  },

  // === ESBUILD OPTIONS ===
  esbuild: {
    // Only remove debugger in production - keep console.warn/error for error visibility
    drop: isProd ? ['debugger'] : [],
    // Remove console.log but keep warn/error for production debugging
    pure: isProd ? ['console.log'] : [],
    legalComments: 'none',
  },

  // === DEPENDENCY PRE-BUNDLING ===
  optimizeDeps: {
    include: [
      'react',
      'react-dom',
      'zustand',
      '@tanstack/react-query',
      'lucide-react',
      'clsx',
      'tailwind-merge',
    ],
  },

  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
