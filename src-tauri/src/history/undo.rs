//! Undo algorithm with conflict detection and resolution.

use crate::history::checksum::compute_file_checksum;
use crate::history::entry::{HistoryOperation, HistorySession, OperationRecord};
use crate::history::store::HistoryStore;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Result of preflight check before undo
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoPreflightResult {
    /// Whether undo can proceed (no critical conflicts)
    pub can_proceed: bool,
    /// Files that have been modified since organization
    pub modified_files: Vec<ConflictInfo>,
    /// Files that are missing (deleted externally)
    pub missing_files: Vec<String>,
    /// Files that would block undo (exist at original location)
    pub blocking_files: Vec<String>,
    /// Number of operations that can be safely undone
    pub safe_operations: usize,
    /// Number of operations with conflicts
    pub conflicted_operations: usize,
    /// Total operations to undo
    pub total_operations: usize,
}

/// Information about a file conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    /// File path
    pub path: String,
    /// Expected checksum (from history)
    pub expected_sha256: String,
    /// Current checksum (None if file missing)
    pub current_sha256: Option<String>,
    /// Type of conflict
    pub conflict_type: ConflictType,
}

/// Type of conflict detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictType {
    /// File content has been modified
    Modified,
    /// File has been deleted
    Deleted,
    /// File exists at the undo destination
    Blocking,
}

/// How to resolve conflicts during undo
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Abort the entire undo operation
    Abort,
    /// Skip conflicted operations, undo the rest
    Skip,
    /// Force undo (overwrite/ignore conflicts)
    Force,
    /// Create backup copies before overwriting
    Backup,
}

impl ConflictResolution {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "abort" => Some(Self::Abort),
            "skip" => Some(Self::Skip),
            "force" => Some(Self::Force),
            "backup" => Some(Self::Backup),
            _ => None,
        }
    }
}

/// Result of undo execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    /// Whether undo was successful
    pub success: bool,
    /// Number of operations undone
    pub operations_undone: usize,
    /// Number of operations skipped
    pub operations_skipped: usize,
    /// Error messages
    pub errors: Vec<String>,
}

/// Perform preflight check before undo
///
/// This verifies the current state of files and identifies any conflicts
/// that need to be resolved before undo can proceed.
pub fn preflight_undo(
    folder_path: &str,
    target_session_id: &str,
) -> Result<UndoPreflightResult, String> {
    let store = HistoryStore::new();
    let history = store
        .load_history(folder_path)?
        .ok_or_else(|| format!("No history found for {}", folder_path))?;

    // Find the target session index
    let target_idx = history
        .sessions
        .iter()
        .position(|s| s.session_id == target_session_id)
        .ok_or_else(|| format!("Session {} not found", target_session_id))?;

    // Collect all operations to undo (from current to target, inclusive)
    // Sessions are stored most-recent-first, so we take sessions from 0 to target_idx
    let sessions_to_undo: Vec<&HistorySession> =
        history.sessions[0..=target_idx].iter().collect();

    let mut modified_files = Vec::new();
    let mut missing_files = Vec::new();
    let mut blocking_files = Vec::new();
    let mut safe_operations = 0;
    let mut conflicted_operations = 0;
    let mut total_operations = 0;

    // Check each session's operations
    for session in &sessions_to_undo {
        // Skip already-undone sessions
        if session.undone {
            continue;
        }

        for op in &session.operations {
            total_operations += 1;

            // Check for conflicts based on operation type
            match check_operation_conflicts(op) {
                Ok(conflicts) => {
                    if conflicts.is_empty() {
                        safe_operations += 1;
                    } else {
                        conflicted_operations += 1;
                        for conflict in conflicts {
                            match conflict.conflict_type {
                                ConflictType::Modified => modified_files.push(conflict),
                                ConflictType::Deleted => missing_files.push(conflict.path),
                                ConflictType::Blocking => blocking_files.push(conflict.path),
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Error checking operation {}: {}", op.id, e);
                    conflicted_operations += 1;
                }
            }
        }
    }

    // Undo can proceed if there are no blocking files and at least some safe operations
    let can_proceed = blocking_files.is_empty()
        && (safe_operations > 0 || modified_files.is_empty() && missing_files.is_empty());

    Ok(UndoPreflightResult {
        can_proceed,
        modified_files,
        missing_files,
        blocking_files,
        safe_operations,
        conflicted_operations,
        total_operations,
    })
}

/// Check a single operation for conflicts
fn check_operation_conflicts(op: &HistoryOperation) -> Result<Vec<ConflictInfo>, String> {
    let mut conflicts = Vec::new();

    match &op.operation {
        OperationRecord::Move { source, destination } => {
            // For move undo, we need the file to still be at destination
            // and nothing blocking at source
            let dest_path = Path::new(destination);
            let source_path = Path::new(source);

            // Check if file still exists at destination
            if !dest_path.exists() {
                conflicts.push(ConflictInfo {
                    path: destination.clone(),
                    expected_sha256: op
                        .result_checksums
                        .get(destination)
                        .map(|c| c.sha256.clone())
                        .unwrap_or_default(),
                    current_sha256: None,
                    conflict_type: ConflictType::Deleted,
                });
            } else {
                // Check if file content has changed
                if let Some(expected) = op.result_checksums.get(destination) {
                    if !expected.is_directory {
                        if let Ok(current) = compute_file_checksum(dest_path) {
                            if current.sha256 != expected.sha256 {
                                conflicts.push(ConflictInfo {
                                    path: destination.clone(),
                                    expected_sha256: expected.sha256.clone(),
                                    current_sha256: Some(current.sha256),
                                    conflict_type: ConflictType::Modified,
                                });
                            }
                        }
                    }
                }
            }

            // Check if something blocks the source path
            if source_path.exists() {
                conflicts.push(ConflictInfo {
                    path: source.clone(),
                    expected_sha256: String::new(),
                    current_sha256: None,
                    conflict_type: ConflictType::Blocking,
                });
            }
        }

        OperationRecord::Rename { path, new_name } => {
            // For rename undo, check the new path exists
            let path_buf = std::path::PathBuf::from(path);
            let parent = path_buf.parent().unwrap_or(Path::new(""));
            let new_path = parent.join(new_name);

            if !new_path.exists() {
                conflicts.push(ConflictInfo {
                    path: new_path.to_string_lossy().to_string(),
                    expected_sha256: String::new(),
                    current_sha256: None,
                    conflict_type: ConflictType::Deleted,
                });
            }

            // Check if original name is blocked
            let path_path = Path::new(path);
            if path_path.exists() {
                conflicts.push(ConflictInfo {
                    path: path.clone(),
                    expected_sha256: String::new(),
                    current_sha256: None,
                    conflict_type: ConflictType::Blocking,
                });
            }
        }

        OperationRecord::CreateFolder { path } => {
            // To undo a folder creation, the folder must still exist
            let folder_path = Path::new(path);
            if !folder_path.exists() {
                conflicts.push(ConflictInfo {
                    path: path.clone(),
                    expected_sha256: String::new(),
                    current_sha256: None,
                    conflict_type: ConflictType::Deleted,
                });
            }
        }

        OperationRecord::DeleteFolder { path } => {
            // Undoing a delete means recreating - check if path is blocked
            let folder_path = Path::new(path);
            if folder_path.exists() {
                conflicts.push(ConflictInfo {
                    path: path.clone(),
                    expected_sha256: String::new(),
                    current_sha256: None,
                    conflict_type: ConflictType::Blocking,
                });
            }
        }

        OperationRecord::Copy { source: _, destination } => {
            // To undo a copy, we delete the destination
            let dest_path = Path::new(destination);
            if !dest_path.exists() {
                // Already gone - that's fine, just skip
                // (not a conflict, just nothing to do)
            }
        }

        OperationRecord::Quarantine { path, quarantine_path } => {
            // To undo quarantine, move from quarantine back to original
            let qpath = Path::new(quarantine_path);
            let orig_path = Path::new(path);

            if !qpath.exists() {
                conflicts.push(ConflictInfo {
                    path: quarantine_path.clone(),
                    expected_sha256: String::new(),
                    current_sha256: None,
                    conflict_type: ConflictType::Deleted,
                });
            }

            if orig_path.exists() {
                conflicts.push(ConflictInfo {
                    path: path.clone(),
                    expected_sha256: String::new(),
                    current_sha256: None,
                    conflict_type: ConflictType::Blocking,
                });
            }
        }
    }

    Ok(conflicts)
}

/// Collect undo operations from sessions
///
/// Returns operations in reverse order (most recent first) for correct undo sequencing.
pub fn collect_undo_operations(
    folder_path: &str,
    target_session_id: &str,
) -> Result<Vec<OperationRecord>, String> {
    let store = HistoryStore::new();
    let history = store
        .load_history(folder_path)?
        .ok_or_else(|| format!("No history found for {}", folder_path))?;

    // Find the target session index
    let target_idx = history
        .sessions
        .iter()
        .position(|s| s.session_id == target_session_id)
        .ok_or_else(|| format!("Session {} not found", target_session_id))?;

    let mut undo_operations = Vec::new();

    // Collect operations from sessions (most recent first)
    for session in &history.sessions[0..=target_idx] {
        if session.undone {
            continue;
        }

        // Add operations in reverse order within each session
        for op in session.operations.iter().rev() {
            undo_operations.push(op.undo_operation.clone());
        }
    }

    Ok(undo_operations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::entry::FileChecksum;
    use std::collections::HashMap;

    fn create_test_op(source: &str, dest: &str) -> HistoryOperation {
        HistoryOperation {
            id: "op-1".to_string(),
            sequence: 0,
            operation: OperationRecord::Move {
                source: source.to_string(),
                destination: dest.to_string(),
            },
            undo_operation: OperationRecord::Move {
                source: dest.to_string(),
                destination: source.to_string(),
            },
            source_checksums: HashMap::new(),
            result_checksums: HashMap::new(),
        }
    }

    #[test]
    fn test_conflict_resolution_from_str() {
        assert_eq!(
            ConflictResolution::from_str("abort"),
            Some(ConflictResolution::Abort)
        );
        assert_eq!(
            ConflictResolution::from_str("SKIP"),
            Some(ConflictResolution::Skip)
        );
        assert_eq!(
            ConflictResolution::from_str("Force"),
            Some(ConflictResolution::Force)
        );
        assert_eq!(
            ConflictResolution::from_str("backup"),
            Some(ConflictResolution::Backup)
        );
        assert_eq!(ConflictResolution::from_str("invalid"), None);
    }

    #[test]
    fn test_operation_record_inverse() {
        let op = OperationRecord::Move {
            source: "/a".to_string(),
            destination: "/b".to_string(),
        };
        let inverse = op.inverse();
        assert_eq!(
            inverse,
            OperationRecord::Move {
                source: "/b".to_string(),
                destination: "/a".to_string(),
            }
        );
    }
}
