use crate::execution::{
    ConflictPolicy, ExecutionConfig, ExecutionEngine, ExecutionResult, ProgressCallback,
    StateSnapshot, StateValidator, ValidationResult,
};
use crate::jobs::{JobManager, JobStatus, OrganizeJob, OrganizeOperation, OrganizePlan};
use crate::security::PathValidator;
use crate::wal::entry::{WALJournal, WALOperationType};
use crate::wal::journal::WALManager;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Start a new organize job
#[tauri::command]
pub fn start_organize_job(target_folder: String) -> Result<OrganizeJob, String> {
    // Security: Validate the target folder path
    let target_path = PathBuf::from(&target_folder);
    let validated_path = PathValidator::validate_for_read(&target_path, None)
        .map_err(|e| format!("Invalid target folder: {}", e))?;

    if !validated_path.is_dir() {
        return Err(format!("Target is not a directory: {}", target_folder));
    }

    let job = OrganizeJob::new(&target_folder);
    JobManager::save_job(&job)?;
    Ok(job)
}

/// Update job with the generated plan
#[tauri::command]
pub fn set_job_plan(
    job_id: String,
    plan_id: String,
    description: String,
    operations: Vec<serde_json::Value>,
    target_folder: String,
) -> Result<OrganizeJob, String> {
    let mut job = JobManager::load_job()?
        .ok_or_else(|| format!("Job not found: {}", job_id))?;

    if job.job_id != job_id {
        return Err(format!("Job ID mismatch: expected {}, got {}", job.job_id, job_id));
    }

    // Convert operations from JSON with validation
    let mut ops: Vec<OrganizeOperation> = Vec::with_capacity(operations.len());
    for (idx, op) in operations.into_iter().enumerate() {
        let op_id = op
            .get("opId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("Operation {} missing required field 'opId'", idx))?
            .to_string();

        let op_type = op
            .get("type")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("Operation {} missing required field 'type'", idx))?
            .to_string();

        // Validate operation-specific required fields
        match op_type.as_str() {
            "move" => {
                if op.get("source").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).is_none() {
                    return Err(format!("Operation '{}' (move) missing required field 'source'", op_id));
                }
                if op.get("destination").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).is_none() {
                    return Err(format!("Operation '{}' (move) missing required field 'destination'", op_id));
                }
            }
            "create_folder" | "trash" | "quarantine" => {
                if op.get("path").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).is_none() {
                    return Err(format!("Operation '{}' ({}) missing required field 'path'", op_id, op_type));
                }
            }
            "rename" => {
                if op.get("path").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).is_none() {
                    return Err(format!("Operation '{}' (rename) missing required field 'path'", op_id));
                }
                if op.get("newName").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).is_none() {
                    return Err(format!("Operation '{}' (rename) missing required field 'newName'", op_id));
                }
            }
            _ => {
                return Err(format!("Operation '{}' has unknown type '{}'", op_id, op_type));
            }
        }

        ops.push(OrganizeOperation {
            op_id,
            op_type,
            source: op.get("source").and_then(|v| v.as_str()).map(String::from),
            destination: op.get("destination").and_then(|v| v.as_str()).map(String::from),
            path: op.get("path").and_then(|v| v.as_str()).map(String::from),
            new_name: op.get("newName").and_then(|v| v.as_str()).map(String::from),
        });
    }

    let plan = OrganizePlan {
        plan_id,
        description,
        operations: ops,
        target_folder,
    };

    job.set_plan(plan);
    JobManager::save_job(&job)?;
    Ok(job)
}

/// Mark an operation as completed
#[tauri::command]
pub fn complete_job_operation(job_id: String, op_id: String, current_index: i32) -> Result<OrganizeJob, String> {
    let mut job = JobManager::load_job()?
        .ok_or_else(|| format!("Job not found: {}", job_id))?;

    if job.job_id != job_id {
        return Err(format!("Job ID mismatch"));
    }

    job.complete_operation(&op_id);
    job.set_current_op(current_index);
    JobManager::save_job(&job)?;
    Ok(job)
}

/// Mark job as completed
#[tauri::command]
pub fn complete_organize_job(job_id: String) -> Result<(), String> {
    let mut job = JobManager::load_job()?
        .ok_or_else(|| format!("Job not found: {}", job_id))?;

    if job.job_id != job_id {
        return Err(format!("Job ID mismatch"));
    }

    job.mark_completed();
    JobManager::save_job(&job)?;

    // Clear the job file after a short delay (let frontend read final state)
    // In production, you might want to keep history
    Ok(())
}

/// Mark job as failed
#[tauri::command]
pub fn fail_organize_job(job_id: String, error: String) -> Result<(), String> {
    let mut job = JobManager::load_job()?
        .ok_or_else(|| format!("Job not found: {}", job_id))?;

    if job.job_id != job_id {
        return Err(format!("Job ID mismatch"));
    }

    job.mark_failed(&error);
    JobManager::save_job(&job)?;
    Ok(())
}

/// Check for interrupted jobs on app startup
#[tauri::command]
pub fn check_interrupted_job() -> Result<Option<OrganizeJob>, String> {
    JobManager::check_for_interrupted_job()
}

/// Get current job status
#[tauri::command]
pub fn get_current_job() -> Result<Option<OrganizeJob>, String> {
    JobManager::load_job()
}

/// Clear the current job (dismiss interrupted job or cleanup)
#[tauri::command]
pub fn clear_organize_job() -> Result<(), String> {
    JobManager::clear_job()
}

/// Resume an interrupted job (returns the job with remaining operations)
#[tauri::command]
pub fn resume_organize_job(job_id: String) -> Result<OrganizeJob, String> {
    let mut job = JobManager::load_job()?
        .ok_or_else(|| format!("Job not found: {}", job_id))?;

    if job.job_id != job_id {
        return Err(format!("Job ID mismatch"));
    }

    if job.status != JobStatus::Interrupted {
        return Err("Job is not in interrupted state".to_string());
    }

    // Mark as running again
    job.status = JobStatus::Running;
    JobManager::save_job(&job)?;

    Ok(job)
}

/// Execute an organize plan using parallel DAG-based execution
///
/// V5: Now emits 'execution-progress' events for clean UI updates.
/// V6: Now accepts conflict_policy for handling destination conflicts.
/// V7: Now accepts original_folder for post-execution cleanup of empty directories.
/// V8: Now validates filesystem state before execution to detect concurrent modifications.
///
/// This command:
/// 1. Validates that source files haven't changed since plan creation
/// 2. Converts the OrganizePlan to WAL entries
/// 3. Builds a dependency DAG for parallel execution
/// 4. Executes operations in parallel within each level
/// 5. Handles destination conflicts according to policy (skip, auto_rename, fail)
/// 6. Emits progress events after each level completes
/// 7. Cleans up empty directories in the original folder after successful execution
/// 8. Returns the execution result
#[tauri::command]
pub async fn execute_plan_parallel(
    app_handle: AppHandle,
    plan: OrganizePlan,
    conflict_policy: Option<String>,
    original_folder: Option<String>,
    state_snapshot: Option<serde_json::Value>,
) -> Result<ExecutionResult, String> {
    // V8: Validate filesystem state if snapshot was provided
    if let Some(snapshot_json) = state_snapshot {
        let snapshot: StateSnapshot = serde_json::from_value(snapshot_json)
            .map_err(|e| format!("Invalid state snapshot: {}", e))?;

        let validator = StateValidator::new(snapshot);
        let validation = validator.validate_current_state()?;

        if !validation.valid {
            // Emit state conflict event so frontend can show warning
            let _ = app_handle.emit("execution-state-conflict", &validation);

            // If there are critical conflicts (deleted files), abort
            if validation.critical_count > 0 {
                let critical_paths: Vec<&str> = validation
                    .conflicts
                    .iter()
                    .filter(|c| c.is_critical())
                    .map(|c| match c {
                        crate::execution::StateConflict::Deleted { path } => path.as_str(),
                        _ => "",
                    })
                    .filter(|s| !s.is_empty())
                    .collect();

                return Err(format!(
                    "Execution aborted: {} source file(s) were deleted since planning: {}",
                    validation.critical_count,
                    critical_paths.join(", ")
                ));
            }

            // Non-critical conflicts (modifications) - log warning but continue
            tracing::warn!(
                conflicts = validation.warning_count,
                "Proceeding with execution despite {} file modification(s) detected",
                validation.warning_count
            );
        }
    }

    // Parse conflict policy (default to AutoRename for better UX)
    let policy = match conflict_policy.as_deref() {
        Some("skip") => ConflictPolicy::Skip,
        Some("fail") => ConflictPolicy::Fail,
        Some("auto_rename") | None => ConflictPolicy::AutoRename, // Default to auto-rename
        Some(other) => {
            tracing::warn!(policy = %other, "Unknown conflict policy, using auto_rename");
            ConflictPolicy::AutoRename
        }
    };

    let config = ExecutionConfig {
        on_destination_exists: policy,
    };
    tracing::info!(
        operations = plan.operations.len(),
        "Starting parallel plan execution"
    );

    // Create a WAL journal from the plan
    let target_folder = PathBuf::from(&plan.target_folder);
    let mut journal = WALJournal::new(plan.plan_id.clone(), target_folder.clone());

    // Track folder creation operations for dependencies
    let mut folder_op_ids: std::collections::HashMap<String, uuid::Uuid> =
        std::collections::HashMap::new();

    // Convert operations to WAL entries with dependencies
    for op in &plan.operations {
        let wal_op = match op.op_type.as_str() {
            "create_folder" => {
                let path = op.path.as_ref().ok_or_else(|| {
                    format!("Operation '{}' (create_folder) missing required field 'path'", op.op_id)
                })?;
                WALOperationType::CreateFolder {
                    path: PathBuf::from(path),
                }
            }
            "move" => {
                let src = op.source.as_ref().ok_or_else(|| {
                    format!("Operation '{}' (move) missing required field 'source'", op.op_id)
                })?;
                let dst = op.destination.as_ref().ok_or_else(|| {
                    format!("Operation '{}' (move) missing required field 'destination'", op.op_id)
                })?;
                WALOperationType::Move {
                    source: PathBuf::from(src),
                    destination: PathBuf::from(dst),
                }
            }
            "rename" => {
                let path = op.path.as_ref().ok_or_else(|| {
                    format!("Operation '{}' (rename) missing required field 'path'", op.op_id)
                })?;
                let new_name = op.new_name.as_ref().ok_or_else(|| {
                    format!("Operation '{}' (rename) missing required field 'newName'", op.op_id)
                })?;
                WALOperationType::Rename {
                    path: PathBuf::from(path),
                    new_name: new_name.clone(),
                }
            }
            "trash" | "quarantine" => {
                let path = op.path.as_ref().ok_or_else(|| {
                    format!("Operation '{}' ({}) missing required field 'path'", op.op_id, op.op_type)
                })?;
                let quarantine_path = dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("/tmp"))
                    .join("sentinel")
                    .join("quarantine")
                    .join(&op.op_id);
                WALOperationType::Quarantine {
                    path: PathBuf::from(path),
                    quarantine_path,
                }
            }
            unknown_type => {
                return Err(format!("Operation '{}' has unknown type '{}'", op.op_id, unknown_type));
            }
        };

        // Check for dependencies - moves depend on their destination folder being created
        let mut depends_on = Vec::new();
        if let WALOperationType::Move { destination, .. } = &wal_op {
            if let Some(parent) = destination.parent() {
                let parent_str = parent.to_string_lossy().to_string();
                if let Some(&folder_op_id) = folder_op_ids.get(&parent_str) {
                    depends_on.push(folder_op_id);
                }
            }
        }

        // Track folder creation for dependency resolution
        if let WALOperationType::CreateFolder { ref path } = wal_op {
            let path_str = path.to_string_lossy().to_string();
            let op_id = if depends_on.is_empty() {
                journal.add_operation(wal_op)
                    .map_err(|e| format!("Failed to add operation: {}", e))?
            } else {
                journal.add_operation_with_deps(wal_op, depends_on)
                    .map_err(|e| format!("Failed to add operation: {}", e))?
            };
            folder_op_ids.insert(path_str, op_id);
        } else if depends_on.is_empty() {
            journal.add_operation(wal_op)
                .map_err(|e| format!("Failed to add operation: {}", e))?;
        } else {
            journal.add_operation_with_deps(wal_op, depends_on)
                .map_err(|e| format!("Failed to add operation: {}", e))?;
        }
    }

    tracing::debug!(
        entries = journal.entries.len(),
        "Created WAL journal"
    );

    // Save the journal
    let wal_manager = WALManager::new();
    wal_manager
        .save_journal(&journal)
        .map_err(|e| format!("Failed to save WAL journal: {}", e.message))?;

    // V5: Create progress callback that emits Tauri events
    let app_handle_clone = app_handle.clone();
    let progress_callback: Arc<ProgressCallback> = Arc::new(Box::new(move |completed, total| {
        let _ = app_handle_clone.emit(
            "execution-progress",
            serde_json::json!({
                "completed": completed,
                "total": total
            }),
        );
        tracing::debug!(completed = completed, total = total, "Execution progress");
    }));

    // Execute using the parallel DAG executor with progress callback, conflict config, and events
    let engine = ExecutionEngine::new();
    let result = engine
        .execute_journal_with_config_and_events(
            &plan.plan_id,
            Some(progress_callback),
            config,
            Some(app_handle.clone()),
        )
        .await?;

    tracing::info!(
        completed = result.completed_count,
        failed = result.failed_count,
        skipped = result.skipped_count,
        renamed = result.renamed_count,
        "Plan execution complete"
    );

    // Clean up the journal if all succeeded
    if result.success {
        let _ = wal_manager.discard_journal(&plan.plan_id);

        // V7: Clean up empty directories in the original folder
        if let Some(ref original) = original_folder {
            let original_path = PathBuf::from(original);
            if original_path.exists() && original_path.is_dir() {
                match cleanup_empty_directories(&original_path) {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!(
                                deleted = count,
                                folder = %original,
                                "Cleaned up empty directories after organization"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            folder = %original,
                            "Failed to clean up some empty directories"
                        );
                    }
                }
            }
        }
    }

    Ok(result)
}

/// Capture a state snapshot for the given source paths
///
/// This should be called when the plan is generated/displayed to the user.
/// The snapshot can later be passed to execute_plan_parallel to validate
/// that files haven't changed between planning and execution.
#[tauri::command]
pub fn capture_state_snapshot(source_paths: Vec<String>) -> Result<StateSnapshot, String> {
    let paths: Vec<PathBuf> = source_paths.into_iter().map(PathBuf::from).collect();
    StateSnapshot::capture(&paths)
}

/// Validate current state against a previously captured snapshot
///
/// Useful for checking if files have changed before starting execution.
#[tauri::command]
pub fn validate_state_snapshot(snapshot: serde_json::Value) -> Result<ValidationResult, String> {
    let snapshot: StateSnapshot = serde_json::from_value(snapshot)
        .map_err(|e| format!("Invalid state snapshot: {}", e))?;

    let validator = StateValidator::new(snapshot);
    validator.validate_current_state()
}

/// Recursively delete empty directories starting from the given path.
/// Returns the number of directories deleted.
///
/// This function walks the directory tree depth-first and removes
/// directories that are empty (or become empty after their children are removed).
fn cleanup_empty_directories(path: &std::path::Path) -> Result<usize, String> {
    let mut deleted_count = 0;

    if !path.is_dir() {
        return Ok(0);
    }

    // First, recursively clean up all subdirectories
    let entries: Vec<_> = std::fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory {}: {}", path.display(), e))?
        .filter_map(|e| e.ok())
        .collect();

    for entry in &entries {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            deleted_count += cleanup_empty_directories(&entry_path)?;
        }
    }

    // Re-check if directory is now empty (after cleaning subdirectories)
    let is_empty = std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false);

    if is_empty {
        // Don't delete if it's a protected path
        if PathValidator::is_protected_path(path) {
            tracing::debug!(path = %path.display(), "Skipping protected empty directory");
            return Ok(deleted_count);
        }

        match std::fs::remove_dir(path) {
            Ok(()) => {
                tracing::debug!(path = %path.display(), "Deleted empty directory");
                deleted_count += 1;
            }
            Err(e) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %e,
                    "Could not delete directory (may not be empty or permission denied)"
                );
            }
        }
    }

    Ok(deleted_count)
}
