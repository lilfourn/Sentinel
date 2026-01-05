//! V6 AI module with semantic, rule-based file organization.
//!
//! This module provides:
//! - **Architect-Builder pattern**: High-level planning with Blueprint-guided execution
//! - **Prompt caching**: 90% token reduction via Anthropic cache_control
//! - **LocalVectorIndex**: Real semantic search using fastembed embeddings
//! - **Header-based rate limiting**: Dynamic delays based on API quota
//! - **FolderDigest**: Rich pre-computed analytics for one-shot planning
//!
//! V6 Features (new):
//! - **Architect module**: Generates Blueprint from user instruction + semantic sample
//! - **Builder module**: Tiered file matching (vector first, LLM fallback)
//!
//! Tools available to the agent:
//! - `query_semantic_index`: Search files by semantic similarity
//! - `apply_organization_rules`: Define rules for bulk file operations
//! - `preview_operations`: Preview planned changes before execution
//! - `commit_plan`: Finalize and submit the organization plan

#![allow(dead_code)]

mod analytics;
pub mod architect;
pub mod builder;
pub mod compression;
mod local_vector_index;
pub mod prompts;
mod rate_limiter;
mod sampling;
mod tools;
mod vfs;

pub mod agent_loop;

// Public exports
pub use agent_loop::{run_v6_hybrid_organization, run_simplification_loop, ExpandableDetail, ProgressEvent};
#[allow(unused_imports)]
pub use analytics::{ContentPreview, DigestGenerator, FolderDigest, SemanticTag};
#[allow(unused_imports)]
pub use architect::{Blueprint, BlueprintFolder};
#[allow(unused_imports)]
pub use builder::{BatchMatchResult, MatchResult};
#[allow(unused_imports)]
pub use local_vector_index::{LocalVectorConfig, LocalVectorIndex};
#[allow(unused_imports)]
pub use rate_limiter::{RateLimitManager, RateLimitState};
