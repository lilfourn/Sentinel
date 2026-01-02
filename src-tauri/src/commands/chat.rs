//! Chat Tauri commands
//!
//! Provides Tauri command handlers for the Omni-Chat feature:
//! - chat_stream: Run chat agent with streaming responses
//! - list_files_for_mention: Get files for @ mention autocomplete

use crate::ai::chat::{run_chat_agent, ContextItem, ConversationMessage};
use crate::billing::{BillingState, LimitCheckResult};
use crate::security::PathValidator;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

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
    eprintln!("[ChatCommand] Starting chat_stream");
    eprintln!("[ChatCommand] Model: {}", request.model);
    eprintln!("[ChatCommand] Context items: {}", request.context_items.len());
    eprintln!("[ChatCommand] History length: {}", request.history.len());

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
                eprintln!("[ChatCommand] Limit denied: {}", error_message);

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
                eprintln!(
                    "[ChatCommand] Limit check passed, {} requests remaining",
                    remaining
                );
            }
        }
    } else {
        eprintln!("[ChatCommand] No user_id provided, skipping billing check");
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
        Ok(response) => {
            eprintln!("[ChatCommand] Chat completed successfully");

            // === BILLING: Record usage on success ===
            if let Some(ref user_id) = request.user_id {
                // TODO: Extract actual token counts from response
                // For now, use estimates based on response length
                let input_tokens = (request.message.len() / 4) as u64;
                let output_tokens = (response.len() / 4) as u64;

                if let Err(e) = billing.usage_tracker.increment_request(
                    user_id,
                    model_id,
                    request.extended_thinking,
                    input_tokens,
                    output_tokens,
                ) {
                    eprintln!("[ChatCommand] Failed to record usage: {}", e);
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
            eprintln!("[ChatCommand] Chat failed: {}", e);
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
    eprintln!("[ChatCommand] Aborting chat");
    abort_flag.0.store(true, Ordering::SeqCst);
    Ok(())
}

/// Reset the abort flag (call before starting a new chat)
#[tauri::command]
pub fn reset_chat_abort(abort_flag: State<ChatAbortFlag>) -> Result<(), String> {
    abort_flag.0.store(false, Ordering::SeqCst);
    Ok(())
}

/// List files for @ mention autocomplete
///
/// Returns files and folders in the given directory for mention suggestions
#[tauri::command]
pub async fn list_files_for_mention(
    directory: String,
    query: Option<String>,
    max_results: Option<usize>,
) -> Result<Vec<MentionFile>, String> {
    eprintln!("[ChatCommand] list_files_for_mention: {}", directory);

    let dir_path = PathBuf::from(&directory);

    // Security: Validate the directory path
    let validated_path = PathValidator::validate_for_read(&dir_path, None)
        .map_err(|e| format!("Path validation failed: {}", e))?;

    if !validated_path.is_dir() {
        return Err(format!("Not a directory: {}", directory));
    }

    let max = max_results.unwrap_or(50);
    let query_lower = query.map(|q| q.to_lowercase());

    let entries = fs::read_dir(&validated_path)
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    let mut results: Vec<MentionFile> = Vec::new();

    for entry in entries.flatten() {
        if results.len() >= max {
            break;
        }

        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files
        if name.starts_with('.') {
            continue;
        }

        // Filter by query if provided
        if let Some(ref q) = query_lower {
            if !name.to_lowercase().contains(q) {
                continue;
            }
        }

        let is_directory = path.is_dir();

        results.push(MentionFile {
            path: path.display().to_string(),
            name,
            is_directory,
        });
    }

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
