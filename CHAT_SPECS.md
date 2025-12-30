# CHAT_SPECS.md

## Feature Specification: Sentinel Omni-Chat (Agentic IDE)

**Objective**: Implement a persistent, context-aware chat interface ("Cursor for Files") that allows users to query their filesystem, drag-and-drop folders for context, and reference specific files using @ mentions.

---

## Table of Contents
1. [Architecture Overview](#architecture-overview)
2. [UX/UI Design](#uxui-design)
3. [Frontend Architecture](#frontend-architecture)
4. [Backend Architecture (Rust)](#backend-architecture-rust)
5. [Context Hydration Strategy](#context-hydration-strategy)
6. [ReAct Agent Loop](#react-agent-loop)
7. [Tool Definitions](#tool-definitions)
8. [Implementation Steps](#implementation-steps)
9. [Security Model](#security-model)
10. [Integration Points](#integration-points)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Frontend (React 19)                                │
├─────────────────────────────────────────────────────────────────────────────┤
│  ChatPanel           │  chat-store.ts      │  Interactions                  │
│  ├─ Header           │  ├─ messages        │  ├─ @ Mentions (cmdk)          │
│  ├─ MessageArea      │  ├─ activeContext   │  ├─ Drag & Drop (HTML5 DnD)    │
│  ├─ ContextStack     │  ├─ model           │  └─ File References            │
│  └─ InputArea        │  └─ isStreaming     │                                │
└─────────────────────┴────────────────────┴────────────────────────────────┘
                                │ invoke() + events
                                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Backend (Rust + Tauri)                             │
├─────────────────────────────────────────────────────────────────────────────┤
│  commands/chat.rs        │  ai/chat/           │  Existing Infrastructure    │
│  ├─ chat_stream          │  ├─ context.rs      │  ├─ compression.rs (V5)     │
│  ├─ abort_chat           │  ├─ agent.rs        │  ├─ local_vector_index.rs   │
│  └─ get_chat_history     │  └─ tools.rs        │  └─ client.rs               │
└──────────────────────────┴────────────────────┴─────────────────────────────┘
                                │ HTTP (streaming)
                                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Anthropic Claude API                                 │
│              Haiku (fast queries) │ Sonnet (reasoning/planning)             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| State Management | Zustand | Consistent with existing stores |
| Mentions | cmdk | Lightweight command palette for @ triggers |
| Drag & Drop | HTML5 DnD API | Already used in `DragDropProvider.tsx` |
| Folder Context | V5 Hologram | Existing `compression.rs` handles 10k+ file folders |
| Semantic Search | LocalVectorIndex | FastEmbed (AllMiniLM-L6-V2) already integrated |
| Agent Pattern | ReAct Loop | Proven pattern from existing `agent_loop.rs` |

---

## UX/UI Design

### A. Chat Panel Layout (`src/components/ChatPanel/`)

```
┌─────────────────────────────────────────┐
│  [☰] Sentinel Chat    [Sonnet ▾] [×]   │  ← Header
├─────────────────────────────────────────┤
│                                         │
│  ┌─────────────────────────────────┐   │
│  │ 🤖 How can I help you organize  │   │
│  │    your files today?            │   │
│  └─────────────────────────────────┘   │
│                                         │
│  ┌─────────────────────────────────┐   │  ← Message Area
│  │ 👤 Find all tax documents from  │   │
│  │    2024 in @Downloads           │   │
│  └─────────────────────────────────┘   │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │ 🤖 > Searching 'tax documents'  │   │  ← Thought Accordion
│  │   Found 23 files matching...    │   │
│  └─────────────────────────────────┘   │
│                                         │
├─────────────────────────────────────────┤
│  [📁 Downloads] [📄 taxes.pdf] [×]     │  ← Context Stack
├─────────────────────────────────────────┤
│  ┌─────────────────────────────────┐   │
│  │ Ask about your files...    [@] │   │  ← Input Area
│  └─────────────────────────────────┘   │
│  [+]                           [Send]   │
└─────────────────────────────────────────┘
```

**Dimensions:**
- Width: 400px (resizable, min 320px, max 600px)
- Position: Collapsible right sidebar
- Animation: Slide in/out with 200ms transition

### B. Header Components

```typescript
interface ChatHeader {
  // Model selector dropdown
  model: 'claude-haiku-4-5' | 'claude-sonnet-4-5';

  // Status indicator
  status: 'idle' | 'thinking' | 'streaming' | 'error';

  // Actions
  onClose: () => void;
  onClear: () => void;
}
```

### C. Message Types

```typescript
type MessageRole = 'user' | 'assistant' | 'system';

interface ChatMessage {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: number;

  // For assistant messages
  thoughts?: ThoughtStep[];  // Collapsible tool usage
  isStreaming?: boolean;

  // For user messages with context
  contextRefs?: ContextRef[];
}

interface ThoughtStep {
  id: string;
  tool: string;           // 'search_hybrid', 'read_file', etc.
  input: string;          // Human-readable description
  output?: string;        // Result summary
  status: 'pending' | 'running' | 'complete' | 'error';
}
```

### D. Context Interactions

#### @ Mentions
- Trigger: Typing `@` in input
- Popover: cmdk-powered command palette
- Content: Files from current directory + recent files
- Fuzzy search enabled
- Selection adds `ContextChip` to stack

#### Drag & Drop
- Source: `FileRow.tsx`, `FileGridView.tsx`, etc.
- Target: Chat panel drop zone
- Visual: Highlight border on drag over
- Action:
  - **File**: Add as `read` context (text content)
  - **Folder**: Add as `hologram` context (V5 compressed summary)
  - **Image**: Add as `vision` context (base64 for multimodal)

---

## Frontend Architecture

### State Management (`src/stores/chat-store.ts`)

```typescript
import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

// ============================================================================
// TYPES
// ============================================================================

export interface ContextItem {
  id: string;
  type: 'file' | 'folder' | 'image';
  path: string;
  name: string;
  /** Context injection strategy */
  strategy: 'hologram' | 'read' | 'vision';
  /** Size in bytes (for display) */
  size?: number;
  /** MIME type for images */
  mimeType?: string;
}

export interface ThoughtStep {
  id: string;
  tool: string;
  input: string;
  output?: string;
  status: 'pending' | 'running' | 'complete' | 'error';
  timestamp: number;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
  thoughts?: ThoughtStep[];
  contextRefs?: string[];  // IDs of ContextItems used
  isStreaming?: boolean;
}

export type ChatModel = 'claude-haiku-4-5' | 'claude-sonnet-4-5';
export type ChatStatus = 'idle' | 'thinking' | 'streaming' | 'error';

// ============================================================================
// STATE INTERFACE
// ============================================================================

interface ChatState {
  // Panel state
  isOpen: boolean;
  width: number;

  // Conversation
  messages: ChatMessage[];
  activeContext: ContextItem[];

  // Model & status
  model: ChatModel;
  status: ChatStatus;
  error: string | null;

  // Streaming
  currentStreamId: string | null;
  abortController: AbortController | null;
}

interface ChatActions {
  // Panel
  open: () => void;
  close: () => void;
  setWidth: (width: number) => void;

  // Context
  addContext: (item: Omit<ContextItem, 'id'>) => void;
  removeContext: (id: string) => void;
  clearContext: () => void;

  // Model
  setModel: (model: ChatModel) => void;

  // Messaging
  sendMessage: (text: string) => Promise<void>;
  abort: () => void;
  clearHistory: () => void;

  // Internal
  _addThought: (messageId: string, thought: ThoughtStep) => void;
  _updateThought: (messageId: string, thoughtId: string, update: Partial<ThoughtStep>) => void;
  _appendContent: (messageId: string, chunk: string) => void;
  _finishStream: (messageId: string) => void;
}

// ============================================================================
// STORE IMPLEMENTATION
// ============================================================================

export const useChatStore = create<ChatState & ChatActions>((set, get) => ({
  // Initial state
  isOpen: false,
  width: 400,
  messages: [],
  activeContext: [],
  model: 'claude-sonnet-4-5',
  status: 'idle',
  error: null,
  currentStreamId: null,
  abortController: null,

  // Panel actions
  open: () => set({ isOpen: true }),
  close: () => set({ isOpen: false }),
  setWidth: (width) => set({ width: Math.max(320, Math.min(600, width)) }),

  // Context actions
  addContext: (item) => {
    const newItem: ContextItem = {
      ...item,
      id: crypto.randomUUID(),
    };
    set((state) => ({
      activeContext: [...state.activeContext.filter(c => c.path !== item.path), newItem],
    }));
  },

  removeContext: (id) => {
    set((state) => ({
      activeContext: state.activeContext.filter((c) => c.id !== id),
    }));
  },

  clearContext: () => set({ activeContext: [] }),

  // Model
  setModel: (model) => set({ model }),

  // Messaging
  sendMessage: async (text) => {
    const { activeContext, model, messages } = get();

    // Create user message
    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: text,
      timestamp: Date.now(),
      contextRefs: activeContext.map(c => c.id),
    };

    // Create placeholder assistant message
    const assistantMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      thoughts: [],
      isStreaming: true,
    };

    set({
      messages: [...messages, userMessage, assistantMessage],
      status: 'thinking',
      error: null,
      currentStreamId: assistantMessage.id,
    });

    // Set up event listeners
    let unlistenToken: UnlistenFn | null = null;
    let unlistenThought: UnlistenFn | null = null;
    let unlistenComplete: UnlistenFn | null = null;
    let unlistenError: UnlistenFn | null = null;

    try {
      // Listen for streaming tokens
      unlistenToken = await listen<{ chunk: string }>('chat:token', (event) => {
        get()._appendContent(assistantMessage.id, event.payload.chunk);
        set({ status: 'streaming' });
      });

      // Listen for thought steps
      unlistenThought = await listen<ThoughtStep>('chat:thought', (event) => {
        get()._addThought(assistantMessage.id, event.payload);
      });

      // Listen for completion
      unlistenComplete = await listen('chat:complete', () => {
        get()._finishStream(assistantMessage.id);
      });

      // Listen for errors
      unlistenError = await listen<{ message: string }>('chat:error', (event) => {
        set({
          status: 'error',
          error: event.payload.message,
        });
        get()._finishStream(assistantMessage.id);
      });

      // Invoke backend command
      await invoke('chat_stream', {
        message: text,
        contextItems: activeContext,
        model,
        conversationHistory: messages.map(m => ({
          role: m.role,
          content: m.content,
        })),
      });

    } catch (err) {
      set({
        status: 'error',
        error: err instanceof Error ? err.message : 'Unknown error',
      });
      get()._finishStream(assistantMessage.id);
    } finally {
      // Cleanup listeners
      unlistenToken?.();
      unlistenThought?.();
      unlistenComplete?.();
      unlistenError?.();
    }
  },

  abort: () => {
    const { currentStreamId } = get();
    if (currentStreamId) {
      invoke('abort_chat').catch(console.error);
      get()._finishStream(currentStreamId);
    }
  },

  clearHistory: () => set({
    messages: [],
    activeContext: [],
    error: null,
  }),

  // Internal actions
  _addThought: (messageId, thought) => {
    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId
          ? { ...m, thoughts: [...(m.thoughts || []), thought] }
          : m
      ),
    }));
  },

  _updateThought: (messageId, thoughtId, update) => {
    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId
          ? {
              ...m,
              thoughts: m.thoughts?.map((t) =>
                t.id === thoughtId ? { ...t, ...update } : t
              ),
            }
          : m
      ),
    }));
  },

  _appendContent: (messageId, chunk) => {
    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId ? { ...m, content: m.content + chunk } : m
      ),
    }));
  },

  _finishStream: (messageId) => {
    set((state) => ({
      messages: state.messages.map((m) =>
        m.id === messageId ? { ...m, isStreaming: false } : m
      ),
      status: 'idle',
      currentStreamId: null,
    }));
  },
}));
```

### Drag & Drop Implementation

#### Source (FileRow.tsx)

```tsx
// Add to existing FileRow component
const handleDragStart = (e: React.DragEvent) => {
  // Set custom MIME types for internal app drag
  e.dataTransfer.setData('sentinel/path', file.path);
  e.dataTransfer.setData('sentinel/type', file.isDirectory ? 'folder' : 'file');
  e.dataTransfer.setData('sentinel/name', file.name);
  e.dataTransfer.setData('sentinel/size', String(file.size));

  // Set drag image (optional)
  e.dataTransfer.effectAllowed = 'link';

  // For images, include MIME type
  if (file.mimeType?.startsWith('image/')) {
    e.dataTransfer.setData('sentinel/mime', file.mimeType);
  }
};

return (
  <div
    draggable="true"
    onDragStart={handleDragStart}
    className={/* existing classes */}
  >
    {/* existing content */}
  </div>
);
```

#### Target (ChatPanel.tsx)

```tsx
const ChatPanel: React.FC = () => {
  const { addContext, isOpen } = useChatStore();
  const [isDragOver, setIsDragOver] = useState(false);

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    if (e.dataTransfer.types.includes('sentinel/path')) {
      e.dataTransfer.dropEffect = 'link';
      setIsDragOver(true);
    }
  };

  const handleDragLeave = () => setIsDragOver(false);

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);

    const path = e.dataTransfer.getData('sentinel/path');
    const type = e.dataTransfer.getData('sentinel/type') as 'file' | 'folder';
    const name = e.dataTransfer.getData('sentinel/name');
    const size = parseInt(e.dataTransfer.getData('sentinel/size') || '0', 10);
    const mimeType = e.dataTransfer.getData('sentinel/mime');

    if (!path) return;

    // Determine context strategy
    let strategy: 'hologram' | 'read' | 'vision' = 'read';
    if (type === 'folder') {
      strategy = 'hologram';
    } else if (mimeType?.startsWith('image/')) {
      strategy = 'vision';
    }

    addContext({
      type,
      path,
      name,
      strategy,
      size,
      mimeType,
    });
  };

  if (!isOpen) return null;

  return (
    <div
      className={`chat-panel ${isDragOver ? 'drag-over' : ''}`}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {/* Panel content */}
    </div>
  );
};
```

### @ Mentions with cmdk

```tsx
// src/components/ChatPanel/MentionPopover.tsx
import { Command } from 'cmdk';
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useNavigationStore } from '@/stores/navigation-store';

interface MentionPopoverProps {
  isOpen: boolean;
  onClose: () => void;
  onSelect: (item: { path: string; name: string; type: 'file' | 'folder' }) => void;
  searchQuery: string;
}

export function MentionPopover({ isOpen, onClose, onSelect, searchQuery }: MentionPopoverProps) {
  const currentPath = useNavigationStore((s) => s.currentPath);
  const [items, setItems] = useState<Array<{ path: string; name: string; isDirectory: boolean }>>([]);

  useEffect(() => {
    if (isOpen && currentPath) {
      // Fetch files from current directory
      invoke<Array<{ path: string; name: string; isDirectory: boolean }>>('list_files_for_mention', {
        directory: currentPath,
        query: searchQuery,
        limit: 20,
      }).then(setItems).catch(console.error);
    }
  }, [isOpen, currentPath, searchQuery]);

  if (!isOpen) return null;

  return (
    <Command.Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <Command.Input placeholder="Search files..." value={searchQuery} />
      <Command.List>
        <Command.Empty>No files found</Command.Empty>
        {items.map((item) => (
          <Command.Item
            key={item.path}
            value={item.name}
            onSelect={() => {
              onSelect({
                path: item.path,
                name: item.name,
                type: item.isDirectory ? 'folder' : 'file',
              });
              onClose();
            }}
          >
            {item.isDirectory ? '📁' : '📄'} {item.name}
          </Command.Item>
        ))}
      </Command.List>
    </Command.Dialog>
  );
}
```

---

## Backend Architecture (Rust)

### Module Structure

```
src-tauri/src/ai/chat/
├── mod.rs           # Public exports
├── context.rs       # Context hydration (files → text, folders → holograms)
├── agent.rs         # ReAct agent loop
├── tools.rs         # Chat-specific tool definitions
└── history.rs       # Conversation history management
```

### Context Hydration (`context.rs`)

This is the **critical integration point** with the V5 Hologram system.

```rust
//! Context hydration module
//!
//! Converts ContextItems into text suitable for LLM system prompts.
//! - Files → Read text content (truncated to 20KB)
//! - Folders → V5 Hologram compression
//! - Images → Base64 for vision

use crate::ai::v2::compression::{generate_hologram, FolderHologram};
use crate::ai::rules::VirtualFile;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Maximum text content size per file (20KB)
const MAX_FILE_CONTENT: usize = 20_000;

/// Maximum context items per request
const MAX_CONTEXT_ITEMS: usize = 10;

/// Context item from frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,  // "file" | "folder" | "image"
    pub path: String,
    pub name: String,
    pub strategy: String,   // "hologram" | "read" | "vision"
    pub size: Option<u64>,
    pub mime_type: Option<String>,
}

/// Hydrated context ready for LLM
pub struct HydratedContext {
    pub system_addition: String,
    pub images: Vec<ImageContext>,
}

pub struct ImageContext {
    pub name: String,
    pub base64: String,
    pub mime_type: String,
}

/// Build the system prompt addition from context items
///
/// # Arguments
/// * `context_items` - Items from frontend (files, folders, images)
///
/// # Returns
/// HydratedContext with text for system prompt and images for multimodal
pub fn hydrate_context(context_items: &[ContextItem]) -> Result<HydratedContext, String> {
    let mut sections: Vec<String> = Vec::new();
    let mut images: Vec<ImageContext> = Vec::new();

    // Limit context items
    let items = if context_items.len() > MAX_CONTEXT_ITEMS {
        eprintln!("[ChatContext] Limiting context from {} to {} items",
                  context_items.len(), MAX_CONTEXT_ITEMS);
        &context_items[..MAX_CONTEXT_ITEMS]
    } else {
        context_items
    };

    for item in items {
        match item.strategy.as_str() {
            "hologram" => {
                // V5 HOLOGRAM - Folder compression
                let hologram = hydrate_folder_hologram(&item.path, &item.name)?;
                sections.push(hologram);
            }
            "read" => {
                // Text file content
                let content = hydrate_file_content(&item.path, &item.name)?;
                sections.push(content);
            }
            "vision" => {
                // Image for multimodal
                if let Some(img) = hydrate_image(&item.path, &item.name, item.mime_type.as_deref())? {
                    images.push(img);
                }
            }
            _ => {
                eprintln!("[ChatContext] Unknown strategy: {}", item.strategy);
            }
        }
    }

    let system_addition = if sections.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n## User-Provided Context\n\n{}",
            sections.join("\n\n---\n\n")
        )
    };

    Ok(HydratedContext {
        system_addition,
        images,
    })
}

/// Generate V5 Hologram for a folder
fn hydrate_folder_hologram(path: &str, name: &str) -> Result<String, String> {
    eprintln!("[ChatContext] Generating hologram for folder: {}", path);

    let folder_path = Path::new(path);
    if !folder_path.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }

    // Scan folder into VirtualFiles
    let mut files: Vec<VirtualFile> = Vec::new();
    scan_folder_recursive(folder_path, &mut files, 3)?; // Max depth 3

    // Generate hologram using V5 compression
    let hologram: FolderHologram = generate_hologram(&files);

    // Format for LLM
    Ok(format!(
        "### Folder: {}\nPath: {}\n\n{}",
        name,
        path,
        hologram.to_prompt_text()
    ))
}

/// Read text content from a file (truncated)
fn hydrate_file_content(path: &str, name: &str) -> Result<String, String> {
    eprintln!("[ChatContext] Reading file: {}", path);

    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path, e))?;

    let truncated = if content.len() > MAX_FILE_CONTENT {
        format!(
            "{}...\n\n[Truncated: {} bytes total]",
            &content[..MAX_FILE_CONTENT],
            content.len()
        )
    } else {
        content
    };

    Ok(format!(
        "### File: {}\nPath: {}\n\n```\n{}\n```",
        name, path, truncated
    ))
}

/// Load image as base64 for vision
fn hydrate_image(path: &str, name: &str, mime_type: Option<&str>) -> Result<Option<ImageContext>, String> {
    eprintln!("[ChatContext] Loading image: {}", path);

    let bytes = fs::read(path)
        .map_err(|e| format!("Failed to read image {}: {}", path, e))?;

    // Skip very large images (> 5MB)
    if bytes.len() > 5 * 1024 * 1024 {
        eprintln!("[ChatContext] Skipping large image: {} bytes", bytes.len());
        return Ok(None);
    }

    let mime = mime_type
        .unwrap_or("image/png")
        .to_string();

    Ok(Some(ImageContext {
        name: name.to_string(),
        base64: base64::encode(&bytes),
        mime_type: mime,
    }))
}

/// Recursively scan folder into VirtualFiles
fn scan_folder_recursive(
    path: &Path,
    files: &mut Vec<VirtualFile>,
    max_depth: usize,
) -> Result<(), String> {
    if max_depth == 0 {
        return Ok(());
    }

    let entries = fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries.flatten() {
        let entry_path = entry.path();
        let metadata = entry.metadata().ok();

        let vfile = VirtualFile::from_path(&entry_path)
            .map_err(|e| format!("Failed to create VirtualFile: {}", e))?;

        files.push(vfile.clone());

        // Recurse into subdirectories
        if vfile.is_directory {
            scan_folder_recursive(&entry_path, files, max_depth - 1)?;
        }
    }

    // Limit total files scanned
    if files.len() > 10_000 {
        eprintln!("[ChatContext] Folder scan limit reached: {} files", files.len());
    }

    Ok(())
}
```

### ReAct Agent Loop (`agent.rs`)

```rust
//! ReAct Agent Loop for Chat
//!
//! Implements the Reason + Act loop pattern:
//! 1. LLM reasons about the query
//! 2. LLM decides to call a tool (or respond)
//! 3. Tool is executed, result fed back
//! 4. Loop until final response

use crate::ai::chat::context::{hydrate_context, ContextItem, HydratedContext};
use crate::ai::chat::tools::{get_chat_tools, execute_chat_tool, ChatToolResult};
use crate::ai::client::AnthropicClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use std::time::Duration;
use tokio::time::sleep;

/// Maximum ReAct loop iterations
const MAX_ITERATIONS: usize = 8;

/// Delay between API requests (rate limiting)
const REQUEST_DELAY_MS: u64 = 1000;

/// Message in conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
}

/// Run the chat agent loop
///
/// # Arguments
/// * `app` - Tauri app handle for emitting events
/// * `message` - User's message
/// * `context_items` - Drag-dropped/mentioned context
/// * `model` - Model ID ("claude-haiku-4-5" or "claude-sonnet-4-5")
/// * `history` - Previous conversation messages
///
/// # Events Emitted
/// * `chat:thought` - Tool usage (ThoughtStep)
/// * `chat:token` - Response chunk
/// * `chat:complete` - Finished
/// * `chat:error` - Error occurred
pub async fn run_chat_agent(
    app: &AppHandle,
    message: &str,
    context_items: &[ContextItem],
    model: &str,
    history: &[ConversationMessage],
) -> Result<String, String> {
    eprintln!("[ChatAgent] Starting with model: {}", model);
    eprintln!("[ChatAgent] Context items: {}", context_items.len());

    // 1. Hydrate context (files → text, folders → holograms)
    let hydrated: HydratedContext = hydrate_context(context_items)?;

    // 2. Build system prompt
    let system_prompt = build_chat_system_prompt(&hydrated.system_addition);

    // 3. Build message history
    let mut messages = build_message_history(history, message, &hydrated)?;

    // 4. Get available tools
    let tools = get_chat_tools();

    // 5. Get API client
    let api_key = crate::ai::credentials::get_api_key("anthropic")?;
    let client = AnthropicClient::new();

    // 6. ReAct Loop
    let mut final_response = String::new();

    for iteration in 0..MAX_ITERATIONS {
        eprintln!("[ChatAgent] Iteration {}/{}", iteration + 1, MAX_ITERATIONS);

        // Rate limiting
        if iteration > 0 {
            sleep(Duration::from_millis(REQUEST_DELAY_MS)).await;
        }

        // Send request to Claude
        let response = client.send_tool_message(
            &api_key,
            model,
            &system_prompt,
            &messages,
            Some(&tools),
            4096,
        ).await?;

        // Process response
        let mut has_tool_use = false;
        let mut tool_results: Vec<Value> = Vec::new();

        for content_block in &response.content {
            match content_block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    let text = content_block.get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");

                    // Emit text chunks
                    app.emit("chat:token", json!({ "chunk": text }))
                        .map_err(|e| format!("Event emit failed: {}", e))?;

                    final_response.push_str(text);
                }
                Some("tool_use") => {
                    has_tool_use = true;

                    let tool_id = content_block.get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("unknown");
                    let tool_name = content_block.get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");
                    let tool_input = content_block.get("input")
                        .cloned()
                        .unwrap_or(json!({}));

                    // Emit thought step
                    app.emit("chat:thought", json!({
                        "id": tool_id,
                        "tool": tool_name,
                        "input": format!("{:?}", tool_input),
                        "status": "running",
                        "timestamp": chrono::Utc::now().timestamp_millis(),
                    })).ok();

                    // Execute tool
                    let result = execute_chat_tool(tool_name, &tool_input).await;

                    // Emit result
                    let (result_content, is_error) = match &result {
                        ChatToolResult::Success(s) => (s.clone(), false),
                        ChatToolResult::Error(e) => (e.clone(), true),
                    };

                    app.emit("chat:thought", json!({
                        "id": tool_id,
                        "tool": tool_name,
                        "output": &result_content[..result_content.len().min(500)],
                        "status": if is_error { "error" } else { "complete" },
                    })).ok();

                    tool_results.push(json!({
                        "type": "tool_result",
                        "tool_use_id": tool_id,
                        "content": result_content,
                        "is_error": is_error,
                    }));
                }
                _ => {}
            }
        }

        // Add assistant message to history
        messages.push(json!({
            "role": "assistant",
            "content": response.content,
        }));

        // If tool was used, add results and continue loop
        if has_tool_use && !tool_results.is_empty() {
            messages.push(json!({
                "role": "user",
                "content": tool_results,
            }));
        }

        // Check stop condition
        if response.stop_reason == Some("end_turn".to_string()) && !has_tool_use {
            eprintln!("[ChatAgent] Completed after {} iterations", iteration + 1);
            break;
        }
    }

    // 7. Emit completion
    app.emit("chat:complete", json!({}))
        .map_err(|e| format!("Event emit failed: {}", e))?;

    Ok(final_response)
}

/// Build the chat system prompt
fn build_chat_system_prompt(context_addition: &str) -> String {
    format!(r#"You are Sentinel Chat, an intelligent assistant for file management and organization.

## Capabilities
- Search files semantically using the `search_hybrid` tool
- Read file contents using the `read_file` tool
- Inspect folder patterns using the `inspect_pattern` tool
- Answer questions about the user's filesystem

## Guidelines
1. Use tools to gather information before answering
2. Be concise and helpful
3. When searching, explain what you're looking for
4. Cite specific files when referencing content
5. You are READ-ONLY - do not suggest making changes without explicit user request

## Security
- You can only access files the user has explicitly shared or that are in their allowed directories
- Never attempt to access system files or sensitive directories
{}
"#, context_addition)
}

/// Build message history for API request
fn build_message_history(
    history: &[ConversationMessage],
    current_message: &str,
    hydrated: &HydratedContext,
) -> Result<Vec<Value>, String> {
    let mut messages: Vec<Value> = Vec::new();

    // Add previous messages (limit to last 20)
    let start = if history.len() > 20 { history.len() - 20 } else { 0 };
    for msg in &history[start..] {
        messages.push(json!({
            "role": msg.role,
            "content": msg.content,
        }));
    }

    // Add current user message
    // If there are images, use multimodal format
    if hydrated.images.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": current_message,
        }));
    } else {
        let mut content: Vec<Value> = Vec::new();

        // Add images first
        for img in &hydrated.images {
            content.push(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": img.mime_type,
                    "data": img.base64,
                }
            }));
        }

        // Add text
        content.push(json!({
            "type": "text",
            "text": current_message,
        }));

        messages.push(json!({
            "role": "user",
            "content": content,
        }));
    }

    Ok(messages)
}
```

---

## Tool Definitions

### Chat Tools (`tools.rs`)

```rust
//! Chat-specific tools for the ReAct agent
//!
//! Tools:
//! - search_hybrid: Semantic + keyword search
//! - read_file: Read file contents
//! - inspect_pattern: Sample files from hologram pattern

use crate::ai::v2::local_vector_index::LocalVectorIndex;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

pub enum ChatToolResult {
    Success(String),
    Error(String),
}

/// Get tool definitions for the chat agent
pub fn get_chat_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "search_hybrid",
            "description": "Search files using semantic understanding and keyword matching. Use when user asks to find files.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language search query (e.g., 'tax documents 2024', 'vacation photos')"
                    },
                    "directory": {
                        "type": "string",
                        "description": "Optional: Limit search to this directory path"
                    },
                    "file_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional: Filter by extensions (e.g., ['pdf', 'docx'])"
                    },
                    "max_results": {
                        "type": "integer",
                        "default": 20,
                        "description": "Maximum results to return"
                    }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "read_file",
            "description": "Read the text content of a file. Use when you need to examine file contents.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the file"
                    },
                    "max_lines": {
                        "type": "integer",
                        "default": 200,
                        "description": "Maximum lines to read"
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "inspect_pattern",
            "description": "Get sample files from a detected hologram pattern. Use to verify pattern contents.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern from hologram (e.g., 'IMG_\\d+\\.jpg')"
                    },
                    "directory": {
                        "type": "string",
                        "description": "Directory containing the pattern"
                    },
                    "sample_count": {
                        "type": "integer",
                        "default": 3,
                        "description": "Number of sample files to return"
                    }
                },
                "required": ["pattern", "directory"]
            }
        }),
        json!({
            "name": "list_directory",
            "description": "List files and folders in a directory. Use for exploring structure.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list"
                    },
                    "max_items": {
                        "type": "integer",
                        "default": 50,
                        "description": "Maximum items to return"
                    }
                },
                "required": ["path"]
            }
        })
    ]
}

/// Execute a chat tool
pub async fn execute_chat_tool(name: &str, input: &Value) -> ChatToolResult {
    eprintln!("[ChatTool] Executing: {} with input: {:?}", name, input);

    match name {
        "search_hybrid" => execute_search_hybrid(input).await,
        "read_file" => execute_read_file(input),
        "inspect_pattern" => execute_inspect_pattern(input),
        "list_directory" => execute_list_directory(input),
        _ => ChatToolResult::Error(format!("Unknown tool: {}", name)),
    }
}

async fn execute_search_hybrid(input: &Value) -> ChatToolResult {
    let query = match input.get("query").and_then(|q| q.as_str()) {
        Some(q) => q,
        None => return ChatToolResult::Error("Missing 'query' parameter".to_string()),
    };

    let max_results = input.get("max_results")
        .and_then(|m| m.as_u64())
        .unwrap_or(20) as usize;

    // Use LocalVectorIndex for semantic search
    // Note: In production, this would use a persistent index
    match crate::ai::v2::local_vector_index::LocalVectorIndex::new_default() {
        Ok(index) => {
            match index.search(query) {
                Ok(results) => {
                    let formatted: Vec<String> = results
                        .iter()
                        .take(max_results)
                        .map(|(path, score)| format!("- {} (score: {:.2})", path.display(), score))
                        .collect();

                    if formatted.is_empty() {
                        ChatToolResult::Success("No files found matching the query.".to_string())
                    } else {
                        ChatToolResult::Success(format!(
                            "Found {} files:\n{}",
                            formatted.len(),
                            formatted.join("\n")
                        ))
                    }
                }
                Err(e) => ChatToolResult::Error(format!("Search failed: {}", e)),
            }
        }
        Err(e) => ChatToolResult::Error(format!("Index initialization failed: {}", e)),
    }
}

fn execute_read_file(input: &Value) -> ChatToolResult {
    let path = match input.get("path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return ChatToolResult::Error("Missing 'path' parameter".to_string()),
    };

    let max_lines = input.get("max_lines")
        .and_then(|m| m.as_u64())
        .unwrap_or(200) as usize;

    // Security: Validate path
    let path_buf = PathBuf::from(path);
    if !path_buf.exists() {
        return ChatToolResult::Error(format!("File not found: {}", path));
    }

    if path_buf.is_dir() {
        return ChatToolResult::Error("Path is a directory, not a file".to_string());
    }

    match fs::read_to_string(&path_buf) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().take(max_lines).collect();
            let truncated = lines.len() < content.lines().count();

            let result = if truncated {
                format!("{}\n\n[Truncated at {} lines]", lines.join("\n"), max_lines)
            } else {
                lines.join("\n")
            };

            ChatToolResult::Success(result)
        }
        Err(e) => ChatToolResult::Error(format!("Failed to read file: {}", e)),
    }
}

fn execute_inspect_pattern(input: &Value) -> ChatToolResult {
    let pattern = match input.get("pattern").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return ChatToolResult::Error("Missing 'pattern' parameter".to_string()),
    };

    let directory = match input.get("directory").and_then(|d| d.as_str()) {
        Some(d) => d,
        None => return ChatToolResult::Error("Missing 'directory' parameter".to_string()),
    };

    let sample_count = input.get("sample_count")
        .and_then(|s| s.as_u64())
        .unwrap_or(3) as usize;

    // Compile regex
    let regex = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return ChatToolResult::Error(format!("Invalid regex: {}", e)),
    };

    // Find matching files
    let dir_path = PathBuf::from(directory);
    if !dir_path.is_dir() {
        return ChatToolResult::Error("Directory not found".to_string());
    }

    let mut matches: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir_path) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if regex.is_match(name) {
                    matches.push(entry.path().display().to_string());
                    if matches.len() >= sample_count {
                        break;
                    }
                }
            }
        }
    }

    if matches.is_empty() {
        ChatToolResult::Success("No files matched the pattern.".to_string())
    } else {
        ChatToolResult::Success(format!(
            "Sample files matching '{}':\n{}",
            pattern,
            matches.join("\n")
        ))
    }
}

fn execute_list_directory(input: &Value) -> ChatToolResult {
    let path = match input.get("path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return ChatToolResult::Error("Missing 'path' parameter".to_string()),
    };

    let max_items = input.get("max_items")
        .and_then(|m| m.as_u64())
        .unwrap_or(50) as usize;

    let dir_path = PathBuf::from(path);
    if !dir_path.is_dir() {
        return ChatToolResult::Error("Path is not a directory".to_string());
    }

    match fs::read_dir(&dir_path) {
        Ok(entries) => {
            let items: Vec<String> = entries
                .flatten()
                .take(max_items)
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = e.path().is_dir();
                    if is_dir {
                        format!("📁 {}/", name)
                    } else {
                        format!("📄 {}", name)
                    }
                })
                .collect();

            ChatToolResult::Success(format!(
                "Contents of {}:\n{}",
                path,
                items.join("\n")
            ))
        }
        Err(e) => ChatToolResult::Error(format!("Failed to list directory: {}", e)),
    }
}
```

### Tauri Command (`commands/chat.rs`)

```rust
//! Chat Tauri commands

use crate::ai::chat::agent::{run_chat_agent, ConversationMessage};
use crate::ai::chat::context::ContextItem;
use tauri::{command, AppHandle, State};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Global abort flag for chat
pub struct ChatAbortFlag(pub Arc<AtomicBool>);

/// Stream a chat response
#[command]
pub async fn chat_stream(
    app: AppHandle,
    message: String,
    context_items: Vec<ContextItem>,
    model: String,
    conversation_history: Vec<ConversationMessage>,
) -> Result<(), String> {
    run_chat_agent(
        &app,
        &message,
        &context_items,
        &model,
        &conversation_history,
    ).await.map(|_| ())
}

/// Abort the current chat stream
#[command]
pub fn abort_chat(abort_flag: State<ChatAbortFlag>) -> Result<(), String> {
    abort_flag.0.store(true, Ordering::SeqCst);
    Ok(())
}

/// List files for @ mention autocomplete
#[command]
pub async fn list_files_for_mention(
    directory: String,
    query: String,
    limit: usize,
) -> Result<Vec<MentionItem>, String> {
    use std::fs;
    use std::path::PathBuf;

    let dir_path = PathBuf::from(&directory);
    if !dir_path.is_dir() {
        return Err("Invalid directory".to_string());
    }

    let query_lower = query.to_lowercase();

    let mut items: Vec<MentionItem> = fs::read_dir(&dir_path)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();

            // Filter by query (fuzzy match)
            if !query_lower.is_empty() && !name.to_lowercase().contains(&query_lower) {
                return None;
            }

            let path = entry.path();
            Some(MentionItem {
                path: path.display().to_string(),
                name,
                is_directory: path.is_dir(),
            })
        })
        .take(limit)
        .collect();

    // Sort directories first, then by name
    items.sort_by(|a, b| {
        match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });

    Ok(items)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MentionItem {
    pub path: String,
    pub name: String,
    pub is_directory: bool,
}
```

---

## Implementation Steps

### Phase 1: Frontend Foundation (Day 1-2)

1. **Install Dependencies**
   ```bash
   npm install cmdk react-markdown
   ```

2. **Create Chat Store** (`src/stores/chat-store.ts`)
   - Implement state management as specified above
   - Add event listener setup for streaming

3. **Create Basic ChatPanel**
   - Header with model selector
   - Message list with markdown rendering
   - Input area with send button
   - Context chip display

4. **Wire Panel Toggle**
   - Add chat button to Toolbar
   - Connect to `useChatStore().open()`

### Phase 2: Drag & Drop (Day 2-3)

1. **Add Drag Source to FileRow**
   - Set custom MIME types
   - Test dragging files and folders

2. **Add Drop Target to ChatPanel**
   - Highlight on drag over
   - Convert drops to ContextItems
   - Distinguish file/folder/image

3. **Display Context Chips**
   - Show name + type icon
   - Remove button
   - Limit to 10 items

### Phase 3: Backend Agent (Day 3-5)

1. **Create `ai/chat/` Module**
   - `mod.rs` - exports
   - `context.rs` - hydration using V5 Hologram
   - `agent.rs` - ReAct loop
   - `tools.rs` - tool definitions

2. **Implement Context Hydration**
   - File reading (truncated)
   - Folder → Hologram conversion
   - Image → base64

3. **Implement ReAct Loop**
   - Build system prompt
   - Process tool_use blocks
   - Execute tools
   - Stream tokens

4. **Add Tauri Commands**
   - `chat_stream`
   - `abort_chat`
   - `list_files_for_mention`

### Phase 4: @ Mentions (Day 5-6)

1. **Create MentionPopover Component**
   - Use cmdk for popover
   - Fetch files on open
   - Fuzzy search

2. **Wire to Input**
   - Detect @ trigger
   - Position popover
   - Insert selected file

3. **Add to Context Stack**
   - Convert mention to ContextItem
   - Strategy based on type

### Phase 5: Polish (Day 6-7)

1. **Thought Accordions**
   - Collapsible tool usage display
   - Status indicators (pending/running/complete)

2. **Streaming UX**
   - Pulsing cursor during stream
   - Smooth scroll to bottom

3. **Error Handling**
   - Display errors in panel
   - Retry mechanism

4. **Testing**
   - Test with large folders (10k+ files)
   - Test drag & drop from all views
   - Test abort mid-stream

---

## Security Model

### Read-Only by Default

The chat agent has **READ-ONLY** access:
- Can search files
- Can read file contents
- Can list directories
- **Cannot** move, rename, delete, or create files

### Path Validation

All paths are validated:
```rust
fn validate_chat_path(path: &str) -> Result<(), String> {
    let path = PathBuf::from(path);

    // Must be absolute
    if !path.is_absolute() {
        return Err("Path must be absolute".to_string());
    }

    // No traversal
    if path.to_string_lossy().contains("..") {
        return Err("Path traversal not allowed".to_string());
    }

    // Not in protected directories
    let protected = ["/System", "/usr", "/bin", "/sbin", "/etc"];
    for p in &protected {
        if path.starts_with(p) {
            return Err("Access to system directories not allowed".to_string());
        }
    }

    Ok(())
}
```

### Context Limits

| Limit | Value | Rationale |
|-------|-------|-----------|
| Max context items | 10 | Prevent token bloat |
| Max file content | 20KB | Keep context manageable |
| Max image size | 5MB | Avoid memory issues |
| Max folder scan depth | 3 | Prevent deep recursion |
| Max folder files | 10,000 | Hologram handles this |

---

## Integration Points

### With Existing Infrastructure

| Component | Integration |
|-----------|-------------|
| `compression.rs` | `hydrate_context()` calls `generate_hologram()` |
| `local_vector_index.rs` | `search_hybrid` tool uses `LocalVectorIndex` |
| `client.rs` | Extended with `send_tool_message()` |
| `credentials.rs` | Reuse API key storage |
| `navigation-store.ts` | Get current directory for mentions |
| `DragDropProvider.tsx` | Extend existing drag system |

### New Files

```
src/
├── stores/
│   └── chat-store.ts           # NEW
├── components/
│   └── ChatPanel/
│       ├── ChatPanel.tsx       # UPDATE (full implementation)
│       ├── ChatHeader.tsx      # NEW
│       ├── MessageList.tsx     # NEW
│       ├── MessageItem.tsx     # NEW
│       ├── ThoughtAccordion.tsx # NEW
│       ├── ContextStack.tsx    # NEW
│       ├── ChatInput.tsx       # NEW
│       └── MentionPopover.tsx  # NEW

src-tauri/src/
├── ai/
│   └── chat/
│       ├── mod.rs              # NEW
│       ├── context.rs          # NEW
│       ├── agent.rs            # NEW
│       ├── tools.rs            # NEW
│       └── history.rs          # NEW
├── commands/
│   └── chat.rs                 # NEW
```

### Event Flow

```
User types message + context
        │
        ▼
Frontend: chat-store.sendMessage()
        │
        ├─ Listen: chat:token, chat:thought, chat:complete, chat:error
        │
        ▼
Backend: chat_stream command
        │
        ├─ hydrate_context() [V5 Hologram for folders]
        │
        ▼
ReAct Loop (agent.rs)
        │
        ├─ Claude API call
        │     │
        │     ├─ text → Emit chat:token
        │     └─ tool_use → Execute tool → Add to history
        │
        └─ Loop until end_turn
                │
                ▼
           Emit chat:complete
```

---

## Future Enhancements

1. **Action Tools (Write Mode)**
   - `move_files` tool (with confirmation)
   - `rename_file` tool
   - `create_folder` tool

2. **Conversation Persistence**
   - Save chat history to disk
   - Resume conversations

3. **Multi-modal Enhancements**
   - PDF preview and search
   - Audio file transcription

4. **Collaborative Context**
   - Share context configurations
   - Team workspaces
