use crate::ai::{run_v2_agentic_organize, ExpandableDetail, AnthropicClient, CredentialManager};
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
#[tauri::command]
pub fn get_configured_providers() -> Vec<ProviderStatus> {
    let has_key = CredentialManager::has_api_key("anthropic");
    eprintln!("[DEBUG] Checking if anthropic API key is configured: {}", has_key);

    // Try to get the key to see any error details
    match CredentialManager::get_api_key("anthropic") {
        Ok(_) => eprintln!("[DEBUG] Successfully retrieved anthropic API key from keychain"),
        Err(e) => eprintln!("[DEBUG] Failed to get anthropic API key: {}", e),
    }

    vec![ProviderStatus {
        provider: "anthropic".to_string(),
        configured: has_key,
    }]
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

#[tauri::command]
pub async fn apply_rename(
    old_path: String,
    new_name: String,
) -> Result<RenameResult, String> {
    let old = std::path::Path::new(&old_path);

    if !old.exists() {
        return Err(format!("File does not exist: {}", old_path));
    }

    let parent = old.parent().ok_or("Could not get parent directory")?;
    let new_path = parent.join(&new_name);

    if new_path.exists() {
        return Err(format!("File already exists: {:?}", new_path));
    }

    std::fs::rename(&old, &new_path)
        .map_err(|e| format!("Failed to rename: {}", e))?;

    Ok(RenameResult {
        success: true,
        old_path: old_path.clone(),
        new_path: new_path.to_string_lossy().to_string(),
    })
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

    if original.exists() {
        return Err(format!("Original path already exists: {}", original_path));
    }

    std::fs::rename(&current, &original)
        .map_err(|e| format!("Failed to undo rename: {}", e))?;

    Ok(())
}

/// Agentic organize command - explores folder and generates typed plan
/// Uses Claude tool-use with V2 semantic tools and Shadow VFS
#[tauri::command]
pub async fn generate_organize_plan_agentic(
    folder_path: String,
    user_request: String,
    app_handle: tauri::AppHandle,
) -> Result<OrganizePlan, String> {
    use tauri::Emitter;

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

    run_v2_agentic_organize(Path::new(&folder_path), &user_request, emit).await
}

/// Suggest naming conventions for a folder
#[tauri::command]
pub async fn suggest_naming_conventions(
    folder_path: String,
    app_handle: tauri::AppHandle,
) -> Result<crate::ai::NamingConventionSuggestions, String> {
    use tauri::Emitter;

    let path = std::path::Path::new(&folder_path);
    if !path.exists() || !path.is_dir() {
        return Err(format!("Invalid folder path: {}", folder_path));
    }

    // Emit progress event
    let _ = app_handle.emit(
        "ai-thought",
        serde_json::json!({
            "type": "naming_conventions",
            "content": "Analyzing file naming patterns...",
        }),
    );

    // Build file listing (just top-level files for naming analysis)
    let mut file_listing = String::new();
    let entries = std::fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type().map_err(|e| e.to_string())?;

        if file_type.is_file() {
            file_listing.push_str(&format!("{}\n", name));
        }
    }

    if file_listing.is_empty() {
        return Err("No files found in folder".to_string());
    }

    // Get AI suggestions
    let client = AnthropicClient::new();
    let suggestions = client
        .suggest_naming_conventions(&folder_path, &file_listing)
        .await?;

    let _ = app_handle.emit(
        "ai-thought",
        serde_json::json!({
            "type": "naming_conventions",
            "content": format!("Found {} naming conventions", suggestions.suggestions.len()),
        }),
    );

    Ok(suggestions)
}

/// Generate organize plan with selected naming convention
#[tauri::command]
pub async fn generate_organize_plan_with_convention(
    folder_path: String,
    user_request: String,
    convention: Option<crate::ai::NamingConvention>,
    app_handle: tauri::AppHandle,
) -> Result<crate::jobs::OrganizePlan, String> {
    use tauri::Emitter;

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

    // Build modified request with convention if provided
    let full_request = if let Some(ref conv) = convention {
        format!(
            "{}\n\nIMPORTANT - NAMING CONVENTION TO APPLY:\nWhen renaming files, use the '{}' convention.\nPattern: {}\nExample: {}\n\nApply this naming style consistently to all file rename operations.",
            user_request, conv.name, conv.pattern, conv.example
        )
    } else {
        user_request
    };

    run_v2_agentic_organize(Path::new(&folder_path), &full_request, emit).await
}
