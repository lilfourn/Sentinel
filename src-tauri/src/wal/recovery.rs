//! WAL Recovery
//!
//! Provides recovery operations for interrupted jobs including:
//! - Checking for interrupted operations on startup
//! - Resuming incomplete operations
//! - Rolling back failed operations in reverse order

use super::entry::{WALJournal, WALOperationType, WALStatus};
use super::journal::WALManager;
use crate::security::PathValidator;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Information about a recoverable job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInfo {
    /// Job ID of the interrupted job
    pub job_id: String,
    /// Target folder that was being organized
    pub target_folder: String,
    /// Number of operations completed before interruption
    pub completed_count: usize,
    /// Number of operations still pending
    pub pending_count: usize,
    /// Number of operations that failed
    pub failed_count: usize,
    /// When the job was started
    pub started_at: DateTime<Utc>,
    /// Descriptions of pending operations
    pub pending_operations: Vec<String>,
}

/// Result of a recovery operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryResult {
    /// Whether recovery was successful
    pub success: bool,
    /// Number of operations completed during recovery
    pub completed_count: usize,
    /// Number of operations that failed during recovery
    pub failed_count: usize,
    /// Error messages from failed operations
    pub errors: Vec<String>,
}

/// Check for any interrupted jobs that need recovery
///
/// This should be called on application startup to detect
/// jobs that were interrupted due to crash or unexpected shutdown.
pub fn check_for_recovery() -> Result<Option<RecoveryInfo>, String> {
    let manager = WALManager::new();

    let journal = match manager.find_incomplete_journal() {
        Ok(Some(j)) => j,
        Ok(None) => return Ok(None),
        Err(e) => return Err(e.message),
    };

    let (pending, _in_progress, complete, failed) = journal.status_counts();

    // Collect descriptions of pending operations for UI display
    let pending_operations: Vec<String> = journal
        .entries
        .iter()
        .filter(|e| matches!(e.status, WALStatus::Pending | WALStatus::InProgress))
        .map(|e| e.operation.description())
        .collect();

    Ok(Some(RecoveryInfo {
        job_id: journal.job_id,
        target_folder: journal.target_folder.to_string_lossy().to_string(),
        completed_count: complete,
        pending_count: pending,
        failed_count: failed,
        started_at: journal.started_at,
        pending_operations,
    }))
}

/// Resume an interrupted journal by executing remaining pending operations
///
/// This will:
/// 1. Load the journal
/// 2. Execute all pending operations in sequence order
/// 3. Update entry statuses as operations complete or fail
pub fn resume_journal(job_id: &str) -> Result<RecoveryResult, String> {
    let manager = WALManager::new();

    let mut journal = manager
        .load_journal(job_id)
        .map_err(|e| e.message)?
        .ok_or_else(|| format!("Journal not found: {}", job_id))?;

    tracing::info!(
        job_id = %job_id,
        pending = journal.pending_entries().len(),
        "Resuming interrupted journal"
    );

    let mut completed_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    // Get pending entries sorted by sequence
    let mut pending_ids: Vec<(u32, uuid::Uuid)> = journal
        .entries
        .iter()
        .filter(|e| matches!(e.status, WALStatus::Pending | WALStatus::InProgress))
        .map(|e| (e.sequence, e.id))
        .collect();
    pending_ids.sort_by_key(|(seq, _)| *seq);

    for (_, entry_id) in pending_ids {
        // Re-fetch entry after potential mutations
        let entry = journal
            .get_entry(entry_id)
            .ok_or_else(|| format!("Entry not found: {}", entry_id))?
            .clone();

        tracing::debug!(
            operation = %entry.operation.description(),
            "Recovery: Executing operation"
        );

        // Mark as in progress
        if let Some(e) = journal.get_entry_mut(entry_id) {
            e.mark_in_progress();
        }
        manager.save_journal(&journal).map_err(|e| e.message)?;

        // Execute the operation
        match execute_operation(&entry.operation) {
            Ok(()) => {
                if let Some(e) = journal.get_entry_mut(entry_id) {
                    e.mark_complete();
                }
                completed_count += 1;
                tracing::debug!("Recovery: Operation completed successfully");
            }
            Err(err) => {
                if let Some(e) = journal.get_entry_mut(entry_id) {
                    e.mark_failed(err.clone());
                }
                failed_count += 1;
                errors.push(err.clone());
                tracing::debug!(error = %err, "Recovery: Operation failed");
            }
        }

        manager.save_journal(&journal).map_err(|e| e.message)?;
    }

    // If all complete, discard the journal
    if journal.is_complete() {
        manager.discard_journal(job_id).map_err(|e| e.message)?;
    }

    Ok(RecoveryResult {
        success: failed_count == 0,
        completed_count,
        failed_count,
        errors,
    })
}

/// Rollback a journal by executing undo operations in reverse order
///
/// This will:
/// 1. Load the journal
/// 2. Get all completed operations
/// 3. Execute their undo operations in reverse sequence order
/// 4. Mark entries as rolled back
pub fn rollback_journal(job_id: &str) -> Result<RecoveryResult, String> {
    let manager = WALManager::new();

    let mut journal = manager
        .load_journal(job_id)
        .map_err(|e| e.message)?
        .ok_or_else(|| format!("Journal not found: {}", job_id))?;

    tracing::info!(
        job_id = %job_id,
        completed = journal.completed_entries().len(),
        "Rolling back journal"
    );

    let mut completed_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    // Get completed entries sorted by sequence in reverse order
    let mut completed_ids: Vec<(u32, uuid::Uuid)> = journal
        .entries
        .iter()
        .filter(|e| e.status == WALStatus::Complete)
        .map(|e| (e.sequence, e.id))
        .collect();
    completed_ids.sort_by_key(|(seq, _)| std::cmp::Reverse(*seq));

    for (_, entry_id) in completed_ids {
        // Re-fetch entry after potential mutations
        let entry = journal
            .get_entry(entry_id)
            .ok_or_else(|| format!("Entry not found: {}", entry_id))?
            .clone();

        tracing::debug!(
            operation = %entry.undo_operation.description(),
            "Rolling back operation"
        );

        // Execute the undo operation
        match execute_operation(&entry.undo_operation) {
            Ok(()) => {
                if let Some(e) = journal.get_entry_mut(entry_id) {
                    e.mark_rolled_back();
                }
                completed_count += 1;
                tracing::debug!("Rollback completed successfully");
            }
            Err(err) => {
                // Even if undo fails, mark as rolled back to avoid retry loops
                if let Some(e) = journal.get_entry_mut(entry_id) {
                    e.mark_rolled_back();
                    e.error = Some(format!("Rollback failed: {}", err));
                }
                failed_count += 1;
                errors.push(err.clone());
                tracing::debug!(error = %err, "Rollback failed");
            }
        }

        manager.save_journal(&journal).map_err(|e| e.message)?;
    }

    // Also mark any pending entries as rolled back
    let pending_ids: Vec<uuid::Uuid> = journal
        .entries
        .iter()
        .filter(|e| matches!(e.status, WALStatus::Pending | WALStatus::InProgress))
        .map(|e| e.id)
        .collect();

    for entry_id in pending_ids {
        if let Some(e) = journal.get_entry_mut(entry_id) {
            e.mark_rolled_back();
        }
    }
    manager.save_journal(&journal).map_err(|e| e.message)?;

    // Discard the journal after rollback
    manager.discard_journal(job_id).map_err(|e| e.message)?;

    Ok(RecoveryResult {
        success: failed_count == 0,
        completed_count,
        failed_count,
        errors,
    })
}

/// Discard a journal without executing any operations
///
/// Use this when the user wants to abandon the interrupted job
/// without attempting recovery or rollback.
pub fn discard_journal(job_id: &str) -> Result<(), String> {
    let manager = WALManager::new();
    manager.discard_journal(job_id).map_err(|e| e.message)
}

/// Execute a single WAL operation
///
/// This function performs the actual filesystem operation.
/// It's used by both resume and rollback paths.
fn execute_operation(operation: &WALOperationType) -> Result<(), String> {
    match operation {
        WALOperationType::CreateFolder { path } => {
            if path.exists() {
                // Already exists, consider it success
                return Ok(());
            }
            fs::create_dir_all(path)
                .map_err(|e| format!("Failed to create folder {}: {}", path.display(), e))
        }

        WALOperationType::Move {
            source,
            destination,
        } => {
            if !source.exists() {
                // Source doesn't exist - might have already been moved
                if destination.exists() {
                    return Ok(());
                }
                return Err(format!("Source not found: {}", source.display()));
            }

            if destination.exists() {
                return Err(format!("Destination already exists: {}", destination.display()));
            }

            // Validate source is not protected
            if PathValidator::is_protected_path(source) {
                return Err(format!("Cannot move protected path: {}", source.display()));
            }

            // Try rename first (same filesystem), fall back to copy+delete
            if fs::rename(source, destination).is_err() {
                if source.is_dir() {
                    copy_dir_all(source, destination)?;
                    fs::remove_dir_all(source)
                        .map_err(|e| format!("Failed to remove source: {}", e))?;
                } else {
                    fs::copy(source, destination)
                        .map_err(|e| format!("Failed to copy: {}", e))?;
                    fs::remove_file(source)
                        .map_err(|e| format!("Failed to remove source: {}", e))?;
                }
            }

            Ok(())
        }

        WALOperationType::Rename { path, new_name } => {
            if !path.exists() {
                return Err(format!("Path not found: {}", path.display()));
            }

            let parent = path
                .parent()
                .ok_or_else(|| format!("Cannot determine parent of {}", path.display()))?;
            let new_path = parent.join(new_name);

            if new_path.exists() {
                return Err(format!("Target already exists: {}", new_path.display()));
            }

            if PathValidator::is_protected_path(path) {
                return Err(format!("Cannot rename protected path: {}", path.display()));
            }

            fs::rename(path, &new_path)
                .map_err(|e| format!("Failed to rename {} to {}: {}", path.display(), new_name, e))
        }

        WALOperationType::Quarantine {
            path,
            quarantine_path,
        } => {
            // Quarantine is just a move to a special location
            execute_operation(&WALOperationType::Move {
                source: path.clone(),
                destination: quarantine_path.clone(),
            })
        }

        WALOperationType::Copy {
            source,
            destination,
        } => {
            if !source.exists() {
                return Err(format!("Source not found: {}", source.display()));
            }

            if destination.exists() {
                return Err(format!("Destination already exists: {}", destination.display()));
            }

            if source.is_dir() {
                copy_dir_all(source, destination)
            } else {
                fs::copy(source, destination)
                    .map_err(|e| format!("Failed to copy: {}", e))
                    .map(|_| ())
            }
        }

        WALOperationType::DeleteFolder { path } => {
            if !path.exists() {
                // Already deleted, consider it success
                return Ok(());
            }

            if !path.is_dir() {
                // It's a file, try to delete it
                return fs::remove_file(path)
                    .map_err(|e| format!("Failed to delete file {}: {}", path.display(), e));
            }

            // Safety check: only delete empty directories or copied items
            let is_empty = fs::read_dir(path)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);

            if is_empty {
                fs::remove_dir(path)
                    .map_err(|e| format!("Failed to delete folder {}: {}", path.display(), e))
            } else {
                // For non-empty directories (from copy undo), use remove_dir_all
                // but only if it's not a protected path
                if PathValidator::is_protected_path(path) {
                    return Err(format!("Cannot delete protected path: {}", path.display()));
                }
                fs::remove_dir_all(path)
                    .map_err(|e| format!("Failed to delete folder {}: {}", path.display(), e))
            }
        }
    }
}

/// Helper function to copy a directory recursively
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("Failed to create directory: {}", e))?;

    for entry in fs::read_dir(src).map_err(|e| format!("Failed to read directory: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let ty = entry
            .file_type()
            .map_err(|e| format!("Failed to get file type: {}", e))?;

        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| format!("Failed to copy file: {}", e))?;
        }
    }

    Ok(())
}

/// Get details about a specific journal for recovery UI
pub fn get_journal_details(job_id: &str) -> Result<Option<WALJournal>, String> {
    let manager = WALManager::new();
    manager.load_journal(job_id).map_err(|e| e.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_execute_create_folder() {
        let dir = tempdir().unwrap();
        let new_folder = dir.path().join("test_folder");

        let op = WALOperationType::CreateFolder {
            path: new_folder.clone(),
        };

        execute_operation(&op).unwrap();
        assert!(new_folder.exists());
    }

    #[test]
    fn test_execute_rename() {
        let dir = tempdir().unwrap();
        let original = dir.path().join("original.txt");
        fs::write(&original, "test content").unwrap();

        let op = WALOperationType::Rename {
            path: original.clone(),
            new_name: "renamed.txt".to_string(),
        };

        execute_operation(&op).unwrap();
        assert!(!original.exists());
        assert!(dir.path().join("renamed.txt").exists());
    }

    #[test]
    fn test_execute_move() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source.txt");
        let dest = dir.path().join("subdir").join("dest.txt");

        fs::write(&source, "test content").unwrap();
        fs::create_dir_all(dest.parent().unwrap()).unwrap();

        let op = WALOperationType::Move {
            source: source.clone(),
            destination: dest.clone(),
        };

        execute_operation(&op).unwrap();
        assert!(!source.exists());
        assert!(dest.exists());
    }

    #[test]
    fn test_execute_copy() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source.txt");
        let dest = dir.path().join("copy.txt");

        fs::write(&source, "test content").unwrap();

        let op = WALOperationType::Copy {
            source: source.clone(),
            destination: dest.clone(),
        };

        execute_operation(&op).unwrap();
        assert!(source.exists());
        assert!(dest.exists());
    }
}
