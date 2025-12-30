# AGENT.md

Comprehensive documentation for the AI agent system in Sentinel. This document explains how the agentic file organization works, including tool execution, plan generation, and the complete workflow.

## Table of Contents
1. [Architecture Overview](#architecture-overview)
2. [Agent Workflow](#agent-workflow)
3. [Backend AI Module](#backend-ai-module)
4. [V2 Agentic System](#v2-agentic-system)
5. [Rules Engine](#rules-engine)
6. [Tool System](#tool-system)
7. [Commands Layer](#commands-layer)
8. [Job Persistence](#job-persistence)
9. [Frontend State Machine](#frontend-state-machine)
10. [Security Model](#security-model)
11. [Constants & Configuration](#constants--configuration)
12. [Debugging Guide](#debugging-guide)

---

## Architecture Overview

The agent follows a **tool-use agentic pattern** where Claude autonomously explores the filesystem using a semantic rule-based system before generating an organization plan.

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
│  │  commands/ai.rs │───▶│ V2 Agent Loop    │───▶│   Rules Engine      │  │
│  │                 │    │ (agent_loop.rs)  │    │ (parser/evaluator)  │  │
│  └─────────────────┘    └──────────────────┘    └─────────────────────┘  │
│                                    │                       │              │
│                                    ▼                       ▼              │
│                          ┌──────────────────┐    ┌─────────────────────┐  │
│                          │   Anthropic API  │    │  Shadow VFS         │  │
│                          │   (HTTP Client)  │    │  (virtual fs layer) │  │
│                          └──────────────────┘    └─────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────┘
```

### Key Files

| File | Purpose |
|------|---------|
| `src-tauri/src/ai/v2/agent_loop.rs` | V2 agentic loop with tool execution |
| `src-tauri/src/ai/v2/tools.rs` | V2 semantic tool definitions |
| `src-tauri/src/ai/v2/vfs.rs` | Shadow VFS for virtual operations |
| `src-tauri/src/ai/v2/prompts.rs` | V2 system prompts with DSL docs |
| `src-tauri/src/ai/rules/ast.rs` | Rule DSL abstract syntax tree |
| `src-tauri/src/ai/rules/parser.rs` | Rule DSL lexer and parser |
| `src-tauri/src/ai/rules/evaluator.rs` | Rule evaluation engine |
| `src-tauri/src/ai/client.rs` | Anthropic API client |
| `src-tauri/src/ai/prompts.rs` | Legacy prompts for naming/renaming |
| `src-tauri/src/ai/credentials.rs` | API key storage |
| `src-tauri/src/commands/ai.rs` | Tauri command handlers |
| `src-tauri/src/commands/jobs.rs` | Job persistence commands |
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
│  Phase 2: V2      │  Claude explores with semantic tools
│  Agent Loop       │  query_semantic_index, apply_organization_rules
│                   │  preview_operations, commit_plan
│                   │  Max 10 iterations
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│  Phase 3: Plan    │  Agent calls commit_plan tool
│  (commit_plan)    │  Returns OrganizePlan struct
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│  Phase 4: Execute │  Frontend executes operations
│  (parallel DAG)   │  create_folder → move → rename → trash
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│  Complete         │  Job marked complete, state cleared
└───────────────────┘
```

### V2 Agentic Loop Detail (`v2/agent_loop.rs`)

```rust
pub async fn run_v2_agentic_organize<F>(
    target_folder: &Path,
    user_request: &str,
    event_emitter: F,
) -> Result<OrganizePlan, String>
where F: Fn(&str, &str),  // Emits (event_type, content)
```

**Algorithm Flow:**

1. **Build ShadowVFS** - Recursively scan target folder into virtual filesystem
2. **Generate Compressed Tree** - Create context with intelligent collapsing
3. **Build Initial Context** - Create message with folder structure (30KB limit)
4. **Main Loop (0..MAX_ITERATIONS=10)**:
   - Rate limit delay (2500ms between requests)
   - Replace full tree with compact summary on iteration 1 (saves ~15K tokens)
   - Prune old messages keeping initial + last N (MAX_MESSAGES=7)
   - Model selection: Haiku for iterations < 2, Sonnet for later
   - Send request with tools to Claude
   - Handle 429 rate limits with exponential backoff (5s, 10s, 20s...)
   - Process response content (text and tool uses)
   - Execute tools immediately, collect results
   - Check stop_reason: if "end_turn" and no tool_results, try auto-commit
   - Add assistant message and tool results back to conversation

**Stop Conditions:**
- `commit_plan` tool called → return parsed plan
- `stop_reason: end_turn` with operations → auto-commit
- Max iterations reached → return error
- No operations generated → "folder already organized" error

---

## Backend AI Module

### Module Structure (`src-tauri/src/ai/`)

```
ai/
├── mod.rs           # Public exports
├── client.rs        # Anthropic API client
├── credentials.rs   # API key storage
├── json_parser.rs   # Robust JSON extraction
├── naming.rs        # Naming convention types
├── prompts.rs       # Legacy system prompts
├── tools.rs         # Legacy tool definitions
├── rules/           # Rules engine
│   ├── mod.rs
│   ├── ast.rs       # Abstract syntax tree
│   ├── parser.rs    # Lexer and parser
│   └── evaluator.rs # Rule evaluation
└── v2/              # V2 agentic system
    ├── mod.rs
    ├── agent_loop.rs
    ├── prompts.rs
    ├── tools.rs
    └── vfs.rs
```

### Anthropic Client (`ai/client.rs`)

**Structs:**

```rust
pub enum ClaudeModel {
    Haiku,      // claude-haiku-4-5 (fast, 10x cheaper)
    Sonnet,     // claude-sonnet-4-5 (balanced reasoning)
}

struct MessageContent {
    content_type: String,  // "text"
    text: String,
}

struct Message {
    role: String,          // "user" or "assistant"
    content: Vec<MessageContent>,
}

struct ApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

pub struct AnthropicClient {
    client: Client,  // reqwest HTTP client
}
```

**Methods:**

| Method | Signature | Purpose |
|--------|-----------|---------|
| `new` | `() -> Self` | Create HTTP client |
| `send_message` | `(model, system_prompt, user_message, max_tokens) -> Result<String>` | Basic message send |
| `suggest_rename` | `(filename, extension, size, content_preview) -> Result<String>` | AI file rename suggestion |
| `suggest_naming_conventions` | `(folder_path, file_listing) -> Result<NamingConventionSuggestions>` | Detect folder naming patterns |
| `validate_api_key` | `(api_key: &str) -> Result<bool>` | Validate API key with test call |

**Constants:**
```rust
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
```

### Credentials Manager (`ai/credentials.rs`)

**Storage:**
- Primary: File-based at `~/.config/sentinel/{provider}_key`
- Format: Base64 encoded (minimal obfuscation)

**Methods:**

| Method | Signature | Purpose |
|--------|-----------|---------|
| `store_api_key` | `(provider, api_key) -> Result<()>` | Base64 encode and save |
| `get_api_key` | `(provider) -> Result<String>` | Read and decode |
| `delete_api_key` | `(provider) -> Result<()>` | Delete credential file |
| `has_api_key` | `(provider) -> bool` | Check existence |

### JSON Parser (`ai/json_parser.rs`)

**Main Function:**
```rust
pub fn extract_json<T: DeserializeOwned>(response: &str) -> Result<T, String>
```

**4-Stage Parsing Pipeline:**
1. Direct parse - If response is pure JSON
2. Remove markdown - Strip ```json ... ``` blocks
3. Brace counting - Find outermost {} pair
4. Original + brace counting - Try on unmodified response

**Helper Functions:**
```rust
fn remove_markdown_blocks(text: &str) -> String
fn find_json_object(text: &str) -> Option<&str>  // Brace counting
```

### Naming Conventions (`ai/naming.rs`)

**Structs:**

```rust
pub struct NamingConvention {
    pub id: String,               // e.g., "conv-1"
    pub name: String,             // "Kebab Case Date Prefixed"
    pub description: String,      // How files would be named
    pub example: String,          // Real example from folder
    pub pattern: String,          // Pattern for AI to follow
    pub confidence: f64,          // 0.0-1.0 match score
    pub matching_files: u32,      // Count of files matching
}

pub struct NamingConventionSuggestions {
    pub folder_path: String,
    pub total_files_analyzed: u32,
    pub suggestions: Vec<NamingConvention>,  // Exactly 3
}
```

### Legacy Prompts (`ai/prompts.rs`)

**RENAME_SYSTEM_PROMPT:**
- Instructs Claude to rename files in kebab-case
- 3-6 meaningful words max
- Preserves extension
- Examples: "invoice-apple-oct24.pdf", "screenshot-2024-12-28.png"

**NAMING_CONVENTION_SYSTEM_PROMPT:**
- Analyzes folder naming patterns
- Returns JSON with exactly 3 convention suggestions
- Ordered by confidence (highest first)

**Prompt Building Functions:**

```rust
pub fn build_rename_prompt(
    filename: &str,
    extension: &str,
    size: u64,
    content_preview: Option<&str>,
) -> String

pub fn build_naming_convention_prompt(
    folder_path: &str,
    file_listing: &str,  // Limited to 8000 chars
) -> String
```

---

## V2 Agentic System

### Module Structure (`src-tauri/src/ai/v2/`)

The V2 system replaces shell-based exploration with semantic, rule-based tools.

### Agent Events (`v2/agent_loop.rs`)

```rust
pub enum AgentEvent {
    Indexing(String),      // Scanning files
    Searching(String),     // Using query_semantic_index
    ApplyingRules(String), // Using apply_organization_rules
    Previewing(String),    // Using preview_operations
    Committing(String),    // Using commit_plan
    Thinking(String),      // Model thinking text
    Error(String),         // Error occurred
}
```

### Message Types

```rust
struct ToolApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<ToolMessage>,
    tools: Option<Vec<ToolDefinition>>,
}

struct ToolMessage {
    role: String,
    content: Vec<ToolMessageContent>,
}

enum ToolMessageContent {
    Text { content_type: String, text: String },
    ToolUse { content_type: String, id: String, name: String, input: Value },
    ToolResult { content_type: String, tool_use_id: String, content: String, is_error: bool },
}

enum ContentBlockResponse {
    Text { text: String },
    ToolUse { id: String, name: String, input: Value },
}
```

### V2 Tool Result Handling

```rust
pub enum V2ToolResult {
    Continue(String),           // Continue the loop
    Commit(OrganizePlan),       // Finalize and return
    Error(String),              // Error occurred
}
```

### V2 System Prompt (`v2/prompts.rs`)

The V2 system prompt teaches the agent:

1. **Tools Available** - Explains each of 4 tools and when to use
2. **Rule DSL Syntax** - Complete grammar for rule expressions
3. **Workflow** - Step through: Understand → Plan → Verify → Execute
4. **Best Practices** - Bulk rules, semantic search, preview before commit
5. **Operation Types** - create_folder, move, rename, trash

**Context Building:**

```rust
pub fn build_v2_initial_context(
    target_folder: &str,
    compressed_tree: &str,      // Full tree on first iteration
    user_request: &str,
) -> String
// Returns formatted message with folder path, tree, and request
// Limits tree to 30KB to fit context

pub fn build_v2_summary_context(
    target_folder: &str,
    file_count: usize,
    dir_count: usize,
    user_request: &str,
) -> String
// Returns compact summary replacing full tree
// Saves ~15K tokens per request after iteration 1
```

### Shadow VFS (`v2/vfs.rs`)

The Shadow VFS provides a virtual filesystem layer for planning operations without touching real files.

**PlannedOperation:**

```rust
pub struct PlannedOperation {
    pub op_id: String,                    // Unique ID
    pub op_type: OperationType,           // create_folder, move, rename, trash
    pub source: Option<String>,           // For move/rename
    pub destination: Option<String>,      // For move/create_folder
    pub path: Option<String>,             // For create_folder, trash, rename
    pub new_name: Option<String>,         // For rename
    pub rule_name: Option<String>,        // Which rule generated this
}

pub enum OperationType {
    CreateFolder,
    Move,
    Rename,
    Trash,
}
```

**OrganizationRule:**

```rust
pub struct OrganizationRule {
    pub name: String,                     // Human-readable rule name
    pub condition: String,                // DSL rule expression (if field)
    pub then_move_to: Option<String>,     // Destination folder
    pub then_rename_to: Option<String>,   // New name pattern
    pub priority: Option<i32>,            // Higher = earlier execution
}
```

**ShadowVFS Struct:**

```rust
pub struct ShadowVFS {
    root: PathBuf,
    files: HashMap<String, VirtualFile>,  // All files indexed by path
    operations: Vec<PlannedOperation>,    // Planned operations
    op_counter: usize,                    // For generating op IDs
    vector_index: SimpleVectorIndex,      // For semantic search
}
```

**ShadowVFS Methods:**

| Method | Signature | Purpose |
|--------|-----------|---------|
| `new` | `(root: &Path) -> io::Result<Self>` | Scan folder, build index |
| `root` | `() -> &Path` | Get root path |
| `files` | `() -> Vec<&VirtualFile>` | Get all files (not dirs) |
| `directory_count` | `() -> usize` | Count directories |
| `directories` | `() -> Vec<&VirtualFile>` | Get directory entries |
| `all_entries` | `() -> Vec<&VirtualFile>` | Files and directories |
| `vector_index` | `() -> &SimpleVectorIndex` | Access semantic index |
| `operations` | `() -> &[PlannedOperation]` | Get planned operations |
| `clear_operations` | `()` | Clear all operations |
| `query_semantic` | `(query, filter_ext, min_size, max_results, min_similarity) -> Vec<(VirtualFile, f32)>` | Semantic search |
| `apply_rules` | `(rules, mode) -> Result<usize>` | Apply organization rules |
| `preview_operations` | `(group_by, include_unchanged) -> OperationPreview` | Preview operations |
| `add_operation` | `(op_type, params)` | Manual operation creation |
| `generate_compressed_tree` | `() -> String` | Intelligent tree collapsing |

**apply_rules Algorithm:**

1. Sort rules by priority descending
2. Track processed files to avoid duplicates
3. For each rule:
   - Parse condition with RuleParser
   - Create RuleEvaluator
   - Find all matching files not yet processed
   - For each match:
     - If `thenMoveTo`: Create Move operation, track folder creation
     - If `thenRenameTo`: Create Rename operation
     - Mark file as processed
     - Check operation limit (MAX_OPERATIONS = 5000)
4. Prepend CreateFolder operations for all needed folders
5. Return count of non-folder operations

**Rename Pattern Placeholders:**
- `{name}` - Original filename without extension
- `{ext}` - File extension
- `{date}` - Modified date formatted as YYYY-MM-DD

**OperationPreview:**

```rust
pub struct OperationPreview {
    pub groups: HashMap<String, Vec<PlannedOperation>>,
    pub total_operations: usize,
    pub unchanged_files: usize,
}
```

---

## Rules Engine

### Module Structure (`src-tauri/src/ai/rules/`)

```
rules/
├── mod.rs       # Exports
├── ast.rs       # Abstract syntax tree
├── parser.rs    # Lexer and recursive descent parser
└── evaluator.rs # Rule evaluation against files
```

### AST (`rules/ast.rs`)

**Expression AST:**

```rust
pub enum Expression {
    Or(Box<Expression>, Box<Expression>),      // ||
    And(Box<Expression>, Box<Expression>),     // &&
    Not(Box<Expression>),                      // !
    Comparison(Comparison),                    // field op value
    FunctionCall(FunctionCall),                // func(args)
    Literal(bool),                             // true/false
}

pub struct Comparison {
    pub field: Field,
    pub op: ComparisonOp,
    pub value: Value,
}

pub enum ComparisonOp {
    Eq,       // ==
    Ne,       // !=
    Gt,       // >
    Lt,       // <
    Gte,      // >=
    Lte,      // <=
    In,       // IN
    Matches,  // MATCHES
}

pub enum Field {
    FileName,        // file.name
    FileExt,         // file.ext
    FileSize,        // file.size
    FilePath,        // file.path
    FileModifiedAt,  // file.modifiedAt
    FileCreatedAt,   // file.createdAt
    FileMimeType,    // file.mimeType
    FileIsHidden,    // file.isHidden
}

pub struct FunctionCall {
    pub receiver: String,          // "file", "file.name", etc.
    pub function: FunctionName,
    pub args: Vec<Value>,
}

pub enum FunctionName {
    Contains,
    StartsWith,
    EndsWith,
    Matches,
    VectorSimilarity,
}

pub enum Value {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<Value>),
    SizeBytes(u64),    // Parsed from 10KB, 5MB, 1GB
    Null,
}
```

**Field Parsing:**
- Accepts both camelCase and snake_case
- E.g., "modifiedAt" or "modified_at" both map to FileModifiedAt
- Canonical names: name, ext, size, path, modifiedAt, createdAt, mimeType, isHidden

### Parser (`rules/parser.rs`)

**Token Types:**

```rust
pub enum Token {
    Identifier(String),
    String(String),
    Number(f64),
    SizeBytes(u64),     // 10KB, 5MB, 1GB
    And, Or, Not, In, Matches, True, False,
    Eq, Ne, Gt, Lt, Gte, Lte,
    Dot, Comma, LParen, RParen, LBracket, RBracket,
    Eof,
}

pub struct ParseError {
    pub message: String,
    pub position: usize,
}

pub struct Lexer<'a> {
    input: &'a str,
    chars: Peekable<Chars<'a>>,
    position: usize,
}
```

**Lexer Features:**
- Whitespace skipping
- String parsing with escape sequences (\n, \t, \\, etc.)
- Number parsing (integers and floats)
- Size unit parsing: `10KB` → SizeBytes(10240), `5MB`, `1GB`
- Keyword recognition: AND, OR, NOT, IN, MATCHES, TRUE, FALSE
- Multi-char operator handling: ==, !=, >=, <=

**Parser (Recursive Descent):**

```
parse()       → parse_or()
parse_or()    → parse_and() separated by OR
parse_and()   → parse_not() separated by AND
parse_not()   → NOT prefix or parse_primary()
parse_primary() → Parens, Comparison, FunctionCall, Literals
```

### Evaluator (`rules/evaluator.rs`)

**VirtualFile:**

```rust
pub struct VirtualFile {
    pub name: String,                   // Without extension
    pub ext: Option<String>,            // Lowercase, no dot
    pub size: u64,
    pub path: String,
    pub modified_at: Option<i64>,       // Unix ms
    pub created_at: Option<i64>,        // Unix ms
    pub mime_type: Option<String>,
    pub is_hidden: bool,                // Starts with .
    pub is_directory: bool,
}
```

**VirtualFile Methods:**

| Method | Signature | Purpose |
|--------|-----------|---------|
| `from_path` | `(path: &Path) -> io::Result<Self>` | Read from filesystem |
| `new` | `(name, ext, size, ...)` | Create from data |

**VectorIndex Trait:**

```rust
pub trait VectorIndex: Send + Sync {
    fn similarity(&self, file_path: &str, query: &str) -> Result<f32, RuleError>;
}

pub struct SimpleVectorIndex {
    file_content: HashMap<String, String>,  // path → searchable content
}
```

**SimpleVectorIndex Methods:**

| Method | Signature | Purpose |
|--------|-----------|---------|
| `build_from_files` | `(files: &[VirtualFile]) -> Self` | Create index |
| `similarity` | `(file_path, query) -> Result<f32>` | String overlap scoring (0.0-1.0) |

**RuleEvaluator:**

```rust
pub struct RuleEvaluator<'a> {
    vector_index: &'a dyn VectorIndex,
}

impl<'a> RuleEvaluator<'a> {
    pub fn new(vector_index: &'a dyn VectorIndex) -> Self
    pub fn evaluate(&self, expr: &Expression, file: &VirtualFile) -> Result<bool, RuleError>
}
```

**Evaluation Algorithm:**
- Recursively evaluate expressions
- Boolean operators: AND short-circuits false, OR short-circuits true
- Comparison: Extract field value, apply operator, compare
- Functions: Call string methods or vector_similarity
- Size fields: Convert to bytes for comparison

---

## Tool System

### V2 Tools (`v2/tools.rs`)

The V2 system provides 4 semantic tools:

#### 1. query_semantic_index

Search files by semantic query. Returns ranked matches.

```json
{
  "name": "query_semantic_index",
  "input_schema": {
    "properties": {
      "query": { "type": "string", "description": "Search query" },
      "filter_ext": { "type": "array", "items": { "type": "string" } },
      "max_results": { "type": "integer", "default": 20, "maximum": 30 },
      "min_similarity": { "type": "number", "default": 0.6 }
    },
    "required": ["query"]
  }
}
```

#### 2. apply_organization_rules

Apply DSL rules to generate file operations.

```json
{
  "name": "apply_organization_rules",
  "input_schema": {
    "properties": {
      "rules": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "name": { "type": "string" },
            "if": { "type": "string" },
            "thenMoveTo": { "type": "string" },
            "thenRenameTo": { "type": "string" },
            "priority": { "type": "integer" }
          },
          "required": ["name", "if"]
        }
      },
      "mode": { "type": "string", "enum": ["append", "replace"], "default": "append" }
    },
    "required": ["rules"]
  }
}
```

#### 3. preview_operations

Preview planned operations before committing.

```json
{
  "name": "preview_operations",
  "input_schema": {
    "properties": {
      "group_by": { "type": "string", "enum": ["operation_type", "destination_folder"], "default": "operation_type" },
      "include_unchanged": { "type": "boolean", "default": false }
    }
  }
}
```

#### 4. commit_plan

Finalize plan. Call ONCE when satisfied.

```json
{
  "name": "commit_plan",
  "input_schema": {
    "properties": {
      "description": { "type": "string" },
      "confirm": { "type": "boolean" },
      "dry_run": { "type": "boolean", "default": false }
    },
    "required": ["description", "confirm"]
  }
}
```

### Tool Execution Functions

```rust
pub fn execute_v2_tool(
    name: &str,
    input: &serde_json::Value,
    vfs: &mut ShadowVFS,
) -> V2ToolResult

fn execute_query_semantic(input: &Value, vfs: &ShadowVFS) -> V2ToolResult
fn execute_apply_rules(input: &Value, vfs: &mut ShadowVFS) -> V2ToolResult
fn execute_preview(input: &Value, vfs: &ShadowVFS) -> V2ToolResult
fn execute_commit(input: &Value, vfs: &ShadowVFS) -> V2ToolResult
```

### Rule DSL Syntax

**Fields:**
- `file.name` - Filename without extension
- `file.ext` - Extension (lowercase, no dot)
- `file.size` - Size in bytes
- `file.path` - Full path
- `file.modifiedAt` - Modified timestamp (Unix ms)
- `file.createdAt` - Created timestamp (Unix ms)
- `file.mimeType` - MIME type
- `file.isHidden` - Hidden file flag

**Operators:**
- `==`, `!=`, `>`, `<`, `>=`, `<=`
- `IN` - Value in array
- `MATCHES` - Regex match

**Functions:**
- `contains(substr)` - String contains
- `startsWith(prefix)` - String starts with
- `endsWith(suffix)` - String ends with
- `matches(regex)` - Regex match
- `vector_similarity(query)` - Semantic similarity

**Boolean Operators:**
- `AND`, `OR`, `NOT`
- Parentheses for grouping

**Size Units:**
- `10KB`, `5MB`, `1GB`

**Example Rules:**

```json
{
  "rules": [
    {
      "name": "Move PDFs to Documents",
      "if": "file.ext == \"pdf\"",
      "thenMoveTo": "Documents/PDFs"
    },
    {
      "name": "Large files to Archive",
      "if": "file.size > 100MB",
      "thenMoveTo": "Archive/Large"
    },
    {
      "name": "Screenshots by date",
      "if": "file.name.startsWith(\"Screenshot\") AND file.ext IN [\"png\", \"jpg\"]",
      "thenMoveTo": "Screenshots/{date}",
      "thenRenameTo": "screenshot-{date}.{ext}"
    },
    {
      "name": "Hidden config files",
      "if": "file.isHidden AND file.name.endsWith(\"rc\")",
      "thenMoveTo": "Config"
    }
  ]
}
```

---

## Commands Layer

### AI Commands (`commands/ai.rs`)

**Data Structures:**

```rust
pub struct RenameSuggestion {
    pub original_name: String,
    pub suggested_name: String,
    pub path: String,
}

pub struct ProviderStatus {
    pub provider: String,
    pub configured: bool,
}

pub struct RenameResult {
    pub success: bool,
    pub old_path: String,
    pub new_path: String,
}
```

**Commands:**

| Command | Signature | Purpose |
|---------|-----------|---------|
| `set_api_key` | `(provider, api_key) -> Result<bool>` | Validate and store API key |
| `delete_api_key` | `(provider) -> Result<()>` | Delete credential |
| `get_configured_providers` | `() -> Vec<ProviderStatus>` | List providers with status |
| `get_rename_suggestion` | `(path, filename, extension, size, content_preview) -> Result<RenameSuggestion>` | AI rename suggestion |
| `apply_rename` | `(old_path, new_name) -> Result<RenameResult>` | Execute rename |
| `undo_rename` | `(current_path, original_path) -> Result<()>` | Reverse rename |
| `suggest_naming_conventions` | `(folder_path) -> Result<NamingConventionSuggestions>` | Analyze folder patterns |
| `generate_organize_plan_agentic` | `(folder_path, user_request) -> Result<OrganizePlan>` | Main agentic entry point |
| `generate_organize_plan_with_convention` | `(folder_path, user_request, convention) -> Result<OrganizePlan>` | Organize with naming style |

### Jobs Commands (`commands/jobs.rs`)

**Data Structures:**

```rust
pub struct OrganizeJob {
    pub job_id: String,
    pub target_folder: String,
    pub folder_name: String,
    pub started_at: u64,              // Unix ms
    pub status: JobStatus,
    pub plan: Option<OrganizePlan>,
    pub completed_ops: Vec<String>,
    pub current_op_index: i32,
    pub last_updated_at: u64,
    pub error: Option<String>,
    pub total_ops: usize,
}

pub enum JobStatus {
    Running,
    Completed,
    Failed,
    Interrupted,
}

pub struct OrganizeOperation {
    pub op_id: String,
    pub op_type: String,              // "create_folder", "move", "rename", "trash"
    pub source: Option<String>,
    pub destination: Option<String>,
    pub path: Option<String>,
    pub new_name: Option<String>,
}

pub struct OrganizePlan {
    pub plan_id: String,
    pub description: String,
    pub operations: Vec<OrganizeOperation>,
    pub target_folder: String,
}
```

**Commands:**

| Command | Signature | Purpose |
|---------|-----------|---------|
| `start_organize_job` | `(target_folder) -> Result<OrganizeJob>` | Create new job |
| `set_job_plan` | `(job_id, plan_id, description, operations, target_folder) -> Result<OrganizeJob>` | Store generated plan |
| `complete_job_operation` | `(job_id, op_id, current_index) -> Result<OrganizeJob>` | Mark operation done |
| `complete_organize_job` | `(job_id) -> Result<()>` | Mark job completed |
| `fail_organize_job` | `(job_id, error) -> Result<()>` | Mark job failed |
| `check_interrupted_job` | `() -> Result<Option<OrganizeJob>>` | Recovery on startup |
| `get_current_job` | `() -> Result<Option<OrganizeJob>>` | Load current job |
| `clear_organize_job` | `() -> Result<()>` | Dismiss job |
| `resume_organize_job` | `(job_id) -> Result<OrganizeJob>` | Resume interrupted job |
| `execute_plan_parallel` | `(plan) -> Result<ExecutionResult>` | Parallel execution |

---

## Job Persistence

### Why Persistence?

Organization involves multiple file operations. If the app crashes mid-execution:
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

### JobManager Methods

```rust
pub struct JobManager;

impl JobManager {
    pub fn get_job_path() -> Option<PathBuf>
    // ~/.config/sentinel/current_job.json

    pub fn save_job(job: &OrganizeJob) -> Result<(), String>
    // Serialize to JSON, write to file

    pub fn load_job() -> Result<Option<OrganizeJob>, String>
    // Read and deserialize

    pub fn clear_job() -> Result<(), String>
    // Delete job file

    pub fn check_for_interrupted_job() -> Result<Option<OrganizeJob>, String>
    // Mark Running jobs as Interrupted on startup
}
```

### Recovery Flow

On app startup:

```typescript
// In App.tsx
useEffect(() => {
  organizeStore.checkForInterruptedJob();
}, []);
```

If interrupted job found → show `InterruptedJobBanner` with resume option.

---

## Frontend State Machine

### Type Definitions (`organize-store.ts`)

**Organization Types:**

```typescript
export interface OrganizeOperation {
  opId: string;
  type: 'create_folder' | 'move' | 'rename' | 'trash' | 'copy';
  source?: string;
  destination?: string;
  path?: string;
  newName?: string;
  riskLevel: 'low' | 'medium' | 'high';  // Frontend added
}

export interface OrganizePlan {
  planId: string;
  description: string;
  operations: OrganizeOperation[];
  targetFolder: string;
}

export type ThoughtType =
  | 'scanning' | 'analyzing' | 'naming_conventions'
  | 'planning' | 'thinking' | 'executing'
  | 'complete' | 'error';

export interface AIThought {
  id: string;
  type: ThoughtType;
  content: string;
  timestamp: number;
  detail?: string;
}

export type OrganizePhase =
  | 'idle' | 'indexing' | 'planning' | 'simulation'
  | 'review' | 'committing' | 'rolling_back'
  | 'complete' | 'failed';

export interface InterruptedJobInfo {
  jobId: string;
  folderName: string;
  targetFolder: string;
  completedOps: number;
  totalOps: number;
  startedAt: number;
}
```

### State

```typescript
interface OrganizeState {
  isOpen: boolean;
  targetFolder: string | null;
  currentJobId: string | null;
  thoughts: AIThought[];
  currentPhase: ThoughtType;
  phase: OrganizePhase;
  operationStatuses: Map<string, OperationStatus>;
  wal: WalEntry[];
  rollbackProgress: { completed: number; total: number } | null;
  currentPlan: OrganizePlan | null;
  isAnalyzing: boolean;
  analysisError: string | null;
  isExecuting: boolean;
  executedOps: string[];
  failedOp: string | null;
  currentOpIndex: number;
  hasInterruptedJob: boolean;
  interruptedJob: InterruptedJobInfo | null;
  awaitingConventionSelection: boolean;
  suggestedConventions: NamingConvention[] | null;
  selectedConvention: NamingConvention | null;
  conventionSkipped: boolean;
}
```

### Actions

```typescript
interface OrganizeActions {
  startOrganize(folderPath: string): Promise<void>;
  closeOrganizer(): void;
  addThought(type: ThoughtType, content: string, detail?: string): void;
  setPhase(phase: ThoughtType): void;
  clearThoughts(): void;
  setPlan(plan: OrganizePlan | null): void;
  setAnalyzing(analyzing: boolean): void;
  setAnalysisError(error: string | null): void;
  setExecuting(executing: boolean): void;
  markOpExecuted(opId: string): void;
  markOpFailed(opId: string): void;
  setCurrentOpIndex(index: number): void;
  resetExecution(): void;
  checkForInterruptedJob(): Promise<void>;
  dismissInterruptedJob(): Promise<void>;
  resumeInterruptedJob(): Promise<void>;
  rollbackInterruptedJob(): Promise<void>;
  selectConvention(convention: NamingConvention): void;
  skipConventionSelection(): void;
  transitionTo(phase: OrganizePhase): void;
  acceptPlan(): Promise<void>;
  acceptPlanParallel(): Promise<void>;
  rejectPlan(): void;
  startRollback(): Promise<void>;
  setOperationStatus(opId: string, status: OperationStatus): void;
}
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

### Workflow Orchestration

**startOrganize Phase 1:**
1. Create persistent job with `start_organize_job`
2. Listen to 'ai-thought' events for streaming updates
3. Call `suggest_naming_conventions` to analyze folder patterns
4. Show naming convention selector
5. Wait for user selection or skip

**continueWithConvention Phase 2:**
1. Build enhanced user request with naming convention
2. Call `generate_organize_plan_with_convention`
3. Listen to streaming thoughts
4. Add risk levels to operations (frontend-only)
5. Handle "already organized" case (0 operations)
6. Store plan in persistent job

**Phase 3: Execution**
- Loop through operations sequentially
- For each operation:
  - Call appropriate command (move_file, rename_file, create_directory, etc.)
  - Track progress
  - Persist with `complete_job_operation`
  - Handle failures
- Call `complete_organize_job` on success

**Parallel Execution (acceptPlanParallel):**
- Convert plan to backend format (remove frontend fields)
- Call `execute_plan_parallel`
- Uses DAG-based parallel execution
- Returns ExecutionResult with completion stats

**Recovery/Rollback (startRollback):**
- Iterate through WAL in reverse
- For each completed operation:
  - Reverse the operation (move back, undo rename, etc.)
  - Skip copy and trash (hard to reverse)
- Update rollback progress

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

## Security Model

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

### Protected Paths

The security module prevents operations on system directories:
- `/`, `/System`, `/usr`, `/bin`, `/sbin`
- `/Users` (but not subdirectories)
- `/Library` system folders

### Operation Limits

```rust
const MAX_OPERATIONS: usize = 5000;  // Per plan
const MAX_OUTPUT_SIZE: usize = 4000; // Preview truncation
```

---

## Constants & Configuration

### API Constants

```rust
// In client.rs and agent_loop.rs
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
```

### Model IDs

| Model | ID | Use Case |
|-------|-----|----------|
| Haiku | claude-haiku-4-5 | Fast analysis, early iterations |
| Sonnet | claude-sonnet-4-5 | Complex reasoning, later iterations |

### Rate Limiting

```rust
const MIN_REQUEST_DELAY_MS: u64 = 2500;  // Between requests
const MAX_RETRIES: u32 = 3;               // For 429 errors
// Backoff: 5s, 10s, 20s...
```

### Iteration Limits

```rust
const MAX_ITERATIONS: usize = 10;    // Agent loop iterations
const MAX_MESSAGES: usize = 7;       // Messages to keep in context
const MAX_TOKENS: u32 = 8192;        // Per response
```

### Context Limits

```rust
const MAX_TREE_CONTEXT: usize = 30_000;   // 30KB for tree
const MAX_PREVIEW_SIZE: usize = 4_000;    // 4KB for preview
const MAX_OPERATIONS: usize = 5_000;      // Operations per plan
```

### Model Selection Strategy

```rust
// In agent_loop.rs
let model = if iteration < 2 {
    "claude-haiku-4-5"   // Fast, cheap for exploration
} else {
    "claude-sonnet-4-5"  // Better reasoning for planning
};
```

### File Paths

```
~/.config/sentinel/
├── current_job.json     # Job persistence
└── anthropic_key        # API credential (base64)
```

---

## Debugging Guide

### Log Prefixes

| Prefix | Location | Purpose |
|--------|----------|---------|
| `[AI]` | client.rs | API calls, response parsing |
| `[V2AgenticLoop]` | agent_loop.rs | V2 loop iterations |
| `[V2Tool]` | tools.rs | V2 tool execution |
| `[VFS]` | vfs.rs | Shadow VFS operations |
| `[Rules]` | evaluator.rs | Rule evaluation |
| `[JobManager]` | jobs/mod.rs | Job persistence |
| `[Organize]` | organize-store.ts | Frontend state changes |

### Enable Debug Logging

Terminal output during `npm run tauri dev` shows all Rust `eprintln!` logs.

### Common Issues

#### "Agent finished without submitting a plan"
- Agent hit `stop_reason: end_turn` without calling `commit_plan`
- Check if folder is already organized (should auto-commit empty plan)
- May need to increase MAX_ITERATIONS

#### "Path is outside allowed directory"
- Tool tried to access path outside target folder
- Security measure working correctly
- Check rule conditions for absolute paths

#### "Max operations exceeded"
- Rule matched too many files (>5000)
- Make rule conditions more specific
- Consider splitting into multiple organize sessions

#### JSON Parsing Failures
- Check `[AI] Response preview:` logs for malformed JSON
- 4-stage parser usually handles most edge cases
- May be markdown blocks or conversational text

#### Rate Limiting (429 errors)
- API rate limit hit
- Automatic retry with exponential backoff
- Check `[V2AgenticLoop]` logs for retry messages

### Testing Rules Manually

```bash
# Test rule parsing
echo 'file.ext == "pdf" AND file.size > 1MB' | cargo test rule_parser

# Check semantic index
cargo test simple_vector_index

# Validate rule evaluation
cargo test rule_evaluator
```

### Inspecting Job State

```bash
cat ~/.config/sentinel/current_job.json | jq .
```

### API Request Debugging

Add more logging in `agent_loop.rs`:

```rust
eprintln!("[V2AgenticLoop] Request: {}", serde_json::to_string_pretty(&request)?);
eprintln!("[V2AgenticLoop] Response: {:?}", response);
```

---

## Future Improvements

### Planned Enhancements

1. **Undo Support**: Track reverse operations for full undo
2. **Batch Mode**: Process multiple folders in sequence
3. **Custom Rules**: Allow user-defined rule templates
4. **Streaming Responses**: Use SSE for real-time plan generation
5. **Dry Run Mode**: Preview changes without execution
6. **File Content Analysis**: Use content for intelligent grouping
7. **Better Vector Index**: Replace simple overlap with embeddings

### Extension Points

- **New Tools**: Add to `get_v2_organize_tools()` in v2/tools.rs
- **New Operations**: Add to OperationType enum and execute logic
- **New Fields**: Add to Field enum in rules/ast.rs and evaluator
- **New Functions**: Add to FunctionName enum and evaluation
- **Custom Models**: Modify model selection in agent_loop.rs
