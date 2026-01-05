//! Chat Tauri commands
//!
//! Provides Tauri command handlers for the Omni-Chat feature:
//! - chat_stream: Run chat agent with streaming responses
//! - list_files_for_mention: Get files for @ mention autocomplete

use crate::ai::chat::{run_chat_agent, ChatAgentResult, ContextItem, ConversationMessage};
use crate::billing::{BillingState, LimitCheckResult};
use crate::security::PathValidator;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tracing::{debug, info, warn};

/// Global abort flag for chat operations
pub struct ChatAbortFlag(pub Arc<AtomicBool>);

impl Default for ChatAbortFlag {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

/// File entry for mention autocomplete
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MentionFile {
    pub path: String,
    pub name: String,
    pub is_directory: bool,
}

/// Chat stream request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamRequest {
    pub message: String,
    pub context_items: Vec<ContextItem>,
    pub model: String,
    pub history: Vec<ConversationMessage>,
    #[serde(default = "default_extended_thinking")]
    pub extended_thinking: bool,
    /// Optional user ID for billing (Clerk token identifier)
    pub user_id: Option<String>,
}

fn default_extended_thinking() -> bool {
    true
}

/// Chat stream response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamResponse {
    pub success: bool,
    pub response: Option<String>,
    pub error: Option<String>,
}

/// Run chat agent with streaming
///
/// Events emitted:
/// - chat:token - { chunk: string } - streamed response text
/// - chat:thought - { id, tool, input?, output?, status } - tool usage
/// - chat:complete - {} - finished
/// - chat:error - { message } - error occurred
/// - chat:aborted - { reason: string } - aborted by user
/// - chat:limit-error - { reason, upgradeUrl } - limit exceeded
#[tauri::command]
pub async fn chat_stream(
    app: AppHandle,
    abort_flag: State<'_, ChatAbortFlag>,
    billing: State<'_, BillingState>,
    request: ChatStreamRequest,
) -> Result<ChatStreamResponse, String> {
    info!(
        model = request.model,
        context_items = request.context_items.len(),
        history_len = request.history.len(),
        "Starting chat stream"
    );

    // Reset abort flag at start of new chat
    abort_flag.0.store(false, std::sync::atomic::Ordering::SeqCst);

    // Map to Anthropic model aliases (or use full IDs)
    let model_id = match request.model.as_str() {
        "claude-haiku-4-5" => "claude-haiku-4-5",     // claude-haiku-4-5-20251001
        "claude-sonnet-4-5" => "claude-sonnet-4-5",   // claude-sonnet-4-5-20250929
        "claude-opus-4-5" => "claude-opus-4-5",       // claude-opus-4-5-20251101
        _ => &request.model,
    };

    // === BILLING: Check limits before API call ===
    if let Some(ref user_id) = request.user_id {
        let subscription = billing.subscription_manager.get_cached_or_default(user_id);
        let usage = billing.usage_tracker.get_today_usage(user_id)?;

        let limit_result = billing.limit_enforcer.check_limit(
            &subscription,
            &usage,
            model_id,
            request.extended_thinking,
        );

        match limit_result {
            LimitCheckResult::Denied { reason, upgrade_url } => {
                let error_message = reason.to_string();
                info!(reason = %error_message, "Chat limit denied");

                // Emit limit error event
                let _ = app.emit(
                    "chat:limit-error",
                    serde_json::json!({
                        "reason": error_message,
                        "upgradeUrl": upgrade_url,
                    }),
                );

                return Ok(ChatStreamResponse {
                    success: false,
                    response: None,
                    error: Some(error_message),
                });
            }
            LimitCheckResult::Allowed { remaining } => {
                debug!(remaining = remaining, "Limit check passed");
            }
        }
    } else {
        debug!("No user_id provided, skipping billing check");
    }
    // === END BILLING CHECK ===

    // Clone the abort flag Arc for passing to agent
    let abort_flag_arc = Some(Arc::clone(&abort_flag.0));

    match run_chat_agent(
        &app,
        &request.message,
        &request.context_items,
        model_id,
        &request.history,
        request.extended_thinking,
        abort_flag_arc,
    )
    .await
    {
        Ok(ChatAgentResult { response, usage }) => {
            info!(
                input_tokens = usage.input_tokens,
                output_tokens = usage.output_tokens,
                cache_created = usage.cache_creation_input_tokens,
                cache_read = usage.cache_read_input_tokens,
                "Chat completed successfully"
            );

            // === BILLING: Record usage on success with ACCURATE token counts ===
            if let Some(ref user_id) = request.user_id {
                if let Err(e) = billing.usage_tracker.increment_request(
                    user_id,
                    model_id,
                    request.extended_thinking,
                    usage.input_tokens,
                    usage.output_tokens,
                ) {
                    warn!(error = %e, "Failed to record usage");
                }
            }
            // === END BILLING RECORD ===

            Ok(ChatStreamResponse {
                success: true,
                response: Some(response),
                error: None,
            })
        }
        Err(e) => {
            warn!(error = %e, "Chat failed");
            // Emit error event
            let _ = app.emit("chat:error", serde_json::json!({ "message": e }));
            Ok(ChatStreamResponse {
                success: false,
                response: None,
                error: Some(e),
            })
        }
    }
}

/// Abort the current chat operation
#[tauri::command]
pub fn abort_chat(abort_flag: State<ChatAbortFlag>) -> Result<(), String> {
    info!("Chat abort requested");
    abort_flag.0.store(true, Ordering::SeqCst);
    Ok(())
}

/// Reset the abort flag (call before starting a new chat)
#[tauri::command]
pub fn reset_chat_abort(abort_flag: State<ChatAbortFlag>) -> Result<(), String> {
    abort_flag.0.store(false, Ordering::SeqCst);
    Ok(())
}

/// Directories to skip during recursive search
const MENTION_EXCLUDED_DIRS: &[&str] = &[
    "node_modules", ".git", ".cache", ".npm", ".cargo", "target", "build", "dist",
    ".venv", "__pycache__", ".Trash", "Pods", ".gradle", ".m2", ".pnpm", ".yarn",
    "vendor", ".next", ".nuxt", "Library", ".Spotlight-V100", ".fseventsd",
];

/// List files for @ mention autocomplete
///
/// Returns files and folders in the given directory for mention suggestions.
/// Supports recursive search for finding files in subdirectories.
#[tauri::command]
pub async fn list_files_for_mention(
    directory: String,
    query: Option<String>,
    max_results: Option<usize>,
    recursive: Option<bool>,
) -> Result<Vec<MentionFile>, String> {
    debug!(directory = directory, recursive = ?recursive, "Listing files for mention");

    let dir_path = PathBuf::from(&directory);

    // Security: Validate the directory path
    let validated_path = PathValidator::validate_for_read(&dir_path, None)
        .map_err(|e| format!("Path validation failed: {}", e))?;

    if !validated_path.is_dir() {
        return Err(format!("Not a directory: {}", directory));
    }

    let max = max_results.unwrap_or(50);
    let query_lower = query.map(|q| q.to_lowercase());
    let do_recursive = recursive.unwrap_or(true);
    // Limit recursive depth - search deeper when user provides a query
    let max_depth = if do_recursive && query_lower.is_some() { 5 } else { 1 };

    debug!(
        max = max,
        recursive = do_recursive,
        max_depth = max_depth,
        query = ?query_lower,
        "Search params"
    );

    let mut results: Vec<MentionFile> = Vec::new();

    // Recursive search helper
    fn search_dir(
        dir: &PathBuf,
        query_lower: &Option<String>,
        results: &mut Vec<MentionFile>,
        max: usize,
        current_depth: usize,
        max_depth: usize,
    ) {
        if current_depth > max_depth || results.len() >= max {
            return;
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            if results.len() >= max {
                break;
            }

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files/dirs
            if name.starts_with('.') {
                continue;
            }

            let is_directory = path.is_dir();

            // Skip excluded directories
            if is_directory && MENTION_EXCLUDED_DIRS.contains(&name.as_str()) {
                continue;
            }

            // Check if name matches query
            let matches_query = match query_lower {
                Some(ref q) if !q.is_empty() => name.to_lowercase().contains(q),
                _ => true, // No query or empty query = show all
            };

            if matches_query {
                results.push(MentionFile {
                    path: path.display().to_string(),
                    name: name.clone(),
                    is_directory,
                });
            }

            // Recurse into directories (only if we have a query to narrow results)
            if is_directory && query_lower.as_ref().map(|q| !q.is_empty()).unwrap_or(false) {
                search_dir(&path, query_lower, results, max, current_depth + 1, max_depth);
            }
        }
    }

    search_dir(&validated_path, &query_lower, &mut results, max, 1, max_depth);

    // Sort: directories first, then alphabetically
    results.sort_by(|a, b| {
        match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mention_file_serialize() {
        let file = MentionFile {
            path: "/test/path.txt".to_string(),
            name: "path.txt".to_string(),
            is_directory: false,
        };

        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("isDirectory"));
        assert!(json.contains("path.txt"));
    }
}
