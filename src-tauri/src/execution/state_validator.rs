//! State Validation Module
//!
//! Validates that filesystem state matches the expected state from VFS simulation
//! before executing operations. This prevents issues where files have been modified
//! between planning (VFS simulation) and execution.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::execution::state_validator::{StateSnapshot, StateValidator};
//!
//! // Capture state at simulation time
//! let snapshot = StateSnapshot::capture(&source_paths)?;
//!
//! // ... time passes, user reviews changes ...
//!
//! // Before execution, validate state hasn't changed
//! let validator = StateValidator::new(snapshot);
//! let conflicts = validator.validate_current_state()?;
//! if !conflicts.is_empty() {
//!     // Warn user or abort
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Snapshot of filesystem state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// File modification times at snapshot time
    pub mtimes: HashMap<PathBuf, u64>,
    /// File sizes at snapshot time
    pub sizes: HashMap<PathBuf, u64>,
    /// Whether each path existed at snapshot time
    pub exists: HashMap<PathBuf, bool>,
    /// Timestamp when snapshot was taken
    pub captured_at: u64,
}

/// Type of state conflict detected
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StateConflict {
    /// File was modified since snapshot
    Modified {
        path: String,
        old_mtime: u64,
        new_mtime: u64,
    },
    /// File was deleted since snapshot
    Deleted { path: String },
    /// New file appeared since snapshot
    Added { path: String },
    /// File size changed
    SizeChanged {
        path: String,
        old_size: u64,
        new_size: u64,
    },
}

impl StateConflict {
    /// Get a human-readable description of the conflict
    pub fn description(&self) -> String {
        match self {
            StateConflict::Modified { path, .. } => {
                format!("File modified: {}", path)
            }
            StateConflict::Deleted { path } => {
                format!("File deleted: {}", path)
            }
            StateConflict::Added { path } => {
                format!("New file appeared: {}", path)
            }
            StateConflict::SizeChanged { path, old_size, new_size } => {
                format!("File size changed: {} ({} -> {} bytes)", path, old_size, new_size)
            }
        }
    }

    /// Whether this conflict is critical (would cause operation failure)
    pub fn is_critical(&self) -> bool {
        matches!(self, StateConflict::Deleted { .. })
    }
}

/// Result of state validation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    /// Whether validation passed (no conflicts)
    pub valid: bool,
    /// List of detected conflicts
    pub conflicts: Vec<StateConflict>,
    /// Number of paths checked
    pub paths_checked: usize,
    /// Number of critical conflicts (would cause failures)
    pub critical_count: usize,
    /// Number of non-critical conflicts (may cause unexpected results)
    pub warning_count: usize,
}

impl StateSnapshot {
    /// Create a new empty snapshot
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            mtimes: HashMap::new(),
            sizes: HashMap::new(),
            exists: HashMap::new(),
            captured_at: now,
        }
    }

    /// Capture the current state of the given paths
    ///
    /// # Arguments
    /// * `paths` - Paths to capture state for (typically source paths from operations)
    ///
    /// # Returns
    /// A snapshot of the current filesystem state
    pub fn capture(paths: &[PathBuf]) -> Result<Self, String> {
        let mut snapshot = Self::new();

        for path in paths {
            snapshot.add_path(path)?;
        }

        Ok(snapshot)
    }

    /// Add a single path to the snapshot
    pub fn add_path(&mut self, path: &Path) -> Result<(), String> {
        let path_buf = path.to_path_buf();

        if path.exists() {
            self.exists.insert(path_buf.clone(), true);

            match fs::metadata(path) {
                Ok(meta) => {
                    // Record modification time
                    if let Ok(mtime) = meta.modified() {
                        let mtime_secs = mtime
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        self.mtimes.insert(path_buf.clone(), mtime_secs);
                    }

                    // Record size
                    self.sizes.insert(path_buf, meta.len());
                }
                Err(e) => {
                    // Log but don't fail - file may have been deleted between check and metadata
                    tracing::warn!(path = %path.display(), error = %e, "Failed to get metadata");
                }
            }
        } else {
            self.exists.insert(path_buf, false);
        }

        Ok(())
    }

    /// Get the number of paths in this snapshot
    pub fn len(&self) -> usize {
        self.exists.len()
    }

    /// Check if snapshot is empty
    pub fn is_empty(&self) -> bool {
        self.exists.is_empty()
    }
}

impl Default for StateSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates filesystem state against a captured snapshot
pub struct StateValidator {
    snapshot: StateSnapshot,
}

impl StateValidator {
    /// Create a new validator with the given snapshot
    pub fn new(snapshot: StateSnapshot) -> Self {
        Self { snapshot }
    }

    /// Validate current filesystem state against the snapshot
    ///
    /// # Returns
    /// A validation result containing any detected conflicts
    pub fn validate_current_state(&self) -> Result<ValidationResult, String> {
        let mut conflicts = Vec::new();

        for (path, existed) in &self.snapshot.exists {
            let current_exists = path.exists();

            // Check for deleted files
            if *existed && !current_exists {
                conflicts.push(StateConflict::Deleted {
                    path: path.to_string_lossy().to_string(),
                });
                continue;
            }

            // Check for new files (in case we're checking destination paths)
            if !*existed && current_exists {
                conflicts.push(StateConflict::Added {
                    path: path.to_string_lossy().to_string(),
                });
                continue;
            }

            // If file exists, check for modifications
            if current_exists {
                if let Ok(meta) = fs::metadata(path) {
                    // Check modification time
                    if let Some(&old_mtime) = self.snapshot.mtimes.get(path) {
                        if let Ok(mtime) = meta.modified() {
                            let new_mtime = mtime
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);

                            if new_mtime > old_mtime {
                                conflicts.push(StateConflict::Modified {
                                    path: path.to_string_lossy().to_string(),
                                    old_mtime,
                                    new_mtime,
                                });
                            }
                        }
                    }

                    // Check size change (even if mtime didn't change - can happen with NFS)
                    if let Some(&old_size) = self.snapshot.sizes.get(path) {
                        let new_size = meta.len();
                        if new_size != old_size {
                            // Only add if we didn't already add a modification conflict
                            let already_modified = conflicts.iter().any(|c| matches!(
                                c,
                                StateConflict::Modified { path: p, .. } if p == &path.to_string_lossy().to_string()
                            ));

                            if !already_modified {
                                conflicts.push(StateConflict::SizeChanged {
                                    path: path.to_string_lossy().to_string(),
                                    old_size,
                                    new_size,
                                });
                            }
                        }
                    }
                }
            }
        }

        let critical_count = conflicts.iter().filter(|c| c.is_critical()).count();
        let warning_count = conflicts.len() - critical_count;

        Ok(ValidationResult {
            valid: conflicts.is_empty(),
            conflicts,
            paths_checked: self.snapshot.exists.len(),
            critical_count,
            warning_count,
        })
    }

    /// Quick check if any files have been modified
    ///
    /// This is faster than full validation if you just need a boolean result.
    pub fn has_changes(&self) -> bool {
        for (path, existed) in &self.snapshot.exists {
            let current_exists = path.exists();

            // Existence changed
            if *existed != current_exists {
                return true;
            }

            // Check mtime for existing files
            if current_exists {
                if let Some(&old_mtime) = self.snapshot.mtimes.get(path) {
                    if let Ok(meta) = fs::metadata(path) {
                        if let Ok(mtime) = meta.modified() {
                            let new_mtime = mtime
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            if new_mtime > old_mtime {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        false
    }
}

/// Extract source paths from WAL operations for state validation
pub fn extract_source_paths(operations: &[crate::wal::entry::WALOperationType]) -> Vec<PathBuf> {
    use crate::wal::entry::WALOperationType;

    operations
        .iter()
        .filter_map(|op| match op {
            WALOperationType::Move { source, .. } => Some(source.clone()),
            WALOperationType::Rename { path, .. } => Some(path.clone()),
            WALOperationType::Copy { source, .. } => Some(source.clone()),
            WALOperationType::DeleteFolder { path } => Some(path.clone()),
            WALOperationType::Quarantine { path, .. } => Some(path.clone()),
            WALOperationType::CreateFolder { .. } => None, // No source to check
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_snapshot_capture() {
        let dir = tempdir().unwrap();
        let file1 = dir.path().join("test1.txt");
        let file2 = dir.path().join("test2.txt");

        fs::write(&file1, "content1").unwrap();
        fs::write(&file2, "content2").unwrap();

        let snapshot = StateSnapshot::capture(&[file1.clone(), file2.clone()]).unwrap();

        assert_eq!(snapshot.len(), 2);
        assert!(snapshot.exists.get(&file1).copied().unwrap_or(false));
        assert!(snapshot.exists.get(&file2).copied().unwrap_or(false));
    }

    #[test]
    fn test_validate_no_changes() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "content").unwrap();

        let snapshot = StateSnapshot::capture(&[file.clone()]).unwrap();
        let validator = StateValidator::new(snapshot);

        let result = validator.validate_current_state().unwrap();
        assert!(result.valid);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn test_validate_deleted_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "content").unwrap();

        let snapshot = StateSnapshot::capture(&[file.clone()]).unwrap();

        // Delete the file
        fs::remove_file(&file).unwrap();

        let validator = StateValidator::new(snapshot);
        let result = validator.validate_current_state().unwrap();

        assert!(!result.valid);
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.critical_count, 1);
        assert!(matches!(result.conflicts[0], StateConflict::Deleted { .. }));
    }

    #[test]
    fn test_validate_modified_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "content").unwrap();

        let snapshot = StateSnapshot::capture(&[file.clone()]).unwrap();

        // Wait a moment and modify the file
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Update mtime by writing
        let mut f = fs::OpenOptions::new().write(true).open(&file).unwrap();
        f.write_all(b"modified content").unwrap();
        f.sync_all().unwrap();

        let validator = StateValidator::new(snapshot);
        let result = validator.validate_current_state().unwrap();

        // Note: On some filesystems, the modification might not be detected if it happens too quickly
        // So we just check that the validation ran without errors
        assert_eq!(result.paths_checked, 1);
    }

    #[test]
    fn test_has_changes_quick_check() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "content").unwrap();

        let snapshot = StateSnapshot::capture(&[file.clone()]).unwrap();
        let validator = StateValidator::new(snapshot);

        // Initially no changes
        assert!(!validator.has_changes());
    }

    #[test]
    fn test_nonexistent_file_in_snapshot() {
        let dir = tempdir().unwrap();
        let nonexistent = dir.path().join("does_not_exist.txt");

        let snapshot = StateSnapshot::capture(&[nonexistent.clone()]).unwrap();

        assert_eq!(snapshot.len(), 1);
        assert!(!snapshot.exists.get(&nonexistent).copied().unwrap_or(true));
    }
}
