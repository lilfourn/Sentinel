//! History module for tracking organization sessions and enabling multi-level undo.
//!
//! This module provides:
//! - `entry`: Data structures for history sessions and operations
//! - `checksum`: SHA-256 file integrity verification
//! - `store`: Persistence manager for history files
//! - `undo`: Undo algorithm with conflict detection

mod checksum;
mod entry;
mod store;
mod undo;

pub use checksum::*;
pub use entry::*;
pub use store::*;
pub use undo::*;
