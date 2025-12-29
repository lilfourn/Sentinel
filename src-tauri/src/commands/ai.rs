use crate::ai::{AnthropicClient, CredentialManager};
use crate::jobs::OrganizePlan;

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

/// Folder context for AI organization
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderContext {
    pub path: String,
    pub ls_output: String,
    pub analysis: Option<String>,
}

/// Build folder context (uses Claude Haiku for speed)
#[tauri::command]
pub async fn build_folder_context(folder_path: String) -> Result<FolderContext, String> {
    let path = std::path::Path::new(&folder_path);

    if !path.exists() || !path.is_dir() {
        return Err(format!("Invalid folder path: {}", folder_path));
    }

    // Build ls -R style output
    let mut ls_output = String::new();
    build_ls_output(path, path, &mut ls_output, 0)?;

    // Get AI analysis using Haiku (fast)
    let client = AnthropicClient::new();
    let analysis = match client.analyze_folder_context(&folder_path, &ls_output).await {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!("Failed to analyze context: {}", e);
            None
        }
    };

    Ok(FolderContext {
        path: folder_path,
        ls_output,
        analysis,
    })
}

/// Generate organization plan (uses Claude Sonnet)
/// DEPRECATED: Use generate_organize_plan_agentic instead
#[tauri::command]
pub async fn generate_organize_plan(
    context: FolderContext,
    user_request: String,
) -> Result<String, String> {
    let client = AnthropicClient::new();

    client
        .generate_organize_plan(
            &context.path,
            &context.ls_output,
            &user_request,
            context.analysis.as_deref(),
        )
        .await
}

/// Agentic organize command - explores folder and generates typed plan
/// Uses Claude tool-use to explore before generating the plan
#[tauri::command]
pub async fn generate_organize_plan_agentic(
    folder_path: String,
    user_request: String,
    app_handle: tauri::AppHandle,
) -> Result<OrganizePlan, String> {
    use tauri::Emitter;

    let client = AnthropicClient::new();

    let emit = |thought_type: &str, content: &str| {
        let _ = app_handle.emit(
            "ai-thought",
            serde_json::json!({
                "type": thought_type,
                "content": content,
            }),
        );
    };

    client
        .run_agentic_organize(&folder_path, &user_request, emit)
        .await
}

/// Helper to build ls -R style output
fn build_ls_output(
    root: &std::path::Path,
    current: &std::path::Path,
    output: &mut String,
    depth: usize,
) -> Result<(), String> {
    if depth > 5 {
        return Ok(()); // Limit depth
    }

    let relative = current
        .strip_prefix(root)
        .unwrap_or(current)
        .to_string_lossy();

    if depth > 0 {
        output.push_str(&format!("\n{}:\n", relative));
    } else {
        output.push_str("./:\n");
    }

    let entries: Vec<_> = std::fs::read_dir(current)
        .map_err(|e| format!("Failed to read directory: {}", e))?
        .filter_map(|e| e.ok())
        .collect();

    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type().map_err(|e| e.to_string())?;

        if file_type.is_dir() {
            output.push_str(&format!("{}/\n", name));
        } else {
            output.push_str(&format!("{}\n", name));
        }
    }

    // Recurse into subdirectories
    for entry in entries {
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                build_ls_output(root, &entry.path(), output, depth + 1)?;
            }
        }
    }

    Ok(())
}
