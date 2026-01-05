use crate::ai::{run_v6_hybrid_organization, ExpandableDetail, ProgressEvent, AnthropicClient, CredentialManager};
use crate::jobs::OrganizePlan;
use std::path::Path;

/// Rename suggestion response
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameSuggestion {
    pub original_name: String,
    pub suggested_name: String,
    pub path: String,
}

/// API provider status
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub provider: String,
    pub configured: bool,
}

/// Set API key for a provider
#[tauri::command]
pub async fn set_api_key(provider: String, api_key: String) -> Result<bool, String> {
    eprintln!("[DEBUG] set_api_key called for provider: {}", provider);

    // Validate the key first
    if provider == "anthropic" {
        eprintln!("[DEBUG] Validating API key with Anthropic...");
        let is_valid = AnthropicClient::validate_api_key(&api_key).await?;
        if !is_valid {
            eprintln!("[DEBUG] API key validation failed");
            return Ok(false);
        }
        eprintln!("[DEBUG] API key validated successfully");
    }

    // Store the key
    eprintln!("[DEBUG] Attempting to store API key in keychain...");
    match CredentialManager::store_api_key(&provider, &api_key) {
        Ok(_) => {
            eprintln!("[DEBUG] API key stored successfully in keychain");
            // Verify it was stored
            let verify = CredentialManager::has_api_key(&provider);
            eprintln!("[DEBUG] Verification - key exists in keychain: {}", verify);
        }
        Err(e) => {
            eprintln!("[DEBUG] Failed to store API key: {}", e);
            return Err(e);
        }
    }

    Ok(true)
}

/// Delete API key for a provider
#[tauri::command]
pub fn delete_api_key(provider: String) -> Result<(), String> {
    CredentialManager::delete_api_key(&provider)
}

/// Check which providers are configured
/// Checks both credential manager and environment variables
#[tauri::command]
pub fn get_configured_providers() -> Vec<ProviderStatus> {
    // Anthropic: credential manager only (user must configure in settings)
    let has_anthropic = CredentialManager::has_api_key("anthropic");

    // xAI/Grok: check env vars first, then credential manager
    let has_xai = std::env::var("XAI_API_KEY").is_ok()
        || std::env::var("GROK_API_KEY").is_ok()
        || std::env::var("VITE_XAI_API_KEY").is_ok()
        || CredentialManager::has_api_key("xai");

    // OpenAI: check env vars first, then credential manager
    let has_openai = std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("VITE_OPENAI_API_KEY").is_ok()
        || CredentialManager::has_api_key("openai");

    eprintln!("[DEBUG] Provider status - anthropic: {}, xai: {}, openai: {}",
        has_anthropic, has_xai, has_openai);

    vec![
        ProviderStatus {
            provider: "anthropic".to_string(),
            configured: has_anthropic,
        },
        ProviderStatus {
            provider: "xai".to_string(),
            configured: has_xai,
        },
        ProviderStatus {
            provider: "openai".to_string(),
            configured: has_openai,
        },
    ]
}

/// Get rename suggestion for a file
#[tauri::command]
pub async fn get_rename_suggestion(
    path: String,
    filename: String,
    extension: Option<String>,
    size: u64,
    content_preview: Option<String>,
) -> Result<RenameSuggestion, String> {
    let client = AnthropicClient::new();

    let suggested = client
        .suggest_rename(
            &filename,
            extension.as_deref(),
            size,
            content_preview.as_deref(),
        )
        .await?;

    Ok(RenameSuggestion {
        original_name: filename,
        suggested_name: suggested,
        path,
    })
}

/// Apply a rename (with undo info)
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameResult {
    pub success: bool,
    pub old_path: String,
    pub new_path: String,
}

/// Validate that a filename is safe (no path traversal)
fn validate_filename(name: &str) -> Result<(), String> {
    // Reject path separators
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("Invalid filename: path separators not allowed".to_string());
    }

    // Reject control characters and null bytes
    if name.chars().any(|c| c.is_control() || c == '\0') {
        return Err("Invalid filename: control characters not allowed".to_string());
    }

    // Reject empty or whitespace-only names
    if name.trim().is_empty() {
        return Err("Invalid filename: cannot be empty".to_string());
    }

    // Reject names that are too long (filesystem limit)
    if name.len() > 255 {
        return Err("Invalid filename: name too long".to_string());
    }

    Ok(())
}

#[tauri::command]
pub async fn apply_rename(
    old_path: String,
    new_name: String,
) -> Result<RenameResult, String> {
    // SECURITY: Validate filename before any operations
    validate_filename(&new_name)?;

    let old = std::path::Path::new(&old_path);

    if !old.exists() {
        return Err(format!("File does not exist: {}", old_path));
    }

    // SECURITY: Reject symlinks to prevent symlink attacks
    if old.is_symlink() {
        return Err("Cannot rename symbolic links".to_string());
    }

    let parent = old.parent().ok_or("Could not get parent directory")?;
    let new_path = parent.join(&new_name);

    // SECURITY: Verify the new path stays within the same directory
    let canonical_parent = parent.canonicalize()
        .map_err(|e| format!("Parent path validation failed: {}", e))?;

    // For the new file (which doesn't exist yet), verify the parent matches
    let new_parent = new_path.parent().ok_or("Invalid new path")?;
    if new_parent.canonicalize().ok() != Some(canonical_parent.clone()) {
        return Err("Path traversal detected: new file must be in same directory".to_string());
    }

    // Use atomic rename - handles EEXIST race condition
    match std::fs::rename(&old, &new_path) {
        Ok(()) => Ok(RenameResult {
            success: true,
            old_path,
            new_path: new_path.to_string_lossy().to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(format!("File already exists: {}", new_path.display()))
        }
        Err(e) => Err(format!("Failed to rename: {}", e))
    }
}

/// Undo a rename
#[tauri::command]
pub async fn undo_rename(
    current_path: String,
    original_path: String,
) -> Result<(), String> {
    let current = std::path::Path::new(&current_path);
    let original = std::path::Path::new(&original_path);

    if !current.exists() {
        return Err(format!("File does not exist: {}", current_path));
    }

    // SECURITY: Reject symlinks
    if current.is_symlink() {
        return Err("Cannot undo rename of symbolic links".to_string());
    }

    // SECURITY: Verify both paths are in the same directory
    let current_parent = current.parent().ok_or("Invalid current path")?;
    let original_parent = original.parent().ok_or("Invalid original path")?;

    let canonical_current_parent = current_parent.canonicalize()
        .map_err(|e| format!("Current path validation failed: {}", e))?;
    let canonical_original_parent = original_parent.canonicalize()
        .map_err(|e| format!("Original path validation failed: {}", e))?;

    if canonical_current_parent != canonical_original_parent {
        return Err("Security: undo can only restore to same directory".to_string());
    }

    // SECURITY: Validate the original filename
    let original_name = original.file_name()
        .ok_or("Invalid original filename")?
        .to_string_lossy();
    validate_filename(&original_name)?;

    // Use atomic rename
    match std::fs::rename(&current, &original) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(format!("Original path already exists: {}", original_path))
        }
        Err(e) => Err(format!("Failed to undo rename: {}", e))
    }
}

/// Hybrid organization: GPT-5-nano exploration + Claude planning
///
/// This command runs a two-phase organization:
/// 1. GPT-5-nano workers analyze all files (entities, summaries, doc types)
/// 2. Claude creates organization rules based on the enriched analysis
///
/// ## Benefits
/// - **Cost**: GPT-5-nano is ~10x cheaper for bulk file analysis
/// - **Quality**: Claude excels at rule creation and reasoning
/// - **Context**: Entity-based context gives Claude better understanding
#[tauri::command]
pub async fn generate_organize_plan_hybrid(
    folder_path: String,
    user_request: String,
    app_handle: tauri::AppHandle,
) -> Result<OrganizePlan, String> {
    use tauri::Emitter;
    use crate::ai::grok::{FileAnalysis, openai_worker::{FileContent, calculate_worker_count, create_file_batches, run_parallel_workers}};
    use crate::ai::grok::document_parser::parse_document;

    let path = Path::new(&folder_path);
    if !path.exists() || !path.is_dir() {
        return Err(format!("Invalid folder path: {}", folder_path));
    }

    // Get OpenAI API key (check credential manager, then env vars)
    let openai_key = CredentialManager::get_api_key("openai")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .or_else(|_| std::env::var("VITE_OPENAI_API_KEY"))
        .map_err(|_| "OpenAI API key not configured. Set OPENAI_API_KEY or VITE_OPENAI_API_KEY environment variable.".to_string())?;

    // Event emitter for AI thoughts
    let emit = |thought_type: &str, content: &str, expandable_details: Option<Vec<ExpandableDetail>>| {
        let _ = app_handle.emit(
            "ai-thought",
            serde_json::json!({
                "type": thought_type,
                "content": content,
                "expandableDetails": expandable_details,
            }),
        );
    };

    // Progress emitter
    let app_handle_clone = app_handle.clone();
    let progress_emit = move |progress: ProgressEvent| {
        let _ = app_handle_clone.emit("analysis-progress", &progress);
    };

    // Phase 1: Scan folder and extract text
    emit("indexing", "Phase 1: Scanning folder for files...", None);

    let mut file_contents: Vec<FileContent> = Vec::new();
    let entries = std::fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries.filter_map(|e| e.ok()) {
        let entry_path = entry.path();
        if entry_path.is_file() {
            let filename = entry_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let extension = entry_path.extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();

            // Try to extract text from file
            let content = parse_document(&entry_path)
                .map(|doc| doc.text)
                .unwrap_or_else(|_| format!("File: {}", filename));

            file_contents.push(FileContent {
                path: entry_path,
                filename,
                content,
                extension,
            });
        }
    }

    let file_count = file_contents.len();
    if file_count == 0 {
        return Err("No files found in folder".to_string());
    }

    emit("indexing", &format!("Found {} files to analyze", file_count), Some(vec![
        ExpandableDetail { label: "Files".to_string(), value: file_count.to_string() },
        ExpandableDetail { label: "Mode".to_string(), value: "V6 Hybrid (GPT→Claude)".to_string() },
    ]));

    // Phase 2: Run GPT-5-nano workers
    emit("analyzing", "Phase 2: Running GPT-5-nano analysis workers...", None);

    let batch_size = 5;
    let batches = create_file_batches(file_contents, batch_size);
    let worker_count = calculate_worker_count(file_count);

    emit("analyzing", &format!("Dispatching {} workers ({} batches)", worker_count, batches.len()), Some(vec![
        ExpandableDetail { label: "Workers".to_string(), value: worker_count.to_string() },
        ExpandableDetail { label: "Batches".to_string(), value: batches.len().to_string() },
    ]));

    // Run parallel workers
    let results = run_parallel_workers(openai_key, batches, worker_count).await;

    // Collect all successful analyses
    let mut all_analyses: Vec<FileAnalysis> = Vec::new();
    let mut error_messages: Vec<String> = Vec::new();
    for result in results {
        match result {
            Ok(analyses) => all_analyses.extend(analyses),
            Err(e) => error_messages.push(e),
        }
    }

    emit("analyzing", &format!("GPT-5-nano analyzed {} files ({} batch errors)", all_analyses.len(), error_messages.len()), Some(vec![
        ExpandableDetail { label: "Analyzed".to_string(), value: all_analyses.len().to_string() },
        ExpandableDetail { label: "Errors".to_string(), value: error_messages.len().to_string() },
    ]));

    if all_analyses.is_empty() {
        // Return actual error messages so user knows what went wrong
        let error_detail = if error_messages.is_empty() {
            "No files to analyze".to_string()
        } else {
            error_messages.first().cloned().unwrap_or_default()
        };
        return Err(format!("GPT-5-nano analysis failed: {}", error_detail));
    }

    // Phase 3: Run Claude planning with enriched context
    emit("thinking", "Phase 3: Claude is creating organization rules...", None);

    run_v6_hybrid_organization(path, &user_request, all_analyses, emit, Some(progress_emit)).await
}

/// Simplify folder structure when content is already organized
///
/// This command is called when the user accepts the "simplify folder structure"
/// prompt after a hybrid organization returns 0 content operations.
///
/// It focuses on:
/// - Flattening deeply nested hierarchies (depth > 3)
/// - Consolidating sparse folders (< 5 files each)
/// - Shortening verbose path names
#[tauri::command]
pub async fn generate_simplification_plan(
    folder_path: String,
    app_handle: tauri::AppHandle,
) -> Result<OrganizePlan, String> {
    use crate::ai::v2::run_simplification_loop;
    use tauri::Emitter;

    let path = Path::new(&folder_path);
    if !path.exists() || !path.is_dir() {
        return Err(format!("Invalid folder path: {}", folder_path));
    }

    // Event emitter for AI thoughts
    let emit = |thought_type: &str, content: &str, expandable_details: Option<Vec<ExpandableDetail>>| {
        let _ = app_handle.emit(
            "ai-thought",
            serde_json::json!({
                "type": thought_type,
                "content": content,
                "expandableDetails": expandable_details,
            }),
        );
    };

    emit("scanning", "Analyzing folder structure for simplification...", None);

    run_simplification_loop(path, emit).await
}
