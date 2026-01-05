//! VFS Tauri Commands
//!
//! Exposes VFS functionality to the frontend via Tauri commands.
//! Uses State<VFSState> for thread-safe access to the shadow VFS.

use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

use crate::jobs::OrganizePlan;
use crate::quarantine::{CleanupStats, QuarantineManager, QuarantinedItem};
use crate::vfs::{FileNode, JWalkScanner, ScanStats, ShadowVFS, SimulatedOperation};

/// Thread-safe VFS state managed by Tauri
pub type VFSState = Arc<RwLock<Option<ShadowVFS>>>;

/// Create a new VFS state handle for Tauri
pub fn create_vfs_state() -> VFSState {
    Arc::new(RwLock::new(None))
}

/// Quarantine state managed by Tauri
pub type QuarantineState = Arc<RwLock<QuarantineManager>>;

/// Create a new quarantine state handle for Tauri
pub fn create_quarantine_state() -> Result<QuarantineState, String> {
    let manager = QuarantineManager::new()?;
    Ok(Arc::new(RwLock::new(manager)))
}

// ============================================================================
// VFS Commands
// ============================================================================

/// Scan a folder and populate the VFS
///
/// This creates a new VFS rooted at the specified folder and scans
/// all files and directories within it.
#[tauri::command]
pub async fn scan_folder_vfs(
    folder_path: String,
    vfs_state: State<'_, VFSState>,
) -> Result<ScanStats, String> {
    let path = PathBuf::from(&folder_path);

    if !path.exists() {
        return Err(format!("Path does not exist: {}", folder_path));
    }

    if !path.is_dir() {
        return Err(format!("Path is not a directory: {}", folder_path));
    }

    eprintln!("[VFS] Scanning folder: {}", folder_path);

    let scanner = JWalkScanner::new();
    let mut vfs = ShadowVFS::new(path.clone());

    let stats = scanner.scan(&path, &mut vfs).await?;

    // Store the VFS
    let mut state = vfs_state.write().await;
    *state = Some(vfs);

    eprintln!(
        "[VFS] Scan complete: {} files, {} dirs, {} bytes",
        stats.total_files, stats.total_dirs, stats.total_size_bytes
    );

    Ok(stats)
}

/// List directory contents from the VFS
///
/// Returns the children of a directory in the current VFS.
#[tauri::command]
pub async fn vfs_list_dir(
    path: String,
    vfs_state: State<'_, VFSState>,
) -> Result<Vec<FileNode>, String> {
    let state = vfs_state.read().await;
    let vfs = state
        .as_ref()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    let path_buf = PathBuf::from(&path);
    let children = vfs
        .list_dir(&path_buf)
        .map_err(|e| e.to_string())?;

    // Clone the nodes since we can't return references
    Ok(children.into_iter().cloned().collect())
}

/// Search VFS content
///
/// Searches both file names and content previews for the query.
#[tauri::command]
pub async fn vfs_search_content(
    query: String,
    vfs_state: State<'_, VFSState>,
) -> Result<Vec<FileNode>, String> {
    let state = vfs_state.read().await;
    let vfs = state
        .as_ref()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    let results = vfs.search_content(&query);
    Ok(results.into_iter().cloned().collect())
}

/// Get a specific node from the VFS
#[tauri::command]
pub async fn vfs_get_node(
    path: String,
    vfs_state: State<'_, VFSState>,
) -> Result<Option<FileNode>, String> {
    let state = vfs_state.read().await;
    let vfs = state
        .as_ref()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    let path_buf = PathBuf::from(&path);
    Ok(vfs.get(&path_buf).cloned())
}

/// Get VFS statistics
#[tauri::command]
pub async fn vfs_get_stats(
    vfs_state: State<'_, VFSState>,
) -> Result<crate::vfs::VFSStats, String> {
    let state = vfs_state.read().await;
    let vfs = state
        .as_ref()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    Ok(vfs.stats())
}

/// Result of VFS plan validation
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VfsValidationResult {
    /// Whether the plan is valid (no errors)
    pub valid: bool,
    /// List of validation errors
    pub errors: Vec<String>,
    /// Hash of the plan for sync validation
    pub plan_hash: String,
    /// Plan ID for reference
    pub plan_id: String,
}

/// Validate an organize plan on the VFS
///
/// This is the enhanced validation command that returns structured results
/// including a plan hash for sync validation between frontend and backend.
#[tauri::command]
pub async fn vfs_validate_plan(
    plan: OrganizePlan,
    vfs_state: State<'_, VFSState>,
) -> Result<VfsValidationResult, String> {
    let mut state = vfs_state.write().await;
    let vfs = state
        .as_mut()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    // Compute plan hash for sync validation
    let plan_hash = plan.compute_hash();
    let plan_id = plan.plan_id.clone();

    let operations: Vec<SimulatedOperation> = plan
        .operations
        .iter()
        .filter_map(|op| {
            match op.op_type.as_str() {
                "move" => {
                    let src = op.source.as_ref()?;
                    let dest = op.destination.as_ref()?;
                    Some(SimulatedOperation::Move {
                        source: src.clone(),
                        destination: dest.clone(),
                    })
                }
                "create_folder" => {
                    let path = op.path.as_ref()?;
                    Some(SimulatedOperation::CreateFolder { path: path.clone() })
                }
                "delete" | "trash" => {
                    let path = op.path.as_ref().or(op.source.as_ref())?;
                    Some(SimulatedOperation::Delete { path: path.clone() })
                }
                "rename" => {
                    let path = op.path.as_ref()?;
                    let new_name = op.new_name.as_ref()?;
                    let path_buf = PathBuf::from(path);
                    let new_path = path_buf
                        .parent()
                        .map(|p| p.join(new_name))
                        .unwrap_or_else(|| PathBuf::from(new_name));
                    Some(SimulatedOperation::Move {
                        source: path.clone(),
                        destination: new_path.to_string_lossy().to_string(),
                    })
                }
                _ => None,
            }
        })
        .collect();

    let errors = match crate::vfs::simulate_plan(vfs, operations) {
        Ok(()) => Vec::new(),
        Err(e) => e,
    };

    Ok(VfsValidationResult {
        valid: errors.is_empty(),
        errors,
        plan_hash,
        plan_id,
    })
}

/// Simulate an organize plan on the VFS (legacy)
///
/// Validates all operations in the plan without modifying the real filesystem.
/// Returns a list of errors if any operations would fail.
/// Note: Prefer vfs_validate_plan for new code.
#[tauri::command]
pub async fn vfs_simulate_plan(
    plan: OrganizePlan,
    vfs_state: State<'_, VFSState>,
) -> Result<Vec<String>, String> {
    let mut state = vfs_state.write().await;
    let vfs = state
        .as_mut()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    let operations: Vec<SimulatedOperation> = plan
        .operations
        .iter()
        .filter_map(|op| {
            match op.op_type.as_str() {
                "move" => {
                    let src = op.source.as_ref()?;
                    let dest = op.destination.as_ref()?;
                    Some(SimulatedOperation::Move {
                        source: src.clone(),
                        destination: dest.clone(),
                    })
                }
                "create_folder" => {
                    let path = op.path.as_ref()?;
                    Some(SimulatedOperation::CreateFolder { path: path.clone() })
                }
                "delete" => {
                    let path = op.path.as_ref().or(op.source.as_ref())?;
                    Some(SimulatedOperation::Delete { path: path.clone() })
                }
                "rename" => {
                    // Rename is a move to the same directory with a new name
                    let path = op.path.as_ref()?;
                    let new_name = op.new_name.as_ref()?;
                    let path_buf = PathBuf::from(path);
                    let new_path = path_buf
                        .parent()
                        .map(|p| p.join(new_name))
                        .unwrap_or_else(|| PathBuf::from(new_name));
                    Some(SimulatedOperation::Move {
                        source: path.clone(),
                        destination: new_path.to_string_lossy().to_string(),
                    })
                }
                _ => None,
            }
        })
        .collect();

    match crate::vfs::simulate_plan(vfs, operations) {
        Ok(()) => Ok(Vec::new()),
        Err(errors) => Ok(errors),
    }
}

/// Stage a move operation in the VFS
#[tauri::command]
pub async fn vfs_stage_move(
    source: String,
    destination: String,
    vfs_state: State<'_, VFSState>,
) -> Result<(), String> {
    let mut state = vfs_state.write().await;
    let vfs = state
        .as_mut()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    vfs.stage_move(PathBuf::from(&source), PathBuf::from(&destination))
        .map_err(|e| e.to_string())
}

/// Stage a folder creation in the VFS
#[tauri::command]
pub async fn vfs_stage_create_folder(
    path: String,
    vfs_state: State<'_, VFSState>,
) -> Result<(), String> {
    let mut state = vfs_state.write().await;
    let vfs = state
        .as_mut()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    vfs.stage_create_folder(PathBuf::from(&path))
        .map_err(|e| e.to_string())
}

/// Stage a deletion in the VFS
#[tauri::command]
pub async fn vfs_stage_delete(
    path: String,
    vfs_state: State<'_, VFSState>,
) -> Result<(), String> {
    let mut state = vfs_state.write().await;
    let vfs = state
        .as_mut()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    vfs.stage_delete(PathBuf::from(&path))
        .map_err(|e| e.to_string())
}

/// Apply all staged changes to the VFS
#[tauri::command]
pub async fn vfs_apply_staged(
    vfs_state: State<'_, VFSState>,
) -> Result<(), String> {
    let mut state = vfs_state.write().await;
    let vfs = state
        .as_mut()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    crate::vfs::apply_all_staged(vfs).map_err(|errors| {
        errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })
}

/// Clear all staged changes without applying
#[tauri::command]
pub async fn vfs_clear_staged(
    vfs_state: State<'_, VFSState>,
) -> Result<(), String> {
    let mut state = vfs_state.write().await;
    let vfs = state
        .as_mut()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    vfs.clear_staged();
    Ok(())
}

/// Check if VFS has any staged operations
#[tauri::command]
pub async fn vfs_has_staged(
    vfs_state: State<'_, VFSState>,
) -> Result<bool, String> {
    let state = vfs_state.read().await;
    let vfs = state
        .as_ref()
        .ok_or("VFS not initialized. Call scan_folder_vfs first.")?;

    Ok(vfs.has_staged_operations())
}

/// Clear the VFS state
#[tauri::command]
pub async fn vfs_clear(
    vfs_state: State<'_, VFSState>,
) -> Result<(), String> {
    let mut state = vfs_state.write().await;
    *state = None;
    Ok(())
}

// ============================================================================
// Quarantine Commands
// ============================================================================

/// Move a file or directory to quarantine
#[tauri::command]
pub async fn quarantine_item(
    path: String,
    quarantine_state: State<'_, QuarantineState>,
) -> Result<String, String> {
    let manager = quarantine_state.read().await;
    let path_buf = PathBuf::from(&path);

    let quarantine_path = manager.quarantine(&path_buf)?;
    Ok(quarantine_path.to_string_lossy().to_string())
}

/// Restore a quarantined item to its original location
#[tauri::command]
pub async fn quarantine_restore(
    quarantine_path: String,
    original_path: Option<String>,
    quarantine_state: State<'_, QuarantineState>,
) -> Result<(), String> {
    let manager = quarantine_state.read().await;
    let q_path = PathBuf::from(&quarantine_path);
    let orig_path = original_path.map(PathBuf::from);

    manager.restore(&q_path, orig_path)
}

/// List all quarantined items
#[tauri::command]
pub async fn quarantine_list(
    quarantine_state: State<'_, QuarantineState>,
) -> Result<Vec<QuarantinedItem>, String> {
    let manager = quarantine_state.read().await;
    manager.list()
}

/// Clean up old quarantined items
#[tauri::command]
pub async fn quarantine_cleanup(
    quarantine_state: State<'_, QuarantineState>,
) -> Result<CleanupStats, String> {
    let manager = quarantine_state.read().await;
    manager.cleanup()
}

/// Permanently delete a quarantined item
#[tauri::command]
pub async fn quarantine_permanent_delete(
    quarantine_path: String,
    quarantine_state: State<'_, QuarantineState>,
) -> Result<(), String> {
    let manager = quarantine_state.read().await;
    let path = PathBuf::from(&quarantine_path);
    manager.permanent_delete(&path)
}

/// Check if a path is currently in quarantine
#[tauri::command]
pub async fn quarantine_check(
    original_path: String,
    quarantine_state: State<'_, QuarantineState>,
) -> Result<bool, String> {
    let manager = quarantine_state.read().await;
    let path = PathBuf::from(&original_path);
    Ok(manager.is_quarantined(&path))
}
