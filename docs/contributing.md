# Contributing Guide

Thank you for considering contributing to Sentinel! This guide will help you get started and understand our development workflow.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Code Style](#code-style)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Project Structure](#project-structure)
- [Common Tasks](#common-tasks)

## Code of Conduct

### Our Pledge

We are committed to providing a welcoming and inclusive environment for everyone, regardless of experience level, background, or identity.

### Expected Behavior

- Be respectful and considerate
- Welcome newcomers and help them get started
- Focus on constructive feedback
- Assume good intentions
- Respect differing viewpoints

### Unacceptable Behavior

- Harassment or discriminatory language
- Personal attacks or trolling
- Spam or off-topic comments
- Publishing others' private information

## Getting Started

### Prerequisites

Before contributing, ensure you have:

1. **Development Environment:**
   - Rust 1.70+ ([rustup.rs](https://rustup.rs/))
   - Node.js 18+ ([nodejs.org](https://nodejs.org/))
   - Git ([git-scm.com](https://git-scm.com/))

2. **Accounts:**
   - GitHub account
   - Anthropic API key (for testing AI features)

3. **Knowledge:**
   - Basic Rust or TypeScript (depending on contribution area)
   - Familiarity with Git

### First Time Setup

```bash
# Fork the repository on GitHub
# Clone your fork
git clone https://github.com/YOUR_USERNAME/sentinel.git
cd sentinel

# Add upstream remote
git remote add upstream https://github.com/lilfourn/sentinel.git

# Install dependencies
npm install

# Run development build
npm run tauri dev
```

### Finding Issues to Work On

Look for issues labeled:
- `good first issue` - Great for newcomers
- `help wanted` - We need assistance
- `bug` - Something isn't working
- `enhancement` - New features or improvements

**Claiming an Issue:**
1. Comment on the issue: "I'd like to work on this"
2. Wait for confirmation from a maintainer
3. Start working!

## Development Workflow

### Branch Strategy

```
main (protected)
├── feature/add-dark-mode
├── fix/crash-on-large-folders
└── docs/update-api-reference
```

**Branch Naming:**
- `feature/` - New features
- `fix/` - Bug fixes
- `docs/` - Documentation updates
- `refactor/` - Code refactoring
- `test/` - Test additions/fixes

### Workflow Steps

1. **Create a branch:**
```bash
git checkout -b feature/your-feature-name
```

2. **Make changes:**
```bash
# Edit files
# Test locally
npm run tauri dev
```

3. **Commit changes:**
```bash
git add .
git commit -m "luke fournier:feat | add dark mode support"
```

4. **Push to your fork:**
```bash
git push origin feature/your-feature-name
```

5. **Open Pull Request:**
- Go to GitHub
- Click "New Pull Request"
- Fill out the template
- Request review

### Keeping Your Fork Updated

```bash
# Fetch upstream changes
git fetch upstream

# Merge into your local main
git checkout main
git merge upstream/main

# Update your fork
git push origin main

# Rebase your feature branch
git checkout feature/your-feature-name
git rebase main
```

## Code Style

### Rust (Backend)

Follow standard Rust conventions:

```rust
// Use descriptive names
pub struct OrganizePlan {
    pub operations: Vec<Operation>,
    pub created_folders: Vec<PathBuf>,
}

// Document public APIs
/// Generates an organization plan for the given folder.
///
/// # Arguments
/// * `path` - Target folder path
/// * `instruction` - Organization instruction
///
/// # Returns
/// `OrganizePlan` with operations to execute
pub async fn generate_plan(
    path: &Path,
    instruction: &str,
) -> Result<OrganizePlan, Error> {
    // Implementation
}

// Use early returns for error handling
pub fn validate_path(path: &Path) -> Result<(), Error> {
    if !path.exists() {
        return Err(Error::FileNotFound(path.to_path_buf()));
    }
    if !path.is_dir() {
        returnErr(Error::NotADirectory(path.to_path_buf()));
    }
    Ok(())
}

// Prefer iterators over loops
let total_size: u64 = files
    .iter()
    .filter(|f| !f.is_dir)
    .map(|f| f.size)
    .sum();
```

**Format Code:**
```bash
cd src-tauri
cargo fmt
cargo clippy
```

### TypeScript (Frontend)

Follow React and TypeScript best practices:

```typescript
// Use functional components with hooks
export function FileList({ files }: { files: FileEntry[] }) {
  const [selected, setSelected] = useState<Set<string>>(new Set());

  // Memoize expensive calculations
  const sortedFiles = useMemo(
    () => files.sort((a, b) => a.name.localeCompare(b.name)),
    [files]
  );

  // Use early returns
  if (files.length === 0) {
    return <EmptyState />;
  }

  return (
    <div className="file-list">
      {sortedFiles.map((file) => (
        <FileItem key={file.path} file={file} />
      ))}
    </div>
  );
}

// Type everything
interface FileItemProps {
  file: FileEntry;
  onSelect?: (path: string) => void;
}

// Use destructuring
const { path, name, size } = file;

// Prefer const over let
const fileName = path.split('/').pop();
```

**Format Code:**
```bash
npm run format  # If formatter is configured
```

### Commit Messages

Follow this format:

```
luke fournier:{type} | {description}

{optional body}

{optional footer}
```

**Types:**
- `feat` - New feature
- `fix` - Bug fix
- `docs` - Documentation changes
- `refactor` - Code refactoring
- `test` - Test additions/fixes
- `perf` - Performance improvements
- `chore` - Maintenance tasks

**Examples:**
```
luke fournier:feat | add dark mode support

Implements dark mode with system preference detection
and manual toggle in settings.

luke fournier:fix | resolve crash on large folders

Fixed memory leak in directory scanner that caused
crashes when scanning folders with >10k files.

luke fournier:docs | update API reference

Added missing parameters for vfs_simulate_plan command.
```

### File Organization

**Frontend:**
```typescript
// Imports: React, third-party, local
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useChatStore } from '@/stores/chat-store';

// Types
interface Props { ... }

// Constants
const MAX_FILES = 100;

// Helper functions
function formatSize(bytes: number): string { ... }

// Main component
export function Component() { ... }
```

**Backend:**
```rust
// Module documentation
//! Module description

// Imports: std, third-party, crate
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use crate::models::FileEntry;

// Constants
const MAX_RETRIES: usize = 3;

// Types
pub struct MyStruct { ... }

// Implementation
impl MyStruct { ... }

// Tests
#[cfg(test)]
mod tests { ... }
```

## Testing

### Frontend Tests

**Location:** `src/` (alongside component files)

**Example:**
```typescript
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { FileList } from './FileList';

describe('FileList', () => {
  it('renders files correctly', () => {
    const files = [
      { path: '/test/file1.txt', name: 'file1.txt', size: 100 },
    ];

    render(<FileList files={files} />);

    expect(screen.getByText('file1.txt')).toBeInTheDocument();
  });

  it('handles selection', () => {
    const onSelect = vi.fn();
    const files = [{ path: '/test/file.txt', name: 'file.txt', size: 100 }];

    render(<FileList files={files} onSelect={onSelect} />);

    fireEvent.click(screen.getByText('file.txt'));

    expect(onSelect).toHaveBeenCalledWith('/test/file.txt');
  });
});
```

**Run tests:**
```bash
npm test
npm run test:coverage
```

### Backend Tests

**Location:** `src-tauri/src/` (in module or separate test file)

**Example:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_vfs_simulation() {
        let temp = TempDir::new().unwrap();
        let mut vfs = Vfs::new();

        // Setup
        let file = temp.path().join("test.txt");
        std::fs::write(&file, "content").unwrap();

        // Load into VFS
        vfs.load_from_path(temp.path()).await.unwrap();

        // Simulate move
        let new_path = temp.path().join("moved.txt");
        vfs.simulate_move(&file, &new_path).unwrap();

        // Verify
        assert!(vfs.get_node(&new_path).is_some());
        assert_eq!(
            vfs.get_node(&new_path).unwrap().state,
            NodeState::MovedFrom(file.clone())
        );
    }
}
```

**Run tests:**
```bash
cd src-tauri
cargo test
cargo test -- --nocapture  # Show output
cargo test test_vfs_simulation  # Run specific test
```

### Testing Checklist

Before submitting a PR, ensure:

- [ ] All existing tests pass
- [ ] New features have tests
- [ ] Bug fixes include regression tests
- [ ] Edge cases are covered
- [ ] Manual testing completed

## Pull Request Process

### Before Opening a PR

1. **Self-review your code:**
   - Check for console.logs, debug prints
   - Remove commented code
   - Verify formatting

2. **Update documentation:**
   - Add/update code comments
   - Update relevant docs in `/docs`
   - Update CHANGELOG.md (if applicable)

3. **Test thoroughly:**
   - Run all tests
   - Test manually in dev mode
   - Test edge cases

### PR Template

Fill out the template completely:

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
How did you test these changes?

## Screenshots (if applicable)
Before/after screenshots for UI changes

## Checklist
- [ ] Code follows style guidelines
- [ ] Self-review completed
- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] All tests pass
```

### Review Process

1. **Automated checks:**
   - CI/CD runs tests
   - Linting checks
   - Build verification

2. **Code review:**
   - Maintainer reviews code
   - Feedback provided
   - Discussion if needed

3. **Address feedback:**
   - Make requested changes
   - Push new commits
   - Re-request review

4. **Merge:**
   - Approved PRs are merged
   - Branch deleted automatically

### PR Best Practices

**DO:**
- Keep PRs focused and small
- Write descriptive PR descriptions
- Respond to feedback promptly
- Test thoroughly

**DON'T:**
- Mix multiple features in one PR
- Leave feedback unaddressed
- Force push after review starts
- Submit WIP as ready for review

## Project Structure

Understanding the codebase:

### Frontend Structure

```
src/
├── components/
│   ├── ChatPanel/          # AI chat UI
│   ├── ChangesPanel/       # Organization workflow
│   └── file-list/          # File browser
│
├── stores/
│   ├── chat-store.ts       # Chat state
│   ├── organize-store.ts   # Organization state
│   └── navigation-store.ts # Navigation state
│
├── hooks/
│   ├── useFileSystem.ts    # File operations
│   └── useKeyboard.ts      # Keyboard shortcuts
│
└── types/
    ├── file.ts             # File types
    └── vfs.ts              # VFS types
```

### Backend Structure

```
src-tauri/src/
├── commands/               # IPC entry points
│   ├── chat.rs
│   ├── ai.rs
│   └── filesystem.rs
│
├── ai/                     # AI integration
│   ├── chat/              # Chat agent
│   └── v2/                # Organization agent
│
├── vfs/                    # Virtual filesystem
├── wal/                    # Write-ahead log
└── execution/              # Execution engine
```

## Common Tasks

### Adding a New Command

1. **Define command handler** (`src-tauri/src/commands/`):
```rust
#[tauri::command]
pub async fn my_new_command(param: String) -> Result<String, String> {
    // Implementation
    Ok("result".to_string())
}
```

2. **Register command** (`src-tauri/src/lib.rs`):
```rust
tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![
        my_new_command,
        // ... other commands
    ])
```

3. **Use from frontend:**
```typescript
import { invoke } from '@tauri-apps/api/core';

const result = await invoke('my_new_command', { param: 'value' });
```

### Adding a New Store

1. **Create store** (`src/stores/my-store.ts`):
```typescript
import { create } from 'zustand';

interface MyState {
  value: string;
  setValue: (value: string) => void;
}

export const useMyStore = create<MyState>((set) => ({
  value: '',
  setValue: (value) => set({ value }),
}));
```

2. **Use in component:**
```typescript
import { useMyStore } from '@/stores/my-store';

function MyComponent() {
  const { value, setValue } = useMyStore();
  // ...
}
```

### Adding a New Component

1. **Create component** (`src/components/MyComponent.tsx`):
```typescript
interface MyComponentProps {
  title: string;
}

export function MyComponent({ title }: MyComponentProps) {
  return <div>{title}</div>;
}
```

2. **Add tests** (`src/components/MyComponent.test.tsx`):
```typescript
import { render, screen } from '@testing-library/react';
import { MyComponent } from './MyComponent';

describe('MyComponent', () => {
  it('renders title', () => {
    render(<MyComponent title="Test" />);
    expect(screen.getByText('Test')).toBeInTheDocument();
  });
});
```

### Debugging

**Frontend:**
```typescript
// Use console.log (remove before PR)
console.log('Debug:', value);

// Use Chrome DevTools
// Right-click → Inspect Element
```

**Backend:**
```rust
// Use tracing (can stay in code)
use tracing::{debug, info, warn, error};

debug!("Processing file: {:?}", path);
info!("Operation complete: {} files moved", count);
warn!("Potential issue: {}", warning);
error!("Failed to execute: {}", err);
```

**Run with logging:**
```bash
RUST_LOG=debug npm run tauri dev
RUST_LOG=sentinel::ai=trace npm run tauri dev
```

## Getting Help

Stuck? Here's how to get help:

1. **Check documentation:**
   - [Getting Started](./getting-started.md)
   - [Architecture](./architecture.md)
   - [API Reference](./api/README.md)

2. **Search existing issues:**
   - [Issue Tracker](https://github.com/lilfourn/sentinel/issues)

3. **Ask in Discussions:**
   - [GitHub Discussions](https://github.com/lilfourn/sentinel/discussions)

4. **Contact maintainers:**
   - Open an issue with questions
   - Tag @lilfourn for urgent issues

## Recognition

Contributors are recognized in:
- README.md contributors section
- Release notes for their contributions
- GitHub contributors page

Thank you for contributing to Sentinel!

---

**Questions?** Open an issue or discussion. We're here to help!
