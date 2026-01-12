# AGENTS.md - Sentinel

## Commands
```bash
npm run tauri dev           # Dev server (Vite + Tauri)
npm run build               # Type check (tsc && vite build)
npm test                    # Run vitest watch mode
npm run test:run            # Run tests once
npx vitest run path/to/file # Single test file
cargo check                 # Rust type check (run from src-tauri/)
cargo test                  # Rust tests (run from src-tauri/)
cargo test test_name        # Single Rust test
```

## Architecture
Tauri v2 desktop app: React 19 frontend (src/) + Rust backend (src-tauri/). Frontend uses Zustand stores, TailwindCSS v4, TanStack Query. Backend exposes Tauri commands for AI chat, VFS simulation, WAL recovery, and file operations. AI uses Claude API (Haiku/Sonnet/Opus).

## Code Style
- **Frontend**: TypeScript strict, functional React components, Zustand for state, `invoke()` for Tauri IPC
- **Backend**: Rust 2021 edition, `thiserror` for errors, `tokio` async, `tracing` for logs
- **Types**: Keep frontend (src/types/) and backend (src-tauri/src/models/) in sync
- **Imports**: Group by external → internal; use absolute paths in TS
- **Naming**: camelCase (TS), snake_case (Rust); prefix Tauri commands with module name
