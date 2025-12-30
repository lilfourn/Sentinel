use crate::execution::{ConflictPolicy, ExecutionConfig, ExecutionEngine, ExecutionResult, ProgressCallback};
use crate::jobs::{JobManager, JobStatus, OrganizeJob, OrganizeOperation, OrganizePlan};
use crate::wal::entry::{WALJournal, WALOperationType};
use crate::wal::journal::WALManager;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Start a new organize job
#[tauri::command]
pub fn start_organize_job(target_folder: String) -> Result<OrganizeJob, String> {
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

    // Convert operations from JSON
    let ops: Vec<OrganizeOperation> = operations
        .into_iter()
        .map(|op| OrganizeOperation {
            op_id: op.get("opId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            op_type: op.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            source: op.get("source").and_then(|v| v.as_str()).map(String::from),
            destination: op.get("destination").and_then(|v| v.as_str()).map(String::from),
            path: op.get("path").and_then(|v| v.as_str()).map(String::from),
            new_name: op.get("newName").and_then(|v| v.as_str()).map(String::from),
        })
        .collect();

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
///
/// This command:
/// 1. Converts the OrganizePlan to WAL entries
/// 2. Builds a dependency DAG for parallel execution
/// 3. Executes operations in parallel within each level
/// 4. Handles destination conflicts according to policy (skip, auto_rename, fail)
/// 5. Emits progress events after each level completes
/// 6. Returns the execution result
#[tauri::command]
pub async fn execute_plan_parallel(
    app_handle: AppHandle,
    plan: OrganizePlan,
    conflict_policy: Option<String>,
) -> Result<ExecutionResult, String> {
    // Parse conflict policy (default to AutoRename for better UX)
    let policy = match conflict_policy.as_deref() {
        Some("skip") => ConflictPolicy::Skip,
        Some("fail") => ConflictPolicy::Fail,
        Some("auto_rename") | None => ConflictPolicy::AutoRename, // Default to auto-rename
        Some(other) => {
            eprintln!("[ExecutePlan] Unknown conflict policy '{}', using auto_rename", other);
            ConflictPolicy::AutoRename
        }
    };

    let config = ExecutionConfig {
        on_destination_exists: policy,
    };
    eprintln!(
        "[ExecutePlan] Starting parallel execution of {} operations",
        plan.operations.len()
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
                if let Some(ref path) = op.path {
                    WALOperationType::CreateFolder {
                        path: PathBuf::from(path),
                    }
                } else {
                    continue;
                }
            }
            "move" => {
                if let (Some(ref src), Some(ref dst)) = (&op.source, &op.destination) {
                    WALOperationType::Move {
                        source: PathBuf::from(src),
                        destination: PathBuf::from(dst),
                    }
                } else {
                    continue;
                }
            }
            "rename" => {
                if let (Some(ref path), Some(ref new_name)) = (&op.path, &op.new_name) {
                    WALOperationType::Rename {
                        path: PathBuf::from(path),
                        new_name: new_name.clone(),
                    }
                } else {
                    continue;
                }
            }
            "trash" | "quarantine" => {
                if let Some(ref path) = op.path {
                    let quarantine_path = dirs::data_dir()
                        .unwrap_or_else(|| PathBuf::from("/tmp"))
                        .join("sentinel")
                        .join("quarantine")
                        .join(&op.op_id);
                    WALOperationType::Quarantine {
                        path: PathBuf::from(path),
                        quarantine_path,
                    }
                } else {
                    continue;
                }
            }
            _ => continue,
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
            } else {
                journal.add_operation_with_deps(wal_op, depends_on)
            };
            folder_op_ids.insert(path_str, op_id);
        } else if depends_on.is_empty() {
            journal.add_operation(wal_op);
        } else {
            journal.add_operation_with_deps(wal_op, depends_on);
        }
    }

    eprintln!(
        "[ExecutePlan] Created WAL journal with {} entries",
        journal.entries.len()
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
        eprintln!("[ExecutePlan] Progress: {}/{}", completed, total);
    }));

    // Execute using the parallel DAG executor with progress callback and conflict config
    let engine = ExecutionEngine::new();
    let result = engine
        .execute_journal_with_config(&plan.plan_id, Some(progress_callback), config)
        .await?;

    eprintln!(
        "[ExecutePlan] Execution complete: {} completed, {} failed, {} skipped, {} renamed",
        result.completed_count, result.failed_count, result.skipped_count, result.renamed_count
    );

    // Clean up the journal if all succeeded
    if result.success {
        let _ = wal_manager.discard_journal(&plan.plan_id);
    }

    Ok(result)
}
