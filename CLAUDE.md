# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Sentinel** is a Tauri v2 desktop file manager application with AI-powered file organization capabilities. It combines a modern React 19 frontend with a Rust backend, featuring an agentic AI system that can autonomously explore folder structures and create organization plans.

### Key Features
- File browsing with grid, list, and column views
- AI-powered file renaming with content analysis
- Agentic folder organization using Claude's tool-use capabilities
- Naming convention detection and enforcement
- Job persistence for crash recovery
- Real-time AI thought streaming to UI
- Drag-and-drop file operations

## Common Commands

```bash
# Development (starts both Vite dev server and Tauri)
npm run tauri dev

# Build production app
npm run tauri build

# Frontend only (no Tauri shell)
npm run dev

# Type check frontend
npm run build  # runs tsc && vite build

# Rust checks (from src-tauri/)
cargo check
cargo build
cargo test
```

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Frontend (React)                         │
├─────────────────────────────────────────────────────────────────┤
│  Components         │  Stores (Zustand)    │  Hooks             │
│  - ChangesPanel     │  - navigation-store  │  - useDirectory    │
│  - FileGridView     │  - selection-store   │  - useAutoRename   │
│  - SettingsPanel    │  - organize-store    │  - useThumbnail    │
│  - ContextMenu      │  - settings-store    │  - useSyncedSettings│
│                     │  - toast-store       │                    │
└─────────────────────┴──────────────────────┴────────────────────┘
                              │ invoke()
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Backend (Rust + Tauri)                      │
├─────────────────────────────────────────────────────────────────┤
│  Commands              │  AI Module           │  Services        │
│  - commands/ai.rs      │  - client.rs         │  - thumbnails.rs │
│  - commands/filesystem │  - tools.rs          │  - watcher       │
│  - commands/jobs.rs    │  - tool_executor.rs  │                  │
│                        │  - prompts.rs        │  Jobs            │
│  Models                │  - naming.rs         │  - mod.rs        │
│  - FileEntry           │  - json_parser.rs    │  - persistence   │
│  - DirectoryContents   │  - credentials.rs    │                  │
│                        │                      │  Security        │
│                        │                      │  - path_validator│
└────────────────────────┴──────────────────────┴──────────────────┘
                              │ HTTP
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Anthropic Claude API                          │
│         Haiku (fast analysis) │ Sonnet (planning/agentic)       │
└─────────────────────────────────────────────────────────────────┘
```

## Frontend (src/)

### Tech Stack
- **React 19** + **TypeScript** + **Vite 7**
- **TailwindCSS v4** for styling
- **Zustand** for state management
- **TanStack Query** for async data fetching
- **@tauri-apps/api** for IPC with Rust backend

### State Management (src/stores/)
| Store | Purpose |
|-------|---------|
| `navigation-store.ts` | Directory navigation, history, view mode, Quick Look |
| `selection-store.ts` | File/folder multi-selection with drag support |
| `organize-store.ts` | AI organization workflow state machine |
| `settings-store.ts` | User preferences and synced settings |
| `toast-store.ts` | Toast notifications |

### Key Components
- `ChangesPanel.tsx` - Real-time AI thought stream display
- `FileGridView/FileListView/FileColumnsView.tsx` - File browsing views
- `ContextMenu.tsx` - Right-click context menu with AI actions
- `SettingsPanel.tsx` - API key configuration and preferences

## Backend (src-tauri/)

### Module Structure
```
src-tauri/src/
├── commands/           # Tauri command handlers
│   ├── ai.rs          # AI-related commands
│   ├── filesystem.rs  # File operations
│   ├── jobs.rs        # Job persistence commands
│   └── watcher.rs     # Directory watching
├── ai/                 # AI integration module
│   ├── client.rs      # Anthropic API client
│   ├── tools.rs       # Tool definitions for agentic loop
│   ├── tool_executor.rs # Safe tool execution
│   ├── prompts.rs     # System and user prompts
│   ├── naming.rs      # Naming convention types
│   ├── json_parser.rs # Robust JSON extraction
│   └── credentials.rs # API key storage
├── jobs/               # Job persistence
│   └── mod.rs         # OrganizeJob state machine
├── models/             # Shared data structures
├── services/           # Background services
└── security/           # Path validation
```

### Key Tauri Commands

**Filesystem Operations:**
- `read_directory` - Read directory contents with metadata
- `rename_file` - Rename a file/folder
- `delete_to_trash` - Move to trash (reversible)
- `move_file` - Move file to new location
- `copy_file` - Copy file to new location
- `create_directory` - Create new folder

**AI Operations:**
- `set_api_key` - Validate and store API key
- `get_rename_suggestion` - AI-powered file renaming
- `suggest_naming_conventions` - Analyze folder naming patterns
- `generate_organize_plan_agentic` - Full agentic organization
- `generate_organize_plan_with_convention` - Organize with naming style

**Job Persistence:**
- `start_organize_job` - Begin tracking organization job
- `set_job_plan` - Store generated plan
- `complete_job_operation` - Mark operation done
- `check_interrupted_job` - Recovery on app startup

## AI Integration

### Claude Models Used
| Model | Use Case | Max Tokens |
|-------|----------|------------|
| Claude 4.5 Haiku | Fast context analysis, naming conventions | 500-1024 |
| Claude 4.5 Sonnet | Rename suggestions, agentic planning | 100-4096 |

### Agentic Organization Flow
The agent uses tool-use to autonomously explore folders before planning:

1. **Exploration Phase**: Agent runs `ls`, `grep`, `find`, `cat` commands
2. **Analysis Phase**: Understands file structure and patterns
3. **Planning Phase**: Calls `submit_plan` tool with operations
4. **Execution Phase**: Frontend executes operations sequentially

See `AGENT.md` for detailed agent documentation.

### Credentials Storage
- **Primary**: OS keychain via `keyring` crate
- **Fallback**: File-based storage at `~/.config/sentinel/anthropic_key`
- API keys are validated before storage

## Type Sharing

Frontend types in `src/types/file.ts` mirror Rust structs in `src-tauri/src/models/`. When modifying data structures, update both:

| Frontend | Backend |
|----------|---------|
| `src/types/file.ts` | `src-tauri/src/models/mod.rs` |
| `src/stores/organize-store.ts` | `src-tauri/src/jobs/mod.rs` |
| `src/types/naming-convention.ts` | `src-tauri/src/ai/naming.rs` |

## Important Patterns

### Event Emission
The backend emits `ai-thought` events for real-time UI updates:
```rust
app_handle.emit("ai-thought", json!({
    "type": "thinking",
    "content": "Analyzing files..."
}));
```

### Job Persistence
Organization jobs are persisted to `~/.config/sentinel/current_job.json`:
- Allows recovery from crashes mid-organization
- Tracks completed operations for resume capability

### Path Security
All paths are validated through `PathValidator`:
- Prevents directory traversal attacks
- Protects system directories
- Handles macOS special characters safely

## Development Tips

1. **API Key Setup**: Set Anthropic API key in Settings panel before using AI features
2. **Debugging AI**: Check terminal for `[AgenticLoop]` and `[AI]` prefixed logs
3. **Job Recovery**: Interrupted jobs show recovery banner on app startup
4. **Type Changes**: Update both frontend and backend when modifying shared types
