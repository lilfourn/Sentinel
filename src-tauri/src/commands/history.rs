//! Tauri commands for organization history and undo operations.

use crate::history::{
    collect_undo_operations, preflight_undo, ConflictResolution, FolderIndexEntry,
    HistorySession, HistoryStore, HistorySummary, OperationRecord, SessionSummary,
    UndoPreflightResult, UndoResult,
};
use crate::wal::{WALEntry, WALJournal, WALManager, WALOperationType, WALStatus};
use chrono::Utc;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

/// Global lock for undo operations - prevents concurrent undos on the same folder
static UNDO_LOCKS: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Protected system paths that should never be modified
const PROTECTED_PATHS: &[&str] = &[
    "/", "/bin", "/sbin", "/usr", "/etc", "/var", "/System",
    "/Library", "/Applications", "/private", "/tmp", "/dev", "/proc", "/sys",
];

/// Validate that a path is safe for undo operations
fn validate_undo_path(path: &str, base_folder: &str) -> Result<PathBuf, String> {
    // Check for path traversal attempts
    if path.contains("..") {
        return Err(format!("Path traversal not allowed in path: {}", path));
    }

    let path_buf = PathBuf::from(path);

    // Try to canonicalize if the path exists, otherwise validate the components
    let canonical = if path_buf.exists() {
        path_buf
            .canonicalize()
            .map_err(|e| format!("Invalid path '{}': {}", path, e))?
    } else {
        // For paths that don't exist yet (e.g., undo destinations), validate components
        let mut validated = PathBuf::new();
        for component in path_buf.components() {
            use std::path::Component;
            match component {
                Component::ParentDir => {
                    return Err(format!("Path traversal not allowed: {}", path));
                }
                Component::Normal(s) => validated.push(s),
                Component::RootDir => validated.push("/"),
                Component::CurDir => {} // Skip .
                Component::Prefix(p) => validated.push(p.as_os_str()),
            }
        }
        validated
    };

    let canonical_str = canonical.to_string_lossy();

    // Check against protected paths
    for protected in PROTECTED_PATHS {
        if canonical_str == *protected {
            return Err(format!("Cannot operate on protected path: {}", path));
        }
        // Allow paths within user home directories
        if canonical_str.starts_with(&format!("{}/", protected)) {
            let is_user_home = canonical_str.starts_with("/Users/") || canonical_str.starts_with("/home/");
            if !is_user_home {
                return Err(format!("Cannot operate on protected system path: {}", path));
            }
        }
    }

    // Verify path is related to the base folder (either within it or a parent)
    let base = PathBuf::from(base_folder);
    if let Ok(base_canonical) = base.canonicalize() {
        let base_str = base_canonical.to_string_lossy();
        // Path should be within base folder OR base folder should be within path
        // (the latter handles undo of operations that moved files out)
        if !canonical_str.starts_with(base_str.as_ref())
            && !base_str.starts_with(canonical_str.as_ref())
        {
            // Also allow sibling directories (same parent)
            let path_parent = canonical.parent().map(|p| p.to_string_lossy().to_string());
            let base_parent = base_canonical.parent().map(|p| p.to_string_lossy().to_string());
            if path_parent != base_parent {
                return Err(format!(
                    "Path '{}' is outside the target folder scope",
                    path
                ));
            }
        }
    }

    Ok(canonical)
}

/// Acquire a lock for undo operations on a folder
fn acquire_undo_lock(folder_path: &str) -> Result<(), String> {
    let mut locks = UNDO_LOCKS
        .lock()
        .map_err(|_| "Failed to acquire undo lock mutex".to_string())?;

    if locks.contains(folder_path) {
        return Err("Another undo operation is already in progress for this folder".to_string());
    }

    locks.insert(folder_path.to_string());
    Ok(())
}

/// Release the undo lock for a folder
fn release_undo_lock(folder_path: &str) {
    if let Ok(mut locks) = UNDO_LOCKS.lock() {
        locks.remove(folder_path);
    }
}

/// RAII guard for undo lock
struct UndoLockGuard {
    folder_path: String,
}

impl UndoLockGuard {
    fn new(folder_path: &str) -> Result<Self, String> {
        acquire_undo_lock(folder_path)?;
        Ok(Self {
            folder_path: folder_path.to_string(),
        })
    }
}

impl Drop for UndoLockGuard {
    fn drop(&mut self) {
        release_undo_lock(&self.folder_path);
    }
}

/// Check if a folder has organization history
#[tauri::command]
pub fn history_has_history(folder_path: String) -> Result<bool, String> {
    let store = HistoryStore::new();
    Ok(store.has_history(&folder_path))
}

/// Get history summary for a folder
#[tauri::command]
pub fn history_get_summary(folder_path: String) -> Result<Option<HistorySummary>, String> {
    let store = HistoryStore::new();
    store.get_summary(&folder_path)
}

/// Get session list for a folder
#[tauri::command]
pub fn history_get_sessions(folder_path: String) -> Result<Vec<SessionSummary>, String> {
    let store = HistoryStore::new();
    store.get_session_summaries(&folder_path)
}

/// Get detailed session information
#[tauri::command]
pub fn history_get_session_detail(
    folder_path: String,
    session_id: String,
) -> Result<Option<HistorySession>, String> {
    let store = HistoryStore::new();
    store.get_session(&folder_path, &session_id)
}

/// Perform preflight check before undo
#[tauri::command]
pub async fn history_undo_preflight(
    folder_path: String,
    target_session_id: String,
) -> Result<UndoPreflightResult, String> {
    // Run in blocking context since it does file I/O
    tokio::task::spawn_blocking(move || preflight_undo(&folder_path, &target_session_id))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

/// Execute undo to a specific session
#[tauri::command]
pub async fn history_undo_execute(
    app_handle: AppHandle,
    folder_path: String,
    target_session_id: String,
    resolution: String,
) -> Result<UndoResult, String> {
    // Acquire lock to prevent concurrent undo operations on the same folder
    let _lock_guard = UndoLockGuard::new(&folder_path)?;

    let resolution = ConflictResolution::from_str(&resolution)
        .ok_or_else(|| format!("Invalid resolution: {}", resolution))?;

    // Collect undo operations
    let undo_ops = collect_undo_operations(&folder_path, &target_session_id)?;

    if undo_ops.is_empty() {
        return Ok(UndoResult {
            success: true,
            operations_undone: 0,
            operations_skipped: 0,
            errors: vec![],
        });
    }

    // Convert to WAL operations with path validation
    let mut wal_ops: Vec<WALOperationType> = Vec::new();
    for op in &undo_ops {
        match operation_record_to_wal(op, &folder_path) {
            Ok(wal_op) => wal_ops.push(wal_op),
            Err(e) => {
                tracing::warn!("Skipping invalid undo operation: {}", e);
                // Skip operations with invalid paths
            }
        }
    }

    // Create a new WAL journal for the undo
    let job_id = format!("undo-{}", Utc::now().timestamp_millis());
    let mut journal = WALJournal::new(job_id.clone(), PathBuf::from(&folder_path));

    for (i, wal_op) in wal_ops.iter().enumerate() {
        let undo_op = wal_op
            .inverse()
            .map_err(|e| format!("Failed to compute inverse operation: {}", e))?;

        let entry = WALEntry {
            id: Uuid::new_v4(),
            sequence: i as u32,
            operation: wal_op.clone(),
            undo_operation: undo_op,
            status: WALStatus::Pending,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            error: None,
            depends_on: vec![],
        };
        journal.entries.push(entry);
    }

    // Save the journal
    let wal_manager = WALManager::new();
    wal_manager.save_journal(&journal)?;

    // Execute operations
    let mut operations_undone = 0;
    let mut operations_skipped = 0;
    let mut errors = Vec::new();
    let total_ops = journal.entries.len();

    for i in 0..total_ops {
        // Mark as in progress
        journal.entries[i].status = WALStatus::InProgress;
        journal.entries[i].updated_at = Utc::now();
        wal_manager.save_journal(&journal)?;

        // Clone the operation to execute (avoid borrow issues)
        let op_to_execute = journal.entries[i].operation.clone();

        // Execute the operation
        match execute_wal_operation(&op_to_execute) {
            Ok(()) => {
                journal.entries[i].status = WALStatus::Complete;
                operations_undone += 1;

                // Emit progress event
                let _ = app_handle.emit(
                    "undo-progress",
                    serde_json::json!({
                        "completed": operations_undone,
                        "total": total_ops,
                    }),
                );
            }
            Err(e) => {
                match resolution {
                    ConflictResolution::Abort => {
                        // Stop immediately
                        journal.entries[i].status = WALStatus::Failed;
                        journal.entries[i].error = Some(e.clone());
                        journal.entries[i].updated_at = Utc::now();
                        errors.push(e);
                        wal_manager.save_journal(&journal)?;
                        break;
                    }
                    ConflictResolution::Skip => {
                        journal.entries[i].status = WALStatus::Failed;
                        journal.entries[i].error = Some(e.clone());
                        errors.push(e);
                        operations_skipped += 1;
                        // Continue to next operation
                    }
                    ConflictResolution::Force => {
                        // Try to force the operation (delete blocking files first)
                        if let Err(force_err) = execute_wal_operation_forced(&op_to_execute) {
                            journal.entries[i].status = WALStatus::Failed;
                            journal.entries[i].error = Some(force_err.clone());
                            errors.push(force_err);
                            operations_skipped += 1;
                        } else {
                            journal.entries[i].status = WALStatus::Complete;
                            operations_undone += 1;
                            let _ = app_handle.emit(
                                "undo-progress",
                                serde_json::json!({
                                    "completed": operations_undone,
                                    "total": total_ops,
                                }),
                            );
                        }
                    }
                    ConflictResolution::Backup => {
                        // Create backup before forcing
                        if let Err(backup_err) = create_backup_for_operation(&op_to_execute) {
                            journal.entries[i].status = WALStatus::Failed;
                            journal.entries[i].error = Some(format!("Backup failed: {}", backup_err));
                            errors.push(format!("Backup failed: {}", backup_err));
                            operations_skipped += 1;
                        } else if let Err(force_err) = execute_wal_operation_forced(&op_to_execute) {
                            journal.entries[i].status = WALStatus::Failed;
                            journal.entries[i].error = Some(force_err.clone());
                            errors.push(force_err);
                            operations_skipped += 1;
                        } else {
                            journal.entries[i].status = WALStatus::Complete;
                            operations_undone += 1;
                            let _ = app_handle.emit(
                                "undo-progress",
                                serde_json::json!({
                                    "completed": operations_undone,
                                    "total": total_ops,
                                }),
                            );
                        }
                    }
                }
            }
        }

        journal.entries[i].updated_at = Utc::now();
        wal_manager.save_journal(&journal)?;
    }

    // Mark sessions as undone in history
    if operations_undone > 0 {
        let store = HistoryStore::new();
        store.mark_sessions_undone(&folder_path, &target_session_id)?;
    }

    // Clean up journal on success
    if errors.is_empty() {
        wal_manager.discard_journal(&job_id)?;
    }

    Ok(UndoResult {
        success: errors.is_empty(),
        operations_undone,
        operations_skipped,
        errors,
    })
}

/// Delete history for a folder
#[tauri::command]
pub fn history_delete(folder_path: String) -> Result<(), String> {
    let store = HistoryStore::new();
    store.delete_history(&folder_path)
}

/// List all folders with history
#[tauri::command]
pub fn history_list_folders() -> Result<Vec<FolderIndexEntry>, String> {
    let store = HistoryStore::new();
    store.list_folders()
}

/// Convert OperationRecord to WALOperationType with path validation
fn operation_record_to_wal(
    record: &OperationRecord,
    base_folder: &str,
) -> Result<WALOperationType, String> {
    match record {
        OperationRecord::CreateFolder { path } => {
            let validated = validate_undo_path(path, base_folder)?;
            Ok(WALOperationType::CreateFolder { path: validated })
        }
        OperationRecord::Move {
            source,
            destination,
        } => {
            let validated_source = validate_undo_path(source, base_folder)?;
            let validated_dest = validate_undo_path(destination, base_folder)?;
            Ok(WALOperationType::Move {
                source: validated_source,
                destination: validated_dest,
            })
        }
        OperationRecord::Rename { path, new_name } => {
            let validated = validate_undo_path(path, base_folder)?;
            // Validate new_name doesn't contain path separators
            if new_name.contains('/') || new_name.contains('\\') {
                return Err(format!("Invalid new name contains path separator: {}", new_name));
            }
            Ok(WALOperationType::Rename {
                path: validated,
                new_name: new_name.clone(),
            })
        }
        OperationRecord::Quarantine {
            path,
            quarantine_path,
        } => {
            let validated_path = validate_undo_path(path, base_folder)?;
            // Quarantine path is in a system directory, so we just validate it's not traversing
            if quarantine_path.contains("..") {
                return Err("Quarantine path contains traversal".to_string());
            }
            Ok(WALOperationType::Quarantine {
                path: validated_path,
                quarantine_path: PathBuf::from(quarantine_path),
            })
        }
        OperationRecord::Copy {
            source,
            destination,
        } => {
            let validated_source = validate_undo_path(source, base_folder)?;
            let validated_dest = validate_undo_path(destination, base_folder)?;
            Ok(WALOperationType::Copy {
                source: validated_source,
                destination: validated_dest,
            })
        }
        OperationRecord::DeleteFolder { path } => {
            let validated = validate_undo_path(path, base_folder)?;
            Ok(WALOperationType::DeleteFolder { path: validated })
        }
    }
}

/// Execute a single WAL operation
fn execute_wal_operation(op: &WALOperationType) -> Result<(), String> {
    match op {
        WALOperationType::CreateFolder { path } => {
            std::fs::create_dir_all(path)
                .map_err(|e| format!("Failed to create folder {}: {}", path.display(), e))
        }
        WALOperationType::Move {
            source,
            destination,
        } => {
            // Ensure parent directory exists
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create parent directory: {}", e)
                })?;
            }

            std::fs::rename(source, destination).map_err(|e| {
                format!(
                    "Failed to move {} to {}: {}",
                    source.display(),
                    destination.display(),
                    e
                )
            })
        }
        WALOperationType::Rename { path, new_name } => {
            let parent = path.parent().unwrap_or(std::path::Path::new(""));
            let new_path = parent.join(new_name);

            std::fs::rename(path, &new_path).map_err(|e| {
                format!(
                    "Failed to rename {} to {}: {}",
                    path.display(),
                    new_path.display(),
                    e
                )
            })
        }
        WALOperationType::DeleteFolder { path } => {
            if path.is_dir() {
                // Only delete if empty
                if std::fs::read_dir(path)
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false)
                {
                    std::fs::remove_dir(path)
                        .map_err(|e| format!("Failed to delete folder: {}", e))
                } else {
                    Err(format!("Folder {} is not empty", path.display()))
                }
            } else {
                // It's a file, use trash
                trash::delete(path)
                    .map_err(|e| format!("Failed to delete {}: {}", path.display(), e))
            }
        }
        WALOperationType::Copy {
            source: _,
            destination,
        } => {
            // Undo copy = delete destination
            if destination.exists() {
                trash::delete(destination)
                    .map_err(|e| format!("Failed to delete copy: {}", e))
            } else {
                Ok(()) // Already gone
            }
        }
        WALOperationType::Quarantine {
            path: _,
            quarantine_path: _,
        } => {
            // Quarantine undo is handled as a Move in the undo_operation
            // This branch shouldn't be hit directly
            Err("Quarantine undo should be a Move operation".to_string())
        }
    }
}

/// Create a backup of files that would be affected by the operation
fn create_backup_for_operation(op: &WALOperationType) -> Result<(), String> {
    let backup_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("sentinel")
        .join("undo_backups")
        .join(Utc::now().format("%Y%m%d_%H%M%S%.3f").to_string());

    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    match op {
        WALOperationType::Move { destination, .. } => {
            // Backup the file at destination (will be overwritten)
            if destination.exists() {
                let file_name = destination
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let backup_path = backup_dir.join(file_name.as_ref());

                if destination.is_dir() {
                    copy_dir_recursive(destination, &backup_path)?;
                } else {
                    std::fs::copy(destination, &backup_path)
                        .map_err(|e| format!("Failed to backup {}: {}", destination.display(), e))?;
                }

                tracing::info!("Created backup at: {}", backup_path.display());
            }
        }
        WALOperationType::Rename { path, new_name } => {
            // Backup the renamed file
            let parent = path.parent().unwrap_or(std::path::Path::new(""));
            let current_path = parent.join(new_name);

            if current_path.exists() {
                let backup_path = backup_dir.join(new_name);

                if current_path.is_dir() {
                    copy_dir_recursive(&current_path, &backup_path)?;
                } else {
                    std::fs::copy(&current_path, &backup_path)
                        .map_err(|e| format!("Failed to backup {}: {}", current_path.display(), e))?;
                }

                tracing::info!("Created backup at: {}", backup_path.display());
            }
        }
        WALOperationType::DeleteFolder { path } | WALOperationType::CreateFolder { path } => {
            // For delete/create folder operations, backup the whole folder if it exists
            if path.exists() && path.is_dir() {
                let folder_name = path.file_name().unwrap_or_default().to_string_lossy();
                let backup_path = backup_dir.join(folder_name.as_ref());
                copy_dir_recursive(path, &backup_path)?;
                tracing::info!("Created backup at: {}", backup_path.display());
            }
        }
        _ => {
            // Other operations don't need backups
        }
    }

    Ok(())
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create directory {}: {}", dst.display(), e))?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("Failed to read directory {}: {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Failed to copy {}: {}", src_path.display(), e))?;
        }
    }

    Ok(())
}

/// Execute a WAL operation with force - removes blocking files first
fn execute_wal_operation_forced(op: &WALOperationType) -> Result<(), String> {
    match op {
        WALOperationType::Move {
            source,
            destination,
        } => {
            // Remove blocking file at destination if it exists
            if destination.exists() {
                if destination.is_dir() {
                    std::fs::remove_dir_all(destination)
                        .map_err(|e| format!("Failed to remove blocking directory: {}", e))?;
                } else {
                    std::fs::remove_file(destination)
                        .map_err(|e| format!("Failed to remove blocking file: {}", e))?;
                }
            }

            // Now execute the move
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create parent directory: {}", e)
                })?;
            }

            std::fs::rename(source, destination).map_err(|e| {
                format!(
                    "Failed to move {} to {}: {}",
                    source.display(),
                    destination.display(),
                    e
                )
            })
        }
        WALOperationType::Rename { path, new_name } => {
            let parent = path.parent().unwrap_or(std::path::Path::new(""));
            let new_path = parent.join(new_name);

            // Remove blocking file at the old path if it exists
            if path.exists() {
                if path.is_dir() {
                    std::fs::remove_dir_all(path)
                        .map_err(|e| format!("Failed to remove blocking directory: {}", e))?;
                } else {
                    std::fs::remove_file(path)
                        .map_err(|e| format!("Failed to remove blocking file: {}", e))?;
                }
            }

            std::fs::rename(&new_path, path).map_err(|e| {
                format!(
                    "Failed to rename {} to {}: {}",
                    new_path.display(),
                    path.display(),
                    e
                )
            })
        }
        // For other operations, just try the normal execution
        _ => execute_wal_operation(op),
    }
}
