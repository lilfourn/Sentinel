# Getting Started

This guide will help you set up Sentinel for development and understand the core workflows.

## Prerequisites

Before you begin, ensure you have the following installed:

### Required Software

| Tool | Version | Purpose | Installation |
|------|---------|---------|--------------|
| **Rust** | 1.70+ | Backend compilation | [rustup.rs](https://rustup.rs/) |
| **Node.js** | 18+ | Frontend tooling | [nodejs.org](https://nodejs.org/) |
| **npm** | 9+ | Package management | (comes with Node.js) |

### API Keys

You'll need an Anthropic API key to use AI features:

1. Go to [console.anthropic.com](https://console.anthropic.com/)
2. Create an account or sign in
3. Navigate to API Keys section
4. Generate a new API key
5. Keep it safe—you'll enter it in Sentinel's settings

## Installation

### 1. Clone the Repository

```bash
git clone https://github.com/lilfourn/sentinel.git
cd sentinel
```

### 2. Install Dependencies

```bash
# Install Node.js dependencies
npm install

# Rust dependencies are automatically installed during build
```

### 3. Set Up Environment (Optional)

Create a `.env` file for development configuration:

```bash
cp .env.example .env
```

Edit `.env` if needed:

```env
# Optional: Set default model
VITE_DEFAULT_MODEL=claude-sonnet-4.5

# Optional: Enable debug logging
RUST_LOG=debug
```

## Running Sentinel

### Development Mode

```bash
npm run tauri dev
```

This command:
1. Starts Vite dev server (React hot reload)
2. Compiles Rust backend
3. Launches Tauri window

**First build takes 5-10 minutes** as Rust compiles all dependencies. Subsequent builds are much faster (~30 seconds).

### Production Build

```bash
npm run tauri build
```

Creates optimized installers in `src-tauri/target/release/bundle/`:
- **macOS**: `.dmg` and `.app`
- **Windows**: `.msi` and `.exe`
- **Linux**: `.deb` and `.AppImage`

## First Run Setup

### 1. Launch Sentinel

After running `npm run tauri dev`, the app window opens.

### 2. Configure API Key

1. Click the ⚙️ gear icon in the top-right corner
2. Click "Settings"
3. Enter your Anthropic API key
4. Click "Save"

The key is securely stored in your system's keychain/credential manager.

### 3. Navigate to a Folder

Use the sidebar to browse to any folder on your system, or:
- Press `Cmd+O` (macOS) or `Ctrl+O` (Windows/Linux) to open folder picker
- Drag and drop a folder onto the window

## Quick Tour

### File Browsing

Sentinel supports three view modes:

| View | Icon | Description | Shortcut |
|------|------|-------------|----------|
| **Grid** | ⊞ | Thumbnail grid with icons | `Cmd+1` |
| **List** | ≡ | Compact list with details | `Cmd+2` |
| **Columns** | ⋮⋮⋮ | Miller columns (macOS Finder-style) | `Cmd+3` |

### AI Chat

Click the chat icon (💬) to open the AI assistant:

```
You: What types of files are in this folder?

Sentinel: I found 234 files across these categories:
  • Documents (PDF, DOCX): 89 files
  • Images (JPG, PNG): 112 files
  • Spreadsheets (XLSX): 21 files
  • Other: 12 files
```

**@Mentions**: Type `@` to mention specific files or folders for context.

### Organization Workflow

1. **Start Organization**
   - Click "Organize" button in the sidebar
   - Describe how you want files organized

2. **AI Analyzes Folder**
   - Watch progress in the Changes Panel
   - See agent thoughts and decision-making

3. **Preview Changes**
   - Review the organization plan
   - See before/after structure
   - Check for conflicts

4. **Execute Safely**
   - Click "Execute Plan"
   - Operations journal to WAL
   - Monitor progress with real-time updates

5. **Completion**
   - Review results
   - Undo if needed (coming soon)

## Common Tasks

### Organizing by File Type

```
You: Organize files by type: documents, images, and media
```

Sentinel creates:
```
Documents/
  ├── Contracts/
  ├── Invoices/
  └── Reports/
Images/
  ├── Photos/
  └── Screenshots/
Media/
  ├── Audio/
  └── Video/
```

### Finding Files Semantically

```
You: Find all tax-related documents from 2024
```

Sentinel searches by content meaning, not just filenames:
- `2024_taxes.pdf` ✓
- `w2_form_acme_corp.pdf` ✓
- `quarterly_estimated_payments.xlsx` ✓
- `vacation_photos.jpg` ✗

### Project-Based Organization

```
You: Organize by project, separate contracts and deliverables
```

Sentinel analyzes file contents and creates:
```
Projects/
  ├── Acme/
  │   ├── Contracts/
  │   └── Deliverables/
  ├── Henderson/
  │   ├── Contracts/
  │   └── Deliverables/
  └── Initech/
      ├── Contracts/
      └── Deliverables/
```

## Development Workflow

### File Structure

```
sentinel/
├── src/                    # React frontend
│   ├── components/        # UI components
│   ├── stores/           # Zustand stores
│   ├── hooks/            # Custom hooks
│   └── types/            # TypeScript types
│
├── src-tauri/             # Rust backend
│   ├── src/
│   │   ├── commands/     # Tauri command handlers
│   │   ├── ai/          # AI agents and tools
│   │   ├── vfs/         # Virtual filesystem
│   │   ├── wal/         # Write-ahead log
│   │   └── execution/   # Execution engine
│   └── Cargo.toml        # Rust dependencies
│
└── docs/                  # Documentation (you are here!)
```

### Hot Reload

**Frontend changes**: Instant hot reload (Vite)

**Backend changes**: Automatic recompile and restart

**Type changes**: Update both frontend and backend type definitions to keep them in sync.

### Debugging

#### Frontend Debugging

Open Chrome DevTools in the Tauri window:
- Right-click → Inspect Element
- Or enable devtools in `tauri.conf.json`:

```json
{
  "build": {
    "devPath": "http://localhost:1420",
    "devtools": true
  }
}
```

#### Backend Debugging

View Rust logs in the terminal:

```bash
# Enable debug logging
RUST_LOG=debug npm run tauri dev

# Filter by module
RUST_LOG=sentinel::ai::chat=trace npm run tauri dev
```

Log levels: `error`, `warn`, `info`, `debug`, `trace`

#### AI Request/Response Debugging

Watch AI interactions:

```bash
RUST_LOG=sentinel::ai=debug npm run tauri dev
```

Look for:
- `[AI] Request:` - Outgoing API requests
- `[AI] Response:` - Claude's responses
- `[AgenticLoop]` - Agent iteration progress
- `[ChatAgent]` - Chat tool executions

### Running Tests

#### Frontend Tests

```bash
# Run all tests
npm test

# Run with coverage
npm run test:coverage

# Watch mode
npm run test -- --watch
```

#### Backend Tests

```bash
cd src-tauri

# Run all tests
cargo test

# Run specific test
cargo test test_vfs_simulation

# With output
cargo test -- --nocapture
```

### Type Checking

```bash
# TypeScript type checking
npm run build  # Includes tsc

# Rust type checking (faster than full build)
cd src-tauri
cargo check
```

## Troubleshooting

### Issue: First build takes forever

**Solution**: First Rust compilation compiles all dependencies. Grab coffee ☕—it's normal! Subsequent builds are much faster.

### Issue: API key not saved

**Solution**: Ensure Sentinel has keychain access. On macOS, check System Preferences → Security & Privacy → Privacy → Keychain.

### Issue: "Command not found" errors

**Solution**: Ensure Rust and Node.js are in your `PATH`:

```bash
# Check installations
rustc --version
node --version
npm --version
```

### Issue: Tauri window doesn't open

**Solution**: Check terminal for errors. Common causes:
- Port 1420 already in use (kill other Vite instances)
- Rust compilation failed (check error messages)

### Issue: Hot reload not working

**Solution**:
1. Stop dev server (`Ctrl+C`)
2. Clear Vite cache: `rm -rf node_modules/.vite`
3. Restart: `npm run tauri dev`

### Issue: Type errors after pulling latest changes

**Solution**: Regenerate type bindings:

```bash
# Frontend types
npm run build

# Backend types
cd src-tauri
cargo build
```

## Project Configuration Files

### `tauri.conf.json`

Tauri app configuration:

```json
{
  "build": {
    "devPath": "http://localhost:1420",
    "distDir": "../dist"
  },
  "tauri": {
    "bundle": {
      "identifier": "com.sentinel.app",
      "icon": ["icons/icon.png"]
    },
    "allowlist": {
      "fs": { "all": true },
      "shell": { "all": true }
    }
  }
}
```

### `vite.config.ts`

Vite build configuration:

```typescript
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 1420,
    strictPort: true,
  },
  clearScreen: false,
});
```

### `Cargo.toml`

Rust dependencies and optimization:

```toml
[profile.dev]
opt-level = 0  # Fast compilation

[profile.dev.package."*"]
opt-level = 2  # Optimize dependencies for speed

[profile.release]
opt-level = 3      # Maximum optimization
lto = "thin"       # Link-time optimization
codegen-units = 1  # Better optimization, slower compile
```

## Next Steps

Now that you have Sentinel running:

1. **Explore the codebase**: Read [Architecture](./architecture.md) to understand the design
2. **Learn components**: Check [Frontend](./frontend/README.md) and [Backend](./backend/README.md) docs
3. **Understand features**: Read [Features Guide](./features/README.md)
4. **Start contributing**: See [Contributing Guide](./contributing.md)

## Helpful Resources

- [Tauri Documentation](https://tauri.app/v2/guides/)
- [React 19 Documentation](https://react.dev/)
- [Zustand Documentation](https://docs.pmnd.rs/zustand/)
- [Anthropic API Documentation](https://docs.anthropic.com/)

## Getting Help

- **Issues**: [GitHub Issues](https://github.com/lilfourn/sentinel/issues)
- **Discussions**: [GitHub Discussions](https://github.com/lilfourn/sentinel/discussions)
- **Discord**: Coming soon!

---

**Ready to contribute?** Check out the [Contributing Guide](./contributing.md)!
