# Sentinel Performance Optimization Plan

## Overview
This plan addresses 47 validated performance issues identified across frontend, backend, and build configuration. Implementation is organized into 4 phases by priority and dependency.

---

## Phase 1: Build Configuration (Quick Wins)

### 1.1 Vite Configuration Enhancement
**File:** `vite.config.ts`
**Impact:** 972KB → ~400KB bundle (59% reduction)

```typescript
// Add to vite.config.ts
build: {
  target: 'es2020',
  minify: 'esbuild',
  sourcemap: false,
  rollupOptions: {
    output: {
      manualChunks: {
        'react-vendor': ['react', 'react-dom'],
        'ui-vendor': ['lucide-react', 'class-variance-authority', 'clsx', 'tailwind-merge'],
        'state-vendor': ['zustand', '@tanstack/react-query'],
        'tauri-vendor': ['@tauri-apps/api', '@tauri-apps/plugin-fs', '@tauri-apps/plugin-dialog'],
      },
    },
  },
  chunkSizeWarningLimit: 250,
  cssCodeSplit: true,
  cssMinify: true,
},
esbuild: {
  drop: process.env.NODE_ENV === 'production' ? ['console', 'debugger'] : [],
},
```

### 1.2 Cargo Release Profile
**File:** `src-tauri/Cargo.toml`
**Impact:** 15-25% smaller binary, 5-15% faster runtime

```toml
[profile.dev]
incremental = true
opt-level = 0

[profile.dev.package."*"]
opt-level = 2

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = "symbols"
panic = "abort"
```

---

## Phase 2: Frontend Critical Fixes

### 2.1 FileGridItem Memoization
**File:** `src/components/file-list/FileGridView.tsx`
**Lines:** 84-216

```typescript
// BEFORE: function FileGridItem({ ... }) { ... }

// AFTER:
const FileGridItem = memo(function FileGridItem({
  entry,
  isSelected,
  isEditing,
  isDragTarget,
  isValidDropTarget,
  ghostState,
  linkedPath,
  ...handlers
}: FileGridItemProps) {
  // ... component body unchanged
}, (prev, next) => {
  return (
    prev.entry.path === next.entry.path &&
    prev.entry.modified_at === next.entry.modified_at &&
    prev.isSelected === next.isSelected &&
    prev.isEditing === next.isEditing &&
    prev.isDragTarget === next.isDragTarget &&
    prev.isValidDropTarget === next.isValidDropTarget &&
    prev.ghostState === next.ghostState &&
    prev.linkedPath === next.linkedPath
  );
});
```

### 2.2 Store Selector Optimization
**File:** `src/components/file-list/FileGridView.tsx`
**Lines:** 260-276

```typescript
// BEFORE:
const { navigateTo, setQuickLookPath, currentPath, showHidden } = useNavigationStore();

// AFTER:
const navState = useNavigationStore(
  useShallow((s) => ({
    currentPath: s.currentPath,
    showHidden: s.showHidden,
  }))
);
const navigateTo = useNavigationStore((s) => s.navigateTo);
const setQuickLookPath = useNavigationStore((s) => s.setQuickLookPath);

// Similarly for useSelectionStore:
const selState = useSelectionStore(
  useShallow((s) => ({
    selectedPaths: s.selectedPaths,
    focusedPath: s.focusedPath,
    editingPath: s.editingPath,
    creatingType: s.creatingType,
    creatingInPath: s.creatingInPath,
  }))
);
const select = useSelectionStore((s) => s.select);
// ... extract other actions individually
```

### 2.3 Markdown Components Extraction
**File:** `src/components/ChatPanel/MessageItem.tsx`
**Lines:** 256-322

```typescript
// Move OUTSIDE component, at module level:
const markdownComponents = {
  a: ({ href, children }: { href?: string; children: ReactNode }) => {
    const safeHref = sanitizeUrl(href);
    if (!safeHref) return <span className="text-gray-400">{children}</span>;
    return (
      <a href={safeHref} target="_blank" rel="noopener noreferrer"
         className="text-blue-400 hover:underline">
        {children}
      </a>
    );
  },
  pre: ({ children }: { children: ReactNode }) => <CodeBlock>{children}</CodeBlock>,
  code: ({ className, children }: { className?: string; children: ReactNode }) => {
    const match = /language-(\w+)/.exec(className || '');
    const isInline = !className;
    if (isInline) {
      return <code className="px-1 py-0.5 bg-gray-800 rounded text-sm">{children}</code>;
    }
    return <code className={className}>{children}</code>;
  },
  p: ({ children }: { children: ReactNode }) => (
    <p className="mb-2 last:mb-0 leading-relaxed">{children}</p>
  ),
  ul: ({ children }: { children: ReactNode }) => (
    <ul className="list-disc pl-4 mb-2 space-y-1">{children}</ul>
  ),
  ol: ({ children }: { children: ReactNode }) => (
    <ol className="list-decimal pl-4 mb-2 space-y-1">{children}</ol>
  ),
  h1: ({ children }: { children: ReactNode }) => (
    <h1 className="text-xl font-semibold mb-2 mt-4 first:mt-0">{children}</h1>
  ),
  h2: ({ children }: { children: ReactNode }) => (
    <h2 className="text-lg font-semibold mb-2 mt-3 first:mt-0">{children}</h2>
  ),
  h3: ({ children }: { children: ReactNode }) => (
    <h3 className="text-base font-semibold mb-1 mt-2 first:mt-0">{children}</h3>
  ),
};

// In component, use directly:
<Markdown components={markdownComponents}>
  {message.content}
</Markdown>
```

---

## Phase 3: Backend Performance

### 3.1 Token Batching
**File:** `src-tauri/src/ai/chat/agent.rs`
**Near line 471**

```rust
// Add struct for batching
struct TokenBatcher {
    buffer: String,
    last_emit: std::time::Instant,
}

const TOKEN_BATCH_WINDOW_MS: u64 = 16;  // ~60fps
const TOKEN_BATCH_MAX_CHARS: usize = 50;

impl TokenBatcher {
    fn new() -> Self {
        Self {
            buffer: String::with_capacity(256),
            last_emit: std::time::Instant::now(),
        }
    }

    fn add(&mut self, chunk: &str, app: &AppHandle) -> bool {
        self.buffer.push_str(chunk);

        let should_flush = self.buffer.len() >= TOKEN_BATCH_MAX_CHARS
            || self.last_emit.elapsed() > std::time::Duration::from_millis(TOKEN_BATCH_WINDOW_MS);

        if should_flush && !self.buffer.is_empty() {
            emit_logged!(app, "chat:token", json!({ "chunk": &self.buffer }));
            self.buffer.clear();
            self.last_emit = std::time::Instant::now();
            true
        } else {
            false
        }
    }

    fn flush(&mut self, app: &AppHandle) {
        if !self.buffer.is_empty() {
            emit_logged!(app, "chat:token", json!({ "chunk": &self.buffer }));
            self.buffer.clear();
        }
    }
}
```

### 3.2 Regex Caching
**File:** `src-tauri/src/ai/v2/compression.rs`
**Line 135**

```rust
// Add at top of file:
use once_cell::sync::Lazy;

static NUMBER_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"\d+").expect("Invalid number regex")
});

// In generate_hologram function, replace:
// let number_regex = Regex::new(r"\d+").expect("Invalid regex");
// With:
let normalized = NUMBER_REGEX.replace_all(&base_name, "#");
```

### 3.3 Singleton Event Bus
**File:** `src/stores/chat-store.ts`

```typescript
// Add event bus singleton at module level:
class ChatEventBus {
  private initialized = false;
  private activeStreamId: string | null = null;
  private unlisteners: UnlistenFn[] = [];

  async initialize() {
    if (this.initialized) return;

    this.unlisteners.push(
      await listen<{ chunk: string }>('chat:token', (event) => {
        const store = useChatStore.getState();
        if (store.currentStreamId === this.activeStreamId) {
          store._appendContent(store.currentStreamId!, event.payload.chunk);
        }
      })
    );

    // Add other listeners similarly...
    this.initialized = true;
  }

  setActiveStream(id: string | null) {
    this.activeStreamId = id;
  }

  cleanup() {
    this.unlisteners.forEach(fn => fn());
    this.unlisteners = [];
    this.initialized = false;
  }
}

export const chatEventBus = new ChatEventBus();
```

---

## Phase 4: Rust Optimizations

### 4.1 Zero-Copy SSE Parsing
**File:** `src-tauri/src/ai/chat/agent.rs`
**Lines 351-353**

```rust
// BEFORE:
let line = buffer[..newline_pos].trim().to_string();
buffer = buffer[newline_pos + 1..].to_string();

// AFTER - process lines without reallocation:
fn process_complete_lines(buffer: &mut String, mut handler: impl FnMut(&str)) {
    if let Some(last_newline) = buffer.rfind('\n') {
        let complete = &buffer[..last_newline];
        for line in complete.split('\n') {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                handler(trimmed);
            }
        }
        buffer.drain(..=last_newline);
    }
}
```

### 4.2 Batched WAL Updates
**File:** `src-tauri/src/wal/journal.rs`

```rust
// Add batch update method:
pub fn mark_entries_complete_batch(
    &self,
    job_id: &str,
    entry_ids: &[Uuid],
) -> Result<(), WALError> {
    if entry_ids.is_empty() {
        return Ok(());
    }

    let _lock = self.acquire_lock(job_id)?;
    let mut journal = self.load_journal(job_id)?
        .ok_or_else(|| WALError::NotFound(job_id.to_string()))?;

    for entry_id in entry_ids {
        if let Some(entry) = journal.entries.iter_mut()
            .find(|e| e.id == *entry_id)
        {
            entry.status = EntryStatus::Completed;
            entry.completed_at = Some(chrono::Utc::now());
        }
    }

    self.save_journal_internal(&journal)?;
    Ok(())
}
```

### 4.3 Atomic Counters in Executor
**File:** `src-tauri/src/execution/executor.rs`
**Lines 487-569**

```rust
// BEFORE:
let completed = Arc::new(Mutex::new(0usize));
// ...
let mut c = completed.lock().await;
*c += 1;

// AFTER:
use std::sync::atomic::{AtomicUsize, Ordering};

let completed = Arc::new(AtomicUsize::new(0));
// ...
completed.fetch_add(1, Ordering::Relaxed);
```

---

## Expected Results

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Frontend Bundle | 972KB | ~400KB | 59% smaller |
| Release Binary | ~50MB | ~40MB | 20% smaller |
| Events per Chat | 500+ | ~30-50 | 90% fewer |
| Grid Re-renders | All items | Changed only | 95% fewer |
| SSE Allocations | ~1000 | ~50 | 95% fewer |
| WAL File Ops | 100 per plan | ~10 | 90% fewer |

---

## Execution Order

1. **Day 1:** Build config (Vite + Cargo) - Quick wins
2. **Day 2:** FileGridView fixes (memo + selectors)
3. **Day 2:** MessageItem markdown extraction
4. **Day 3:** Token batching in Rust
5. **Day 3:** Regex caching
6. **Day 4:** Event bus singleton
7. **Day 4:** SSE zero-copy parsing
8. **Day 5:** WAL batching + atomic counters
