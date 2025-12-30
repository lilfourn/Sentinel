# AGENT.md

Comprehensive documentation for the AI agent system in Sentinel. This document explains how the agentic file organization works, including tool execution, plan generation, and the complete workflow.

## Table of Contents
1. [Architecture Overview](#architecture-overview)
2. [Agent Workflow](#agent-workflow)
3. [Tool System](#tool-system)
4. [Plan Generation](#plan-generation)
5. [Frontend State Machine](#frontend-state-machine)
6. [Job Persistence](#job-persistence)
7. [Security Model](#security-model)
8. [Prompt Engineering](#prompt-engineering)
9. [Error Handling](#error-handling)
10. [Debugging Guide](#debugging-guide)

---

## Architecture Overview

The agent follows a **tool-use agentic pattern** where Claude autonomously explores the filesystem using whitelisted shell commands before generating an organization plan.

```
┌──────────────────────────────────────────────────────────────────────────┐
│                              Frontend                                     │
│  ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────────┐  │
│  │ organize-store  │───▶│  Tauri invoke()  │───▶│   ChangesPanel UI   │  │
│  │  State Machine  │◀───│    + events      │◀───│ (thought streaming) │  │
│  └─────────────────┘    └──────────────────┘    └─────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                              Backend (Rust)                               │
│  ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────────┐  │
│  │  commands/ai.rs │───▶│ AnthropicClient  │───▶│   tool_executor.rs  │  │
│  │                 │    │   (client.rs)    │    │  (shell execution)  │  │
│  └─────────────────┘    └──────────────────┘    └─────────────────────┘  │
│                                    │                       │              │
│                                    ▼                       ▼              │
│                          ┌──────────────────┐    ┌─────────────────────┐  │
│                          │   Anthropic API  │    │  PathValidator      │  │
│                          │   (HTTP Client)  │    │  (security checks)  │  │
│                          └──────────────────┘    └─────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────┘
```

### Key Files

| File | Purpose |
|------|---------|
| `src-tauri/src/ai/client.rs` | Anthropic API client, agentic loop implementation |
| `src-tauri/src/ai/tools.rs` | Tool definitions for Claude |
| `src-tauri/src/ai/tool_executor.rs` | Secure shell command execution |
| `src-tauri/src/ai/prompts.rs` | System prompts and prompt builders |
| `src-tauri/src/commands/ai.rs` | Tauri command handlers |
| `src/stores/organize-store.ts` | Frontend state machine |
| `src/components/ChangesPanel/ChangesPanel.tsx` | Real-time thought display |

---

## Agent Workflow

### Complete Organization Flow

```
User triggers organize
        │
        ▼
┌───────────────────┐
│  Phase 1: Scan    │  Frontend calls suggest_naming_conventions
│  (Claude Haiku)   │  Analyzes folder patterns
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│  User Selection   │  User picks naming convention or skips
│  (UI pause)       │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│  Phase 2: Agent   │  Claude Sonnet explores with tools
│  (tool-use loop)  │  Calls ls, grep, find, cat
│                   │  Max 15 iterations
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│  Phase 3: Plan    │  Agent calls submit_plan tool
│  (submit_plan)    │  Returns OrganizePlan struct
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│  Phase 4: Execute │  Frontend executes operations
│  (sequential)     │  create_folder → move → rename → trash
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│  Complete         │  Job marked complete, state cleared
└───────────────────┘
```

### Agentic Loop Detail (client.rs:412-554)

```rust
pub async fn run_agentic_organize<F>(
    &self,
    target_folder: &str,
    user_request: &str,
    event_emitter: F,
) -> Result<OrganizePlan, String>
```

The agentic loop:

1. **Initialize conversation** with user message containing folder path and request
2. **Loop up to 15 iterations**:
   - Send messages + tools to Claude
   - Parse response for `tool_use` blocks
   - If `submit_plan` is called → parse and return plan
   - Otherwise execute `run_shell_command` tools
   - Add results back to conversation
3. **Handle stop conditions**:
   - `submit_plan` tool called → return parsed plan
   - `stop_reason: end_turn` → try parsing JSON from text
   - Max iterations reached → return error

---

## Tool System

### Available Tools (tools.rs)

The agent has access to two tools:

#### 1. run_shell_command
```json
{
  "name": "run_shell_command",
  "description": "Execute a read-only shell command to explore folder structure",
  "input_schema": {
    "type": "object",
    "properties": {
      "command": {
        "type": "string",
        "description": "The shell command to run. Only ls, grep, find, or cat are allowed."
      },
      "working_directory": {
        "type": "string",
        "description": "Optional: Directory to run the command in. Defaults to target folder."
      }
    },
    "required": ["command"]
  }
}
```

**Allowed commands (whitelist):**
- `ls` - List directory contents
- `grep` - Search file contents
- `find` - Find files by pattern
- `cat` - Read file contents

#### 2. submit_plan
```json
{
  "name": "submit_plan",
  "description": "Submit the final organization plan. Ends the conversation.",
  "input_schema": {
    "type": "object",
    "properties": {
      "description": {
        "type": "string",
        "description": "Brief summary of what this organization plan does"
      },
      "operations": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "type": { "enum": ["create_folder", "move", "rename", "trash"] },
            "path": { "type": "string" },
            "source": { "type": "string" },
            "destination": { "type": "string" },
            "newName": { "type": "string" }
          },
          "required": ["type"]
        }
      }
    },
    "required": ["description", "operations"]
  }
}
```

### Tool Execution (tool_executor.rs)

```rust
pub fn execute_tool(
    tool_name: &str,
    input: &serde_json::Value,
    allowed_base_path: &Path,
) -> Result<String, String>
```

**Security measures:**

1. **Command whitelist**: Only `ls`, `grep`, `find`, `cat` allowed
2. **Path validation**: Working directory must be within target folder
3. **Lexical normalization**: Handles `..` and `.` without filesystem access
4. **Output truncation**: Max 10KB per command result
5. **Uses duct crate**: Safer than raw `std::process::Command`

**Example execution:**
```rust
// Input from Claude
{
  "command": "ls -la",
  "working_directory": "/Users/john/Downloads"
}

// Validation steps:
// 1. Check "ls" is in whitelist ✓
// 2. Validate /Users/john/Downloads is within allowed base ✓
// 3. Execute via duct: cmd("sh", &["-c", "ls -la"]).dir(working_dir)
// 4. Truncate output if > 10KB
// 5. Return result string
```

---

## Plan Generation

### OrganizePlan Structure

**Backend (Rust):**
```rust
pub struct OrganizePlan {
    pub plan_id: String,           // "plan-{timestamp}"
    pub description: String,       // Human-readable summary
    pub operations: Vec<OrganizeOperation>,
    pub target_folder: String,
}

pub struct OrganizeOperation {
    pub op_id: String,             // "op-1", "op-2", etc.
    pub op_type: String,           // "create_folder", "move", "rename", "trash"
    pub source: Option<String>,    // For move/copy
    pub destination: Option<String>, // For move/copy
    pub path: Option<String>,      // For create_folder, rename, trash
    pub new_name: Option<String>,  // For rename
}
```

**Frontend (TypeScript):**
```typescript
interface OrganizePlan {
  planId: string;
  description: string;
  operations: OrganizeOperation[];
  targetFolder: string;
}

interface OrganizeOperation {
  opId: string;
  type: 'create_folder' | 'move' | 'rename' | 'trash' | 'copy';
  source?: string;
  destination?: string;
  path?: string;
  newName?: string;
  riskLevel: 'low' | 'medium' | 'high';  // Added by frontend
}
```

### Plan Parsing

Two parsing paths exist:

1. **From submit_plan tool** (preferred):
```rust
fn parse_plan_from_tool_input(input: &serde_json::Value, target_folder: &str)
```
Directly extracts from tool input - most reliable.

2. **From text response** (fallback):
```rust
fn parse_organize_plan(text: &str, target_folder: &str)
```
Uses `json_parser::extract_json()` with 4-stage fallback:
- Direct JSON parse
- Remove markdown code blocks
- Brace-counting extraction
- Original with brace-counting

### Risk Levels

Frontend adds risk levels for UI display (not from AI):

| Operation | Risk Level | Color |
|-----------|------------|-------|
| create_folder | low | Green |
| copy | low | Green |
| move | medium | Yellow |
| rename | medium | Yellow |
| trash | high | Red |

---

## Frontend State Machine

### organize-store.ts States

```typescript
type ThoughtType =
  | 'scanning'          // Initial folder exploration
  | 'analyzing'         // Pattern analysis
  | 'naming_conventions'// Convention selection pause
  | 'planning'          // AI generating plan
  | 'thinking'          // AI reasoning (from events)
  | 'executing'         // Running operations
  | 'complete'          // Success
  | 'error';            // Failure
```

### State Flow

```
┌────────────────────────────────────────────────────────────────────┐
│  isOpen: false                                                      │
│  targetFolder: null                                                 │
└───────────────────────────────────┬────────────────────────────────┘
                startOrganize()     │
                                    ▼
┌────────────────────────────────────────────────────────────────────┐
│  isOpen: true                                                       │
│  isAnalyzing: true                                                  │
│  currentPhase: 'scanning'                                           │
└───────────────────────────────────┬────────────────────────────────┘
         suggest_naming_conventions │
                                    ▼
┌────────────────────────────────────────────────────────────────────┐
│  awaitingConventionSelection: true                                  │
│  suggestedConventions: [...3 options]                               │
│  currentPhase: 'naming_conventions'                                 │
└────────────────────────────────────────────────────────────────────┘
    │                                                    │
    │ selectConvention()                                 │ skipConventionSelection()
    ▼                                                    ▼
┌────────────────────────────────────────────────────────────────────┐
│  selectedConvention: {...}  OR  conventionSkipped: true             │
│  isAnalyzing: true                                                  │
│  currentPhase: 'planning'                                           │
└───────────────────────────────────┬────────────────────────────────┘
    generate_organize_plan_with_convention
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────┐
│  currentPlan: {...}                                                 │
│  isExecuting: true                                                  │
│  currentPhase: 'executing'                                          │
│  currentOpIndex: 0, 1, 2...                                         │
└───────────────────────────────────┬────────────────────────────────┘
          executeOperation() loop   │
                                    ▼
┌────────────────────────────────────────────────────────────────────┐
│  isExecuting: false                                                 │
│  currentPhase: 'complete'                                           │
│  executedOps: ['op-1', 'op-2', ...]                                │
└────────────────────────────────────────────────────────────────────┘
```

### Event Streaming

Backend emits `ai-thought` events during the agentic loop:

```rust
// In commands/ai.rs
let emit = |thought_type: &str, content: &str| {
    let _ = app_handle.emit("ai-thought", json!({
        "type": thought_type,
        "content": content,
    }));
};
```

Frontend listens and adds to thoughts array:

```typescript
unlisten = await listen<{ type: string; content: string }>('ai-thought', (event) => {
    get().addThought(event.payload.type as ThoughtType, event.payload.content);
});
```

---

## Job Persistence

### Why Persistence?

Organization can take time and involves multiple file operations. If the app crashes mid-execution:
- User shouldn't lose track of what was done
- User should be able to resume from where they left off

### Job Lifecycle

```
start_organize_job() ──────────────────────────────────────────┐
        │                                                       │
        ▼                                                       │
┌───────────────┐                                              │
│ JobStatus:    │                                              │
│   Running     │                                              │
└───────┬───────┘                                              │
        │ set_job_plan()                                       │
        ▼                                                       │
┌───────────────────────────────────┐                          │
│ plan: Some(OrganizePlan)          │                          │
│ total_ops: N                      │                          │
└───────┬───────────────────────────┘                          │
        │ complete_job_operation() × N                         │
        ▼                                                       │
┌───────────────────────────────────┐     ┌─────────────────┐  │
│ completed_ops: [op-1, op-2, ...]  │     │ App crashes     │  │
│ current_op_index: i               │ ───▶│                 │──┘
└───────┬───────────────────────────┘     │ check_interrupted
        │ complete_organize_job()         │ _job() on restart
        ▼                                 └─────────┬───────┘
┌───────────────┐                                   │
│ JobStatus:    │                                   ▼
│   Completed   │                         ┌─────────────────┐
└───────────────┘                         │ JobStatus:      │
                                          │   Interrupted   │
                                          └─────────────────┘
```

### File Location

```
~/.config/sentinel/current_job.json
```

### Job State Structure

```json
{
  "jobId": "job-1703847293847",
  "targetFolder": "/Users/john/Downloads",
  "folderName": "Downloads",
  "startedAt": 1703847293847,
  "status": "running",
  "plan": {
    "planId": "plan-1703847294000",
    "description": "Organize by file type",
    "operations": [...],
    "targetFolder": "/Users/john/Downloads"
  },
  "completedOps": ["op-1", "op-2"],
  "currentOpIndex": 2,
  "lastUpdatedAt": 1703847295000,
  "error": null,
  "totalOps": 10
}
```

### Recovery Flow

On app startup:

```typescript
// In App.tsx or similar
useEffect(() => {
  organizeStore.checkForInterruptedJob();
}, []);
```

If interrupted job found → show `InterruptedJobBanner` with resume option.

---

## Security Model

### Command Whitelist

Only these shell commands can be executed:

```rust
const ALLOWED_COMMANDS: &[&str] = &["ls", "grep", "find", "cat"];
```

Anything else returns an error:
```
Command 'rm' not allowed. Only ["ls", "grep", "find", "cat"] are permitted.
```

### Path Validation

**Lexical normalization** (avoids canonicalize issues on macOS):

```rust
fn normalize_path(path: &Path) -> Result<PathBuf, String> {
    // Resolve . and .. without filesystem access
    // Handles apostrophes and special characters safely
}

fn validate_path_within(path: &Path, base: &Path) -> Result<(), String> {
    let normalized_path = normalize_path(path)?;
    let normalized_base = normalize_path(base)?;

    if !normalized_path.starts_with(&normalized_base) {
        return Err("Path is outside allowed directory");
    }
    // Additional traversal check on relative path
}
```

**Why not canonicalize()?**
macOS has issues with paths containing apostrophes (e.g., "John's Files"). Lexical normalization avoids filesystem syscalls that can fail.

### Output Truncation

```rust
const MAX_OUTPUT_SIZE: usize = 10 * 1024;  // 10KB

fn truncate_output(output: &str, max_len: usize) -> String {
    if output.len() > max_len {
        format!("{}...\n[truncated, {} more bytes]", &output[..max_len], output.len() - max_len)
    } else {
        output.to_string()
    }
}
```

### Protected Paths

The security module prevents operations on system directories:
- `/`, `/System`, `/usr`, `/bin`, `/sbin`
- `/Users` (but not subdirectories)
- `/Library` system folders

---

## Prompt Engineering

### System Prompts

Located in `src-tauri/src/ai/prompts.rs`:

#### AGENTIC_ORGANIZE_SYSTEM_PROMPT

```rust
pub const AGENTIC_ORGANIZE_SYSTEM_PROMPT: &str = r#"You are a file organization assistant...

WORKFLOW:
1. Use run_shell_command (1-3 times max) to explore the folder structure
2. Once you understand the files, call submit_plan with your organization plan

AVAILABLE TOOLS:
- run_shell_command: Run ls, grep, find, or cat to explore files
- submit_plan: Submit your final organization plan (REQUIRED)

EXPLORATION (keep it brief):
- Start with: ls -la <folder>
- Optionally: find <folder> -type f to see all files
- Don't over-explore - 1-3 commands is usually enough

OPERATION TYPES for submit_plan:
- create_folder: { "type": "create_folder", "path": "/absolute/path" }
- move: { "type": "move", "source": "/abs/src", "destination": "/abs/dest" }
- rename: { "type": "rename", "path": "/abs/path", "newName": "new-name.ext" }
- trash: { "type": "trash", "path": "/abs/path" }

RULES:
1. All paths must be absolute
2. Create folders before moving files into them
3. Never touch system directories
4. Be conservative - group by file type or purpose
5. ALWAYS call submit_plan when done
"#;
```

#### NAMING_CONVENTION_SYSTEM_PROMPT

```rust
pub const NAMING_CONVENTION_SYSTEM_PROMPT: &str = r#"You are a file naming pattern analyst...

OUTPUT: Respond with ONLY valid JSON in this exact format:
{
  "totalFilesAnalyzed": <number>,
  "suggestions": [
    {
      "id": "conv-1",
      "name": "Human Readable Name",
      "description": "Brief description of how files would be named",
      "example": "example-filename.pdf",
      "pattern": "Pattern description for AI to follow when renaming",
      "confidence": 0.85,
      "matchingFiles": 12
    }
  ]
}
"#;
```

### Prompt Building

```rust
pub fn build_organize_prompt(
    folder_path: &str,
    ls_output: &str,
    user_request: &str,
    context_analysis: Option<&str>,
) -> String
```

Includes:
- Target folder path
- Truncated directory listing (max 500 lines for large folders)
- Optional context analysis
- User's organization request

### Convention Injection

When user selects a naming convention:

```rust
let full_request = format!(
    "{}\n\nIMPORTANT - NAMING CONVENTION TO APPLY:\n\
     When renaming files, use the '{}' convention.\n\
     Pattern: {}\nExample: {}\n\n\
     Apply this naming style consistently to all file rename operations.",
    user_request, conv.name, conv.pattern, conv.example
);
```

---

## Error Handling

### Backend Errors

| Error Type | Handling |
|------------|----------|
| API key missing | Return descriptive error, frontend shows setup prompt |
| API request failed | Return error with message, frontend shows in UI |
| Tool execution failed | Return as tool_result with `is_error: true` |
| Path validation failed | Prevent execution, return security error |
| JSON parsing failed | Try 4-stage fallback parser |

### Frontend Error States

```typescript
// In organize-store.ts
catch (error) {
    state.addThought('error', 'Organization failed', String(error));

    const jobId = get().currentJobId;
    if (jobId) {
        invoke('fail_organize_job', { jobId, error: String(error) });
    }

    set({
        isAnalyzing: false,
        analysisError: String(error),
    });
}
```

### Operation Failure Recovery

If a single operation fails:
1. Mark operation as failed
2. Stop execution (don't continue with remaining ops)
3. Persist failure to job state
4. Show error in UI with context
5. User can retry or dismiss

---

## Debugging Guide

### Log Prefixes

| Prefix | Location | Purpose |
|--------|----------|---------|
| `[AI]` | client.rs | API calls, response parsing |
| `[AgenticLoop]` | client.rs | Agentic loop iterations |
| `[ToolExecutor]` | tool_executor.rs | Command execution |
| `[JobManager]` | jobs/mod.rs | Job persistence |
| `[Organize]` | organize-store.ts | Frontend state changes |

### Enable Debug Logging

Terminal output during `npm run tauri dev` shows all Rust eprintln! logs.

### Common Issues

#### "Agent finished without submitting a plan"
- Agent hit `stop_reason: end_turn` without calling `submit_plan`
- Check if folder is already organized (should submit empty operations)
- May need prompt adjustment

#### "Path is outside allowed directory"
- Tool tried to access path outside target folder
- Security measure working correctly
- Agent may need clearer path constraints

#### "Command 'X' not allowed"
- Agent tried to run non-whitelisted command
- Only ls, grep, find, cat are permitted
- Prompt should be clear about allowed commands

#### JSON Parsing Failures
- Check `[AI] Response preview:` logs for malformed JSON
- 4-stage parser usually handles most edge cases
- May be markdown blocks or conversational text

### Testing Tools Manually

```bash
# In terminal, simulate tool execution
cd /path/to/target/folder
ls -la
find . -type f -name "*.pdf"
grep -l "invoice" *.txt
```

### Inspecting Job State

```bash
cat ~/.config/sentinel/current_job.json | jq .
```

### API Request Debugging

Add more logging in `client.rs`:

```rust
eprintln!("[API] Request body: {}", serde_json::to_string_pretty(&request)?);
eprintln!("[API] Response: {:?}", response);
```

---

## Future Improvements

### Planned Enhancements

1. **Undo Support**: Track reverse operations for full undo
2. **Batch Mode**: Process multiple folders in sequence
3. **Custom Tools**: Allow user-defined safe commands
4. **Streaming Responses**: Use SSE for real-time plan generation
5. **Dry Run Mode**: Preview changes without execution
6. **File Content Analysis**: Use cat more for intelligent grouping

### Extension Points

- **New Tools**: Add to `get_organize_tools()` in tools.rs
- **New Operations**: Add to operation type enum and execute switch
- **New Prompts**: Add constants in prompts.rs
- **Custom Models**: Modify `ClaudeModel` enum in client.rs
