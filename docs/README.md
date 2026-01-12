# Sentinel Documentation

Welcome to the Sentinel documentation. This guide will help you understand, develop, and contribute to Sentinel—an AI-powered file manager built with Tauri v2 and React 19.

## Quick Navigation

### Getting Started
- [Getting Started Guide](./getting-started.md) - Setup, installation, and first steps
- [Architecture Overview](./architecture.md) - System design and component relationships

### Component Documentation
- [Frontend Documentation](./frontend/README.md) - React components, stores, and UI layer
- [Backend Documentation](./backend/README.md) - Rust modules, AI integration, and core logic
- [API Reference](./api/README.md) - Tauri commands, events, and type definitions

### Feature Guides
- [Features Overview](./features/README.md) - Detailed feature documentation
  - AI Chat Agent
  - Organization System
  - Virtual File System (VFS)
  - Write-Ahead Log (WAL)
  - Vector Search
  - Crash Recovery

### Contributing
- [Contributing Guide](./contributing.md) - How to contribute to Sentinel

## What is Sentinel?

Sentinel is a desktop file manager that uses AI to understand and organize your files intelligently. Unlike traditional file managers, Sentinel can:

- **Understand file contents** - Not just names, but what's actually inside documents, images, and spreadsheets
- **Organize autonomously** - Describe what you want, and Sentinel creates a complete reorganization plan
- **Preview safely** - All changes simulate in a virtual filesystem before anything moves
- **Recover gracefully** - Crash recovery ensures no data loss, even if your machine fails mid-operation
- **Search semantically** - Find files by meaning, not just keywords

## Architecture at a Glance

```
┌──────────────────────────────────────────────────────┐
│              Frontend (React 19)                     │
│  Chat • File Browser • Preview • Execution Monitor  │
└──────────────────┬───────────────────────────────────┘
                   │ Tauri IPC
┌──────────────────┴───────────────────────────────────┐
│              Backend (Rust + Tauri v2)               │
│  AI Agents • VFS • WAL • Vector Search • Execution   │
└──────────────────┬───────────────────────────────────┘
                   │ HTTPS
┌──────────────────┴───────────────────────────────────┐
│            Claude AI (Anthropic API)                 │
│         Haiku • Sonnet • Opus                        │
└──────────────────────────────────────────────────────┘
```

## Tech Stack

### Frontend
| Technology | Version | Purpose |
|------------|---------|---------|
| React | 19.1 | UI framework |
| TypeScript | 5.8 | Type safety |
| Vite | 7.0 | Build tool |
| TailwindCSS | 4.1 | Styling |
| Zustand | 5.0 | State management |
| TanStack Query | 5.90 | Async state |

### Backend
| Technology | Version | Purpose |
|------------|---------|---------|
| Tauri | 2.0 | Desktop runtime |
| Rust | 1.70+ | Backend logic |
| tokio | 1.0 | Async runtime |
| fastembed | 4.0 | Local embeddings |
| petgraph | 0.6 | DAG operations |
| reqwest | 0.12 | HTTP client |

## Documentation Philosophy

This documentation follows these principles:

1. **Clarity First** - Simple explanations before complex details
2. **Code Examples** - Every concept includes working examples
3. **Visual Aids** - Diagrams to explain architecture and flows
4. **Practical Focus** - Real-world use cases and scenarios
5. **Maintainability** - Easy to update as code evolves

## Finding Your Way

**New to Sentinel?** Start with the [Getting Started Guide](./getting-started.md).

**Contributing code?** Check the [Contributing Guide](./contributing.md) and explore the [Frontend](./frontend/README.md) or [Backend](./backend/README.md) docs.

**Integrating Sentinel?** See the [API Reference](./api/README.md) for all available commands and events.

**Understanding features?** Read the [Features Guide](./features/README.md) for detailed explanations.

## Project Links

- [Main Repository](https://github.com/lilfourn/sentinel)
- [Issue Tracker](https://github.com/lilfourn/sentinel/issues)
- [Discussions](https://github.com/lilfourn/sentinel/discussions)

## Support

If you need help:
1. Check the relevant documentation section
2. Search existing [issues](https://github.com/lilfourn/sentinel/issues)
3. Join our [discussions](https://github.com/lilfourn/sentinel/discussions)
4. Create a new issue with details

---

**Last Updated**: January 2026
**Sentinel Version**: 0.1.0
