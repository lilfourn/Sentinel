//! Chat module for Sentinel Omni-Chat
//!
//! Provides a ReAct-based chat agent that can:
//! - Search files semantically using LocalVectorIndex
//! - Read file contents
//! - Inspect folder patterns using V5 Hologram
//! - Execute shell commands (bash, grep)
//! - Answer questions about the filesystem

pub mod agent;
pub mod context;
pub mod tools;
pub mod tools_terminal;

#[allow(unused_imports)]
pub use agent::{run_chat_agent, ChatAgentResult, ConversationMessage, TokenUsage};
#[allow(unused_imports)]
pub use context::{hydrate_context, ContextItem, HydratedContext};
#[allow(unused_imports)]
pub use tools::{execute_chat_tool, get_chat_tools, ChatToolResult};
