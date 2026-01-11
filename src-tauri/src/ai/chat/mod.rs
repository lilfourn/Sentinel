//! Chat module for Sentinel Omni-Chat
//!
//! Provides a ReAct-based chat agent that can:
//! - Search files semantically using LocalVectorIndex
//! - Read file contents
//! - Inspect folder patterns using V5 Hologram
//! - Execute shell commands (bash, grep)
//! - Answer questions about the filesystem
//!
//! Supports both Anthropic Claude and OpenAI GPT models.

pub mod agent;
pub mod context;
pub mod openai_provider;
pub mod tool_conversion;
pub mod tools;
pub mod tools_terminal;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use agent::{run_chat_agent, ChatAgentResult, ConversationMessage, TokenUsage};
#[allow(unused_imports)]
pub use context::{hydrate_context, ContextItem, HydratedContext};
#[allow(unused_imports)]
pub use openai_provider::run_openai_chat_agent;
#[allow(unused_imports)]
pub use tools::{execute_chat_tool, get_chat_tools, ChatToolResult};
