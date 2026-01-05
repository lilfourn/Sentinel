# CLAUDE.md

## Project Overview

**Sentinel** is a Tauri v2 desktop file manager with AI-powered organization. React 19 frontend + Rust backend, featuring agentic AI that explores folders and creates organization plans.

### Core Features
- File browsing (grid/list/column views) with drag-and-drop
- **AI Chat** - Conversational file exploration with Claude (streaming + extended thinking)
- **Agentic Organization** - Autonomous folder analysis and reorganization
- **Virtual File System (VFS)** - Preview changes before applying
- **Write-Ahead Log (WAL)** - Crash recovery and transactional safety
- **Vector Search** - Semantic file search via local embeddings

## Commands

```bash
npm run tauri dev      # Development (Vite + Tauri)
npm run tauri build    # Production build
npm run build          # Type check (tsc && vite build)
cargo check            # Rust checks (from src-tauri/)
cargo test
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Frontend (React 19)                       │
├──────────────────┬──────────────────┬───────────────────────┤
│ ChatPanel        │ Stores (Zustand) │ Views                 │
│ - Streaming      │ - chat-store     │ - FileGridView        │
│ - Tool viz       │ - organize-store │ - FileListView        │
│ - @mentions      │ - vfs-store      │ - FileColumnsView     │
│                  │ - ghost-store    │ - DiffView            │
│ ChangesPanel     │ - navigation     │                       │
│ - Plan preview   │ - selection      │ Dialogs               │
│ - Execution      │ - settings       │ - ConfirmDialog       │
│ - Ghost overlay  │                  │ - ContextMenu         │
└──────────────────┴──────────────────┴───────────────────────┘
                            │ invoke()
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                   Backend (Rust + Tauri)                     │
├──────────────────┬──────────────────┬───────────────────────┤
│ Commands         │ AI Module        │ Infrastructure        │
│ - chat.rs        │ - chat/agent.rs  │ - vfs/ (simulation)   │
│ - ai.rs          │ - v2/agent_loop  │ - wal/ (journaling)   │
│ - filesystem.rs  │ - v2/compression │ - execution/ (DAG)    │
│ - jobs.rs        │ - v2/tools       │ - vector/ (embeddings)│
│ - vfs.rs         │ - rules/         │ - tree/ (compression) │
│ - wal.rs         │ - client.rs      │ - jobs/ (persistence) │
└──────────────────┴──────────────────┴───────────────────────┘
                            │ HTTP
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Anthropic Claude API                      │
│    Haiku (fast) │ Sonnet (planning) │ Opus (reasoning)      │
└─────────────────────────────────────────────────────────────┘
```

## Frontend (src/)

**Stack**: React 19, TypeScript, Vite 7, TailwindCSS v4, Zustand, TanStack Query

### Key Stores
| Store | Purpose |
|-------|---------|
| `chat-store.ts` | Chat messages, streaming, context items, model selection |
| `organize-store.ts` | Organization workflow state machine, plan execution |
| `vfs-store.ts` | Virtual filesystem preview state |
| `ghost-store.ts` | Ghost animations for file operation previews |
| `navigation-store.ts` | Directory navigation, history, view mode |
| `selection-store.ts` | Multi-file selection |

### Chat System (`src/components/ChatPanel/`)
- **ChatInput** - Model selector (Haiku/Sonnet/Opus), @mention support
- **MessageItem** - Extended thinking display, tool visualization (memoized)
- **ThoughtAccordion** - Shows agent tool calls (search, read, inspect)
- **InlineMentionDropdown** - File/folder picker with context strategies
- **MessageList** - Message rendering with error boundary
- **StreamingIndicator** - Visual feedback during AI responses

## Backend (src-tauri/)

### Module Structure
```
src-tauri/src/
├── commands/        # Tauri command handlers (chat, ai, filesystem, vfs, wal, jobs)
├── ai/
│   ├── chat/        # ReAct agent for Q&A (streaming, extended thinking)
│   ├── v2/          # Organize agent (map-reduce, hologram compression)
│   └── rules/       # DSL rule parser and evaluator
├── vfs/             # Virtual filesystem simulation
├── wal/             # Write-ahead log for crash recovery
├── execution/       # DAG-based parallel execution
├── vector/          # Local embeddings via fastembed
├── tree/            # XML tree compression
└── jobs/            # Job persistence
```

### Key Commands
| Category | Commands |
|----------|----------|
| **Chat** | `chat_stream`, `abort_chat`, `list_files_for_mention` |
| **AI Organize** | `generate_organize_plan_hybrid`, `get_rename_suggestion` |
| **VFS** | `vfs_simulate_plan`, `vfs_list_dir`, `vfs_search_content` |
| **WAL** | `wal_check_recovery`, `wal_resume_job`, `wal_execute_journal` |
| **Execution** | `execute_plan_parallel` (DAG-based) |

## AI Integration

### Models
| Model | Use Case |
|-------|----------|
| GPT-5-nano (OpenAI) | File exploration, entity extraction, document classification |
| Claude Haiku 4.5 | Fast analysis, exploration fallback |
| Claude Sonnet 4.5 | Organization planning, rule creation |
| Claude Opus 4.5 | Extended thinking, complex reasoning |

### Chat Agent (ReAct Loop)
- Max 8 iterations with 500ms rate limiting
- Extended thinking (128K token budget)
- Tools: `search_hybrid`, `read_file`, `inspect_pattern`, `list_directory`
- Streaming via Tauri events: `chat:token`, `chat:thinking`, `chat:thought`

### Organize Agent (V2/V4/V5)
Three modes based on folder size:
1. **Full Tree** (<300 files) - Complete compressed tree
2. **Map-Reduce** (300-5000 files) - Rule-based iteration until 95% coverage
3. **Hologram** (pattern-heavy) - Adaptive pattern folding (85-94% token savings)

Features: Prompt caching, model escalation (Haiku→Sonnet), rate limiting

## Event System

| Event | Purpose |
|-------|---------|
| `chat:token` | Streaming response content |
| `chat:thinking` | Extended thinking updates |
| `chat:thought` | Tool execution steps |
| `ai-thought` | Organize agent progress |
| `execution-progress` | Plan execution updates |

## Type Sharing

| Frontend | Backend |
|----------|---------|
| `src/types/file.ts` | `src-tauri/src/models/mod.rs` |
| `src/stores/organize-store.ts` | `src-tauri/src/jobs/mod.rs` |
| `src/types/vfs.ts` | `src-tauri/src/vfs/mod.rs` |

## Development Tips

1. **API Key**: Set in Settings panel before using AI features
2. **Debug AI**: Check terminal for `[AgenticLoop]`, `[AI]`, `[ChatAgent]` logs
3. **Streaming**: Chat uses SSE parsing; organize uses JSON responses
4. **Recovery**: Interrupted jobs show recovery banner on startup
5. **VFS Preview**: Changes simulated in virtual filesystem before execution
