//! Grok AI Commands
//!
//! Tauri commands for the multi-model file analysis pipeline.
//! Provides commands for scanning, organizing, and executing file organization plans.

use crate::ai::grok::{
    DocumentAnalysis, GrokOrganizer, OrganizationPlan,
    ScanResult, sanitize_filename, sanitize_folder_path,
};
use crate::ai::grok::AnalysisPhase;
use crate::execution::executor::{ExecutionEngine, ProgressCallback};
use crate::jobs::{OrganizeOperation, OrganizePlan};
use crate::wal::entry::{WALJournal, WALOperationType};
use crate::wal::journal::WALManager;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

/// Abort flag for cancelling Grok operations
/// Similar to ChatAbortFlag, this allows the frontend to signal cancellation
pub struct GrokAbortFlag(pub Arc<AtomicBool>);

impl Default for GrokAbortFlag {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

/// State for the Grok organizer
pub struct GrokState {
    organizer: Mutex<Option<Arc<GrokOrganizer>>>,
}

impl GrokState {
    pub fn new() -> Self {
        Self {
            organizer: Mutex::new(None),
        }
    }
}

impl Default for GrokState {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize the Grok organizer with an API key
/// If api_key is empty, will try to get from environment or credential manager
/// Validates the API key format before initializing
#[tauri::command]
pub async fn grok_init(
    api_key: Option<String>,
    state: State<'_, GrokState>,
    app: AppHandle,
) -> Result<(), String> {
    // Get API key from parameter or fallback sources
    let key = match api_key {
        Some(k) if !k.is_empty() => {
            // Validate key passed directly
            validate_api_key(&k)?;
            k
        }
        _ => get_grok_api_key()?, // Already validates internally
    };

    // Get cache directory
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {}", e))?
        .join("grok_cache");

    let organizer = GrokOrganizer::new(key, &cache_dir)?;

    let mut guard = state.organizer.lock().await;
    *guard = Some(Arc::new(organizer));

    tracing::info!("[Grok] Organizer initialized");
    Ok(())
}

/// Scan a folder to identify analyzable files
#[tauri::command]
pub async fn grok_scan_folder(
    path: String,
    state: State<'_, GrokState>,
) -> Result<ScanResult, String> {
    let guard = state.organizer.lock().await;
    let organizer = guard
        .as_ref()
        .ok_or("Grok not initialized. Call grok_init first.")?;

    let path = PathBuf::from(path);
    organizer.scan_folder(&path).await
}

/// Run the full organization pipeline
#[tauri::command]
pub async fn grok_organize(
    path: String,
    user_instruction: String,
    state: State<'_, GrokState>,
    app: AppHandle,
) -> Result<OrganizationPlan, String> {
    let guard = state.organizer.lock().await;
    let organizer = guard
        .as_ref()
        .ok_or("Grok not initialized. Call grok_init first.")?
        .clone();
    drop(guard); // Release lock before long-running operation

    let path = PathBuf::from(path);
    let app_clone = app.clone();

    let plan = organizer
        .organize(&path, &user_instruction, move |progress| {
            // Emit progress events to frontend
            let _ = app_clone.emit("grok:progress", &progress);
        })
        .await?;

    Ok(plan)
}

/// Abort any running Grok plan generation
/// Sets the abort flag which is checked during the organize pipeline
#[tauri::command]
#[allow(dead_code)]
pub fn grok_abort_plan(abort_flag: State<GrokAbortFlag>) -> Result<(), String> {
    tracing::info!("[Grok] Aborting plan generation");
    abort_flag.0.store(true, Ordering::SeqCst);
    Ok(())
}

/// Reset the Grok abort flag (called before starting a new plan)
#[tauri::command]
#[allow(dead_code)]
pub fn grok_reset_abort(abort_flag: State<GrokAbortFlag>) -> Result<(), String> {
    abort_flag.0.store(false, Ordering::SeqCst);
    Ok(())
}

/// Generate organization plan using Grok pipeline and return in frontend-compatible format
/// This is the main entry point for the ChangesPanel to use Grok analysis
#[tauri::command]
pub async fn grok_generate_plan(
    path: String,
    user_instruction: String,
    state: State<'_, GrokState>,
    abort_flag: State<'_, GrokAbortFlag>,
    app: AppHandle,
) -> Result<OrganizePlan, String> {
    // Reset abort flag at start of new plan
    abort_flag.0.store(false, Ordering::SeqCst);

    let guard = state.organizer.lock().await;
    let organizer = guard
        .as_ref()
        .ok_or("Grok not initialized. Call grok_init first.")?
        .clone();
    drop(guard);

    let folder_path = PathBuf::from(&path);
    let app_clone = app.clone();
    let abort_flag_clone = Arc::clone(&abort_flag.0);

    // Emit progress as ai-thought events (for compatibility with ChangesPanel)
    let emit_thought = |phase: &str, message: &str, details: Option<Vec<(&str, String)>>| {
        let expandable_details: Option<Vec<serde_json::Value>> = details.map(|d| {
            d.into_iter()
                .map(|(label, value)| serde_json::json!({"label": label, "value": value}))
                .collect()
        });

        let _ = app_clone.emit(
            "ai-thought",
            serde_json::json!({
                "type": phase,
                "content": message,
                "expandableDetails": expandable_details,
            }),
        );
    };

    emit_thought("scanning", "Analyzing folder contents with AI...", None);

    // Track last phase to only emit ai-thought on phase changes
    let last_phase = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));

    // Clone abort flag for use in callback
    let abort_flag_for_callback = Arc::clone(&abort_flag_clone);

    let plan = organizer
        .organize(&folder_path, &user_instruction, move |progress| {
            // Check if aborted - skip all emissions if so
            if abort_flag_for_callback.load(Ordering::SeqCst) {
                return;
            }

            // Map Grok progress phases to names
            let phase_name = match progress.phase {
                AnalysisPhase::Scanning => "scanning",
                AnalysisPhase::CheckingCache => "extracting",
                AnalysisPhase::RenderingPdf => "rendering",
                AnalysisPhase::AnalyzingContent => "analyzing",
                AnalysisPhase::Aggregating => "summarizing",
                AnalysisPhase::Planning => "planning",
                AnalysisPhase::Complete => "complete",
                AnalysisPhase::Failed => "error",
            };

            // Always emit analysis-progress for the progress bar
            let _ = app.emit(
                "analysis-progress",
                serde_json::json!({
                    "phase": phase_name,
                    "current": progress.current,
                    "total": progress.total,
                    "message": progress.message,
                }),
            );

            // Only emit ai-thought on phase transitions (not every progress update)
            let mut last = last_phase.lock().unwrap();
            let should_emit_thought = match &*last {
                None => true,
                Some(prev) => prev != phase_name,
            };

            if should_emit_thought {
                *last = Some(phase_name.to_string());
                drop(last); // Release lock before emit

                let thought_type = match progress.phase {
                    AnalysisPhase::Scanning => "scanning",
                    AnalysisPhase::CheckingCache | AnalysisPhase::RenderingPdf | AnalysisPhase::AnalyzingContent => "analyzing",
                    AnalysisPhase::Aggregating => "thinking",
                    AnalysisPhase::Planning => "planning",
                    AnalysisPhase::Complete => "complete",
                    AnalysisPhase::Failed => "error",
                };

                let _ = app.emit(
                    "ai-thought",
                    serde_json::json!({
                        "type": thought_type,
                        "content": progress.message,
                    }),
                );
            }
        })
        .await?;

    // Check if aborted after organize completes
    if abort_flag_clone.load(Ordering::SeqCst) {
        tracing::info!("[Grok] Plan generation was aborted by user");
        return Err("Organization cancelled by user".to_string());
    }

    // Convert OrganizationPlan to OrganizePlan (frontend format)
    let frontend_plan = convert_to_frontend_plan(plan, &path);

    emit_thought("complete", &format!(
        "Created plan with {} operations",
        frontend_plan.operations.len()
    ), None);

    Ok(frontend_plan)
}

/// Convert Grok's OrganizationPlan to frontend's OrganizePlan format
fn convert_to_frontend_plan(plan: OrganizationPlan, target_folder: &str) -> OrganizePlan {
    let mut operations = Vec::new();
    let target_path = PathBuf::from(target_folder);

    // Convert folder creations to create_folder operations
    for folder in &plan.folder_structure {
        let sanitized_path = sanitize_folder_path(&folder.path);
        let full_path = target_path.join(&sanitized_path);

        operations.push(OrganizeOperation {
            op_id: uuid::Uuid::new_v4().to_string(),
            op_type: "create_folder".to_string(),
            source: None,
            destination: None,
            path: Some(full_path.to_string_lossy().to_string()),
            new_name: None,
        });
    }

    // Convert file assignments to move operations
    for assignment in &plan.assignments {
        let source = PathBuf::from(&assignment.file_path);
        let sanitized_folder = sanitize_folder_path(&assignment.destination_folder);

        // Get the new filename (sanitized)
        let new_filename = match &assignment.new_name {
            Some(name) if !name.is_empty() => {
                sanitize_filename(name, &assignment.original_name)
            }
            _ => assignment.original_name.clone(),
        };

        let destination = target_path.join(&sanitized_folder).join(&new_filename);

        // Check if this is a rename (same directory, different name)
        let source_parent = source.parent().map(|p| p.to_string_lossy().to_string());
        let dest_parent = destination.parent().map(|p| p.to_string_lossy().to_string());
        let is_same_dir = source_parent == dest_parent;
        let is_rename = is_same_dir && new_filename != assignment.original_name;

        if is_rename {
            // Rename operation
            operations.push(OrganizeOperation {
                op_id: uuid::Uuid::new_v4().to_string(),
                op_type: "rename".to_string(),
                source: None,
                destination: None,
                path: Some(source.to_string_lossy().to_string()),
                new_name: Some(new_filename),
            });
        } else {
            // Move operation
            operations.push(OrganizeOperation {
                op_id: uuid::Uuid::new_v4().to_string(),
                op_type: "move".to_string(),
                source: Some(source.to_string_lossy().to_string()),
                destination: Some(destination.to_string_lossy().to_string()),
                path: None,
                new_name: None,
            });
        }
    }

    OrganizePlan {
        plan_id: uuid::Uuid::new_v4().to_string(),
        description: format!(
            "{}: {}",
            plan.strategy_name,
            plan.description
        ),
        operations,
        target_folder: target_folder.to_string(),
        simplification_recommended: None,
    }
}

/// Analyze a single file
#[tauri::command]
pub async fn grok_analyze_file(
    path: String,
    state: State<'_, GrokState>,
) -> Result<DocumentAnalysis, String> {
    let guard = state.organizer.lock().await;
    let organizer = guard
        .as_ref()
        .ok_or("Grok not initialized. Call grok_init first.")?;

    let path = PathBuf::from(path);
    organizer.analyze_single(&path).await
}

/// Get cache statistics
#[tauri::command]
pub async fn grok_cache_stats(
    state: State<'_, GrokState>,
) -> Result<GrokCacheStats, String> {
    let guard = state.organizer.lock().await;
    let organizer = guard
        .as_ref()
        .ok_or("Grok not initialized. Call grok_init first.")?;

    let stats = organizer.cache_stats()?;

    Ok(GrokCacheStats {
        files_analyzed: stats.files_analyzed as usize,
        tokens_used: stats.tokens_used as usize,
        cost_cents: stats.cost_cents as usize,
        cache_hits: stats.cache_hits as usize,
    })
}

/// Clear the content cache
#[tauri::command]
pub async fn grok_clear_cache(state: State<'_, GrokState>) -> Result<(), String> {
    let guard = state.organizer.lock().await;
    let organizer = guard
        .as_ref()
        .ok_or("Grok not initialized. Call grok_init first.")?;

    organizer.clear_cache()?;
    tracing::info!("[Grok] Cache cleared");
    Ok(())
}

/// Cache statistics for frontend
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrokCacheStats {
    pub files_analyzed: usize,
    pub tokens_used: usize,
    pub cost_cents: usize,
    pub cache_hits: usize,
}

/// Check if Grok API key is configured
#[tauri::command]
pub async fn grok_check_api_key() -> Result<bool, String> {
    // Check environment variable or credential manager
    let has_env_key = std::env::var("XAI_API_KEY").is_ok()
        || std::env::var("GROK_API_KEY").is_ok()
        || std::env::var("VITE_XAI_API_KEY").is_ok();

    if has_env_key {
        return Ok(true);
    }

    // Check credential manager
    use crate::ai::credentials::CredentialManager;
    match CredentialManager::get_api_key("xai") {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Validate that an API key is not a placeholder or invalid value
/// Returns Ok(()) if valid, Err with description if invalid
fn validate_api_key(key: &str) -> Result<(), String> {
    let key_lower = key.to_lowercase();

    // Check for common placeholder patterns
    let placeholder_patterns = [
        "your-api-key",
        "your_api_key",
        "your api key",
        "yourapikey",
        "api-key-here",
        "api_key_here",
        "enter-your",
        "enter_your",
        "replace-with",
        "replace_with",
        "xxx",
        "placeholder",
        "example",
        "test-key",
        "test_key",
        "demo",
        "sk-xxx",
        "xai-xxx",
    ];

    for pattern in placeholder_patterns {
        if key_lower.contains(pattern) {
            return Err(format!(
                "API key appears to be a placeholder (contains '{}'). Please enter a valid xAI API key from https://console.x.ai",
                pattern
            ));
        }
    }

    // xAI keys typically start with "xai-" and are fairly long
    if key.len() < 20 {
        return Err("API key is too short. Valid xAI API keys are typically longer. Get yours from https://console.x.ai".to_string());
    }

    // Check for obviously invalid characters (spaces at start/end, newlines)
    if key.trim() != key {
        return Err("API key contains leading/trailing whitespace. Please remove any extra spaces.".to_string());
    }

    if key.contains('\n') || key.contains('\r') {
        return Err("API key contains newline characters. Please enter only the key itself.".to_string());
    }

    Ok(())
}

/// Get the Grok API key from any available source
fn get_grok_api_key() -> Result<String, String> {
    // Priority: env vars > credential manager
    if let Ok(key) = std::env::var("XAI_API_KEY") {
        validate_api_key(&key)?;
        return Ok(key);
    }
    if let Ok(key) = std::env::var("GROK_API_KEY") {
        validate_api_key(&key)?;
        return Ok(key);
    }
    if let Ok(key) = std::env::var("VITE_XAI_API_KEY") {
        validate_api_key(&key)?;
        return Ok(key);
    }

    // Try credential manager
    use crate::ai::credentials::CredentialManager;
    let key = CredentialManager::get_api_key("xai")
        .map_err(|_| "No Grok API key found. Set XAI_API_KEY in .env or configure in settings.".to_string())?;

    validate_api_key(&key)?;
    Ok(key)
}

/// Store Grok API key (uses the existing credential manager)
/// Validates the key format before storing
#[tauri::command]
pub async fn grok_set_api_key(api_key: String) -> Result<(), String> {
    use crate::ai::credentials::CredentialManager;

    // Validate the key before storing
    validate_api_key(&api_key)?;

    CredentialManager::store_api_key("xai", &api_key)?;
    tracing::info!("[Grok] API key stored");
    Ok(())
}

/// Get Grok API key from credential manager
#[tauri::command]
pub async fn grok_get_api_key() -> Result<Option<String>, String> {
    use crate::ai::credentials::CredentialManager;

    match CredentialManager::get_api_key("xai") {
        Ok(key) => Ok(Some(key)),
        Err(_) => Ok(None),
    }
}

/// Execute an OrganizationPlan by converting it to WAL operations
///
/// This converts the Grok plan into executable filesystem operations:
/// 1. Creates all planned folders
/// 2. Moves files to their destinations with sanitized names
#[tauri::command]
pub async fn grok_execute_plan(
    plan: OrganizationPlan,
    target_folder: String,
    app: AppHandle,
) -> Result<GrokExecutionResult, String> {
    use tauri::Emitter;

    let target_path = PathBuf::from(&target_folder);

    // Generate a unique job ID
    let job_id = format!("grok-{}", uuid::Uuid::new_v4());

    tracing::info!(
        "[Grok] Executing plan: {} folders, {} assignments",
        plan.folder_structure.len(),
        plan.assignments.len()
    );

    // Emit start event
    let _ = app.emit("grok:execution", serde_json::json!({
        "phase": "starting",
        "message": format!("Creating {} folders, moving {} files",
            plan.folder_structure.len(),
            plan.assignments.len())
    }));

    // Create WAL journal
    let mut journal = WALJournal::new(job_id.clone(), target_path.clone());

    // Track folder operation IDs for dependencies
    let mut folder_op_ids: std::collections::HashMap<String, uuid::Uuid> = std::collections::HashMap::new();

    // Step 1: Add CreateFolder operations for all planned folders
    for planned_folder in &plan.folder_structure {
        let sanitized_path = sanitize_folder_path(&planned_folder.path);
        let full_path = target_path.join(&sanitized_path);

        let op = WALOperationType::CreateFolder { path: full_path };
        match journal.add_operation(op) {
            Ok(op_id) => {
                folder_op_ids.insert(sanitized_path, op_id);
            }
            Err(e) => {
                tracing::warn!("[Grok] Failed to add folder operation: {}", e);
            }
        }
    }

    // Step 2: Add Move operations for all file assignments
    let mut move_count = 0;
    for assignment in &plan.assignments {
        let source = PathBuf::from(&assignment.file_path);

        // Sanitize destination folder path
        let sanitized_folder = sanitize_folder_path(&assignment.destination_folder);

        // Get sanitized filename (uses the utility we created)
        let new_filename = match &assignment.new_name {
            Some(name) if !name.is_empty() => {
                sanitize_filename(name, &assignment.original_name)
            }
            _ => assignment.original_name.clone(),
        };

        // Build full destination path
        let destination = target_path.join(&sanitized_folder).join(&new_filename);

        // Find dependency on parent folder creation
        let mut depends_on = Vec::new();
        if let Some(&folder_op_id) = folder_op_ids.get(&sanitized_folder) {
            depends_on.push(folder_op_id);
        }

        let op = WALOperationType::Move {
            source,
            destination,
        };

        match journal.add_operation_with_deps(op, depends_on) {
            Ok(_) => {
                move_count += 1;
            }
            Err(e) => {
                tracing::warn!("[Grok] Failed to add move operation for {}: {}",
                    assignment.original_name, e);
            }
        }
    }

    tracing::info!(
        "[Grok] Created WAL journal with {} folder ops and {} move ops",
        folder_op_ids.len(),
        move_count
    );

    // Emit progress
    let _ = app.emit("grok:execution", serde_json::json!({
        "phase": "executing",
        "message": format!("Executing {} operations...", folder_op_ids.len() + move_count)
    }));

    // Save and execute the journal
    let wal_manager = WALManager::new();
    wal_manager.save_journal(&journal).map_err(|e| e.message)?;

    let engine = ExecutionEngine::new();
    let app_clone = app.clone();

    // Execute with progress callback
    // Emit both grok:execution (for Grok-specific UIs) and execution-progress (for ChangesPanel)
    let progress_callback: Arc<ProgressCallback> = Arc::new(Box::new(move |current: usize, total: usize| {
        // Emit execution-progress for ChangesPanel compatibility
        let _ = app_clone.emit("execution-progress", serde_json::json!({
            "completed": current,
            "total": total,
        }));
        // Also emit grok:execution for richer Grok-specific details
        let _ = app_clone.emit("grok:execution", serde_json::json!({
            "phase": "progress",
            "current": current,
            "total": total,
            "message": format!("Completed {}/{} operations", current, total)
        }));
    }));

    let result = engine
        .execute_journal_with_progress(&job_id, Some(progress_callback))
        .await?;

    // Clean up journal after successful execution
    if result.success {
        let _ = wal_manager.discard_journal(&job_id);
    }

    // Emit completion
    let _ = app.emit("grok:execution", serde_json::json!({
        "phase": if result.success { "complete" } else { "failed" },
        "message": format!(
            "Completed: {} succeeded, {} failed, {} skipped",
            result.completed_count,
            result.failed_count,
            result.skipped_count
        )
    }));

    tracing::info!(
        "[Grok] Execution complete: {} completed, {} failed",
        result.completed_count,
        result.failed_count
    );

    Ok(GrokExecutionResult {
        job_id,
        completed_count: result.completed_count,
        failed_count: result.failed_count,
        skipped_count: result.skipped_count,
        renamed_count: result.renamed_count,
        errors: result.errors,
        success: result.success,
    })
}

/// Result of executing a Grok organization plan
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrokExecutionResult {
    pub job_id: String,
    pub completed_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub renamed_count: usize,
    pub errors: Vec<String>,
    pub success: bool,
}
