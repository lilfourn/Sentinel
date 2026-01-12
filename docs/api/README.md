# API Reference

Complete reference for Sentinel's Tauri IPC commands, events, and type definitions.

## Table of Contents

- [Commands](#commands)
  - [Chat Commands](#chat-commands)
  - [AI Organization Commands](#ai-organization-commands)
  - [Filesystem Commands](#filesystem-commands)
  - [VFS Commands](#vfs-commands)
  - [WAL Commands](#wal-commands)
  - [Job Commands](#job-commands)
- [Events](#events)
- [Type Definitions](#type-definitions)

## Commands

Commands are invoked from the frontend using Tauri's `invoke` function.

### Chat Commands

#### `chat_stream`

Streams AI chat responses with ReAct tool execution.

**Signature:**
```typescript
function chat_stream(
  message: string,
  context_items: ContextItem[],
  model: string,
  extended_thinking: boolean
): Promise<string>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `message` | `string` | User's chat message |
| `context_items` | `ContextItem[]` | Attached files/folders for context |
| `model` | `string` | Model ID (e.g., "claude-sonnet-4.5") |
| `extended_thinking` | `boolean` | Enable extended thinking mode |

**Returns:** Final response text (also streamed via events)

**Events Emitted:**
- `chat:token` - Streaming response chunks
- `chat:thinking` - Extended thinking updates
- `chat:thought` - Tool execution steps

**Example:**
```typescript
import { invoke, listen } from '@tauri-apps/api';

// Listen for streaming tokens
await listen('chat:token', (event) => {
  console.log('Token:', event.payload.chunk);
});

// Invoke command
const response = await invoke('chat_stream', {
  message: 'What files are in this folder?',
  contextItems: [],
  model: 'claude-sonnet-4.5',
  extendedThinking: false,
});
```

---

#### `abort_chat`

Cancels an active chat stream.

**Signature:**
```typescript
function abort_chat(): Promise<void>
```

**Example:**
```typescript
await invoke('abort_chat');
```

---

#### `list_files_for_mention`

Lists files/folders for @mention autocomplete.

**Signature:**
```typescript
function list_files_for_mention(
  base_path: string,
  query: string
): Promise<MentionItem[]>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `base_path` | `string` | Directory to search in |
| `query` | `string` | Search query (filters by name) |

**Returns:** Array of matching files/folders

**Example:**
```typescript
const items = await invoke('list_files_for_mention', {
  basePath: '/Users/me/Documents',
  query: 'tax',
});
// Returns: [{ path: '/Users/me/Documents/taxes.pdf', name: 'taxes.pdf', isDirectory: false }]
```

### AI Organization Commands

#### `generate_organize_plan_hybrid`

Generates an organization plan using adaptive strategies.

**Signature:**
```typescript
function generate_organize_plan_hybrid(
  path: string,
  instruction: string,
  model?: string
): Promise<OrganizePlan>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `path` | `string` | Directory to organize |
| `instruction` | `string` | Organization instruction (e.g., "organize by type") |
| `model` | `string?` | Optional model override |

**Returns:** Organization plan with operations

**Events Emitted:**
- `ai-thought` - Progress updates during analysis

**Example:**
```typescript
const plan = await invoke('generate_organize_plan_hybrid', {
  path: '/Users/me/Downloads',
  instruction: 'Organize by file type: documents, images, media',
});

console.log(`Plan has ${plan.operations.length} operations`);
```

---

#### `get_rename_suggestion`

Get AI suggestion for renaming a file.

**Signature:**
```typescript
function get_rename_suggestion(
  path: string,
  context?: string
): Promise<string>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `path` | `string` | File path |
| `context` | `string?` | Optional context for better suggestions |

**Returns:** Suggested filename

**Example:**
```typescript
const suggestion = await invoke('get_rename_suggestion', {
  path: '/Users/me/IMG_0001.jpg',
  context: 'Photo from Hawaii vacation 2024',
});
// Returns: "hawaii_vacation_2024_01.jpg"
```

### Filesystem Commands

#### `list_directory`

Lists files in a directory.

**Signature:**
```typescript
function list_directory(path: string): Promise<FileEntry[]>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `path` | `string` | Directory path |

**Returns:** Array of file entries

**Example:**
```typescript
const files = await invoke('list_directory', {
  path: '/Users/me/Documents',
});

files.forEach(file => {
  console.log(`${file.name} - ${file.size} bytes`);
});
```

---

#### `move_files`

Moves files to a destination.

**Signature:**
```typescript
function move_files(
  sources: string[],
  destination: string
): Promise<void>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `sources` | `string[]` | Source file paths |
| `destination` | `string` | Destination directory |

**Example:**
```typescript
await invoke('move_files', {
  sources: ['/Users/me/file1.txt', '/Users/me/file2.txt'],
  destination: '/Users/me/Documents',
});
```

---

#### `delete_files`

Deletes files (moves to trash).

**Signature:**
```typescript
function delete_files(paths: string[]): Promise<void>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `paths` | `string[]` | File paths to delete |

**Example:**
```typescript
await invoke('delete_files', {
  paths: ['/Users/me/old_file.txt'],
});
```

---

#### `create_directory`

Creates a new directory.

**Signature:**
```typescript
function create_directory(path: string): Promise<void>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `path` | `string` | Directory path to create |

**Example:**
```typescript
await invoke('create_directory', {
  path: '/Users/me/Documents/New Folder',
});
```

### VFS Commands

#### `vfs_simulate_plan`

Simulates an organization plan in the virtual filesystem.

**Signature:**
```typescript
function vfs_simulate_plan(plan: OrganizePlan): Promise<VfsSimulationResult>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `plan` | `OrganizePlan` | Organization plan to simulate |

**Returns:** Simulation result with conflicts and predicted state

**Example:**
```typescript
const result = await invoke('vfs_simulate_plan', { plan });

if (result.conflicts.length > 0) {
  console.warn('Conflicts detected:', result.conflicts);
}
```

---

#### `vfs_list_dir`

Lists directory contents in the virtual filesystem.

**Signature:**
```typescript
function vfs_list_dir(path: string): Promise<VfsNode[]>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `path` | `string` | Directory path in VFS |

**Returns:** Array of VFS nodes

---

#### `vfs_search_content`

Searches file contents in the virtual filesystem.

**Signature:**
```typescript
function vfs_search_content(
  query: string,
  path?: string
): Promise<VfsNode[]>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `query` | `string` | Search query |
| `path` | `string?` | Optional path to limit search |

**Returns:** Matching VFS nodes

### WAL Commands

#### `wal_check_recovery`

Checks for incomplete jobs on startup.

**Signature:**
```typescript
function wal_check_recovery(): Promise<RecoveryInfo | null>
```

**Returns:** Recovery info if incomplete job found, null otherwise

**Example:**
```typescript
const recovery = await invoke('wal_check_recovery');

if (recovery) {
  console.log(`Found interrupted job: ${recovery.job_id}`);
  console.log(`${recovery.operations_completed}/${recovery.operations_total} complete`);
}
```

---

#### `wal_resume_job`

Resumes an interrupted job.

**Signature:**
```typescript
function wal_resume_job(job_id: string): Promise<void>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `job_id` | `string` | Job ID to resume |

**Example:**
```typescript
await invoke('wal_resume_job', { jobId: recovery.job_id });
```

---

#### `wal_rollback_job`

Rolls back an interrupted job.

**Signature:**
```typescript
function wal_rollback_job(job_id: string): Promise<void>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `job_id` | `string` | Job ID to rollback |

**Example:**
```typescript
await invoke('wal_rollback_job', { jobId: recovery.job_id });
```

---

#### `wal_execute_journal`

Executes operations with WAL journaling.

**Signature:**
```typescript
function wal_execute_journal(
  job_id: string,
  operations: WalOperation[]
): Promise<void>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `job_id` | `string` | Unique job identifier |
| `operations` | `WalOperation[]` | Operations to execute |

**Events Emitted:**
- `execution-progress` - Progress updates

### Job Commands

#### `create_job`

Creates a persistent job record.

**Signature:**
```typescript
function create_job(
  folder: string,
  instruction: string
): Promise<string>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `folder` | `string` | Target folder |
| `instruction` | `string` | Organization instruction |

**Returns:** Job ID

---

#### `update_job_plan`

Updates job with generated plan.

**Signature:**
```typescript
function update_job_plan(
  job_id: string,
  plan: OrganizePlan
): Promise<void>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `job_id` | `string` | Job ID |
| `plan` | `OrganizePlan` | Organization plan |

---

#### `complete_job`

Marks job as complete.

**Signature:**
```typescript
function complete_job(job_id: string): Promise<void>
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `job_id` | `string` | Job ID |

## Events

Events are emitted from the backend to the frontend.

### Chat Events

#### `chat:token`

Emitted during streaming responses.

**Payload:**
```typescript
{
  chunk: string; // Text chunk
}
```

**Example:**
```typescript
await listen('chat:token', (event) => {
  appendToMessage(event.payload.chunk);
});
```

---

#### `chat:thinking`

Emitted during extended thinking.

**Payload:**
```typescript
{
  content: string; // Thinking content
}
```

---

#### `chat:thought`

Emitted for tool execution steps.

**Payload:**
```typescript
{
  id: string;
  tool: string;
  input: string;
  output?: string;
  status: 'pending' | 'running' | 'complete' | 'error';
  timestamp: number;
}
```

**Example:**
```typescript
await listen('chat:thought', (event) => {
  console.log(`Tool: ${event.payload.tool}`);
  console.log(`Status: ${event.payload.status}`);
});
```

### Organization Events

#### `ai-thought`

Emitted during organization analysis.

**Payload:**
```typescript
{
  type: 'scanning' | 'analyzing' | 'generating_rules' | 'building_plan';
  detail: string;
  progress?: number; // 0-100
}
```

**Example:**
```typescript
await listen('ai-thought', (event) => {
  console.log(`${event.payload.type}: ${event.payload.detail}`);
  if (event.payload.progress) {
    updateProgressBar(event.payload.progress);
  }
});
```

### Execution Events

#### `execution-progress`

Emitted during plan execution.

**Payload:**
```typescript
{
  completed: number;
  total: number;
  current_operation?: string;
  errors: string[];
}
```

**Example:**
```typescript
await listen('execution-progress', (event) => {
  const { completed, total } = event.payload;
  console.log(`Progress: ${completed}/${total}`);
});
```

## Type Definitions

### Core Types

#### `FileEntry`

Represents a file or folder.

```typescript
interface FileEntry {
  path: string;
  name: string;
  size: number;
  modified: number; // Unix timestamp
  is_dir: boolean;
  extension?: string;
  mime_type?: string;
}
```

---

#### `ContextItem`

Attached context for chat messages.

```typescript
interface ContextItem {
  id: string;
  type: 'file' | 'folder' | 'image';
  path: string;
  name: string;
  strategy: 'hologram' | 'read' | 'vision';
  size?: number;
  mimeType?: string;
}
```

---

#### `ChatMessage`

Chat message structure.

```typescript
interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
  thoughts?: ThoughtStep[];
  contextItems?: ContextItem[];
  isStreaming?: boolean;
  thinking?: string;
  isThinking?: boolean;
}
```

---

#### `OrganizePlan`

Organization plan with operations.

```typescript
interface OrganizePlan {
  source_path: string;
  instruction: string;
  operations: OrganizeOperation[];
  created_folders: string[];
  stats: {
    total_files: number;
    files_moved: number;
    folders_created: number;
  };
}
```

---

#### `OrganizeOperation`

Single file operation.

```typescript
interface OrganizeOperation {
  id: string;
  op_type: 'move' | 'copy' | 'rename' | 'delete';
  source: string;
  destination: string;
  reason?: string;
}
```

---

#### `VfsNode`

Virtual filesystem node.

```typescript
interface VfsNode {
  path: string;
  name: string;
  is_dir: boolean;
  size: number;
  modified: number;
  children: string[];
  state: NodeState;
}

type NodeState =
  | 'original'
  | 'created'
  | 'modified'
  | 'deleted'
  | { moved_from: string }
  | { moved_to: string };
```

---

#### `VfsSimulationResult`

VFS simulation output.

```typescript
interface VfsSimulationResult {
  nodes: Record<string, VfsNode>;
  conflicts: Conflict[];
  stats: {
    created: number;
    modified: number;
    deleted: number;
  };
}

type Conflict =
  | { type: 'path_collision'; path: string; sources: string[] }
  | { type: 'circular_dependency'; cycle: string[] }
  | { type: 'missing_parent'; child: string; parent: string };
```

---

#### `RecoveryInfo`

Interrupted job information.

```typescript
interface RecoveryInfo {
  job_id: string;
  started: number; // Unix timestamp
  operations_completed: number;
  operations_total: number;
}
```

---

#### `WalOperation`

Write-ahead log operation.

```typescript
interface WalOperation {
  index: number;
  op_type: 'move' | 'copy' | 'delete' | 'create_dir' | 'rename';
  source: string;
  destination?: string;
  timestamp: number;
}
```

### Model Types

#### `ChatModel`

Available chat models.

```typescript
type ChatModel =
  | 'claude-haiku-4-5'
  | 'claude-sonnet-4-5'
  | 'gpt-5.2-2025-12-11'
  | 'gpt-5-mini-2025-08-07'
  | 'gpt-5-nano-2025-08-07';
```

---

#### `ChatStatus`

Chat status indicator.

```typescript
type ChatStatus = 'idle' | 'thinking' | 'streaming' | 'error';
```

## Error Handling

All commands return `Result<T, String>` and can throw errors. Handle them appropriately:

```typescript
try {
  const result = await invoke('command_name', { params });
} catch (error) {
  console.error('Command failed:', error);
  // Show error to user
}
```

Common error types:
- `"File not found: <path>"` - File doesn't exist
- `"Permission denied: <path>"` - Insufficient permissions
- `"AI API error: <message>"` - API request failed
- `"VFS conflict: <details>"` - Simulation detected conflict
- `"WAL recovery failed: <reason>"` - Recovery error

## Rate Limiting

Some commands have rate limiting:

| Command | Rate Limit |
|---------|-----------|
| `chat_stream` | 500ms between requests |
| `generate_organize_plan_hybrid` | 1 request per minute |
| `get_rename_suggestion` | 10 requests per minute |

## Best Practices

1. **Always listen for events before invoking streaming commands:**
```typescript
// ✅ Correct
await listen('chat:token', handler);
await invoke('chat_stream', params);

// ❌ Wrong (may miss events)
await invoke('chat_stream', params);
await listen('chat:token', handler);
```

2. **Clean up event listeners:**
```typescript
const unlisten = await listen('event', handler);
// ... later
unlisten();
```

3. **Handle errors gracefully:**
```typescript
try {
  await invoke('command', params);
} catch (error) {
  showErrorToast(error);
}
```

4. **Use TypeScript types:**
```typescript
import type { OrganizePlan } from '@/types/plan';

const plan = await invoke<OrganizePlan>('generate_organize_plan_hybrid', params);
```

## See Also

- [Frontend Documentation](../frontend/README.md)
- [Backend Documentation](../backend/README.md)
- [Architecture Overview](../architecture.md)
