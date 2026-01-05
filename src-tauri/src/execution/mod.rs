//! Execution Engine Module
//!
//! Provides parallel execution of file operations using a DAG-based
//! dependency graph. Operations at the same level (no dependencies between
//! them) are executed in parallel for optimal performance.
//!
//! # State Validation
//!
//! The `state_validator` submodule provides tools for validating that filesystem
//! state matches expected state from VFS simulation before execution. This prevents
//! issues where files have been modified between planning and execution.

#![allow(dead_code)]
#![allow(unused_imports)]

pub mod dag;
pub mod executor;
pub mod state_validator;

pub use dag::*;
pub use executor::*;
pub use state_validator::*;
