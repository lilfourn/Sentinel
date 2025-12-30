//! V3 AI module with semantic, rule-based file organization.
//!
//! This module upgrades V2 with:
//! - **Prompt caching**: 90% token reduction via Anthropic cache_control
//! - **LocalVectorIndex**: Real semantic search using fastembed embeddings
//! - **Header-based rate limiting**: Dynamic delays based on API quota
//! - **FolderDigest**: Rich pre-computed analytics for one-shot planning
//!
//! Tools available to the agent:
//! - `query_semantic_index`: Search files by semantic similarity
//! - `apply_organization_rules`: Define rules for bulk file operations
//! - `preview_operations`: Preview planned changes before execution
//! - `commit_plan`: Finalize and submit the organization plan

#![allow(dead_code)]

mod analytics;
pub mod compression;
mod local_vector_index;
mod prompts;
mod rate_limiter;
mod sampling;
mod tools;
mod vfs;

pub mod agent_loop;

// Public exports
pub use agent_loop::{run_v2_agentic_organize, ExpandableDetail};
pub use analytics::{ContentPreview, DigestGenerator, FolderDigest, SemanticTag};
pub use local_vector_index::{LocalVectorConfig, LocalVectorIndex};
pub use rate_limiter::{RateLimitManager, RateLimitState};
