//! ReAct Agent Loop for Chat
//!
//! Implements the Reason + Act loop pattern:
//! 1. LLM reasons about the query
//! 2. LLM decides to call a tool (or respond)
//! 3. Tool is executed, result fed back
//! 4. Loop until final response

use crate::ai::chat::context::{hydrate_context, ContextItem, HydratedContext};
use crate::ai::chat::tools::{execute_chat_tool, get_chat_tools, ChatToolResult};
use crate::ai::credentials::CredentialManager;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

/// Maximum ReAct loop iterations
const MAX_ITERATIONS: usize = 8;

/// Delay between API requests (rate limiting)
const REQUEST_DELAY_MS: u64 = 1000;

/// Maximum tokens per response
const MAX_TOKENS: u32 = 4096;

/// Anthropic API URL
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Anthropic API version
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Message in conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
}

/// API Response structure
#[derive(Debug, Deserialize)]
struct ApiResponse {
    content: Vec<Value>,
    stop_reason: Option<String>,
}

/// Run the chat agent loop
///
/// # Arguments
/// * `app` - Tauri app handle for emitting events
/// * `message` - User's message
/// * `context_items` - Drag-dropped/mentioned context
/// * `model` - Model ID ("claude-haiku-4-5" or "claude-sonnet-4-5")
/// * `history` - Previous conversation messages
///
/// # Events Emitted
/// * `chat:thought` - Tool usage
/// * `chat:token` - Response chunk
/// * `chat:complete` - Finished
/// * `chat:error` - Error occurred
pub async fn run_chat_agent(
    app: &AppHandle,
    message: &str,
    context_items: &[ContextItem],
    model: &str,
    history: &[ConversationMessage],
) -> Result<String, String> {
    eprintln!("[ChatAgent] Starting with model: {}", model);
    eprintln!("[ChatAgent] Context items: {}", context_items.len());

    // 1. Get API key
    let api_key = CredentialManager::get_api_key("anthropic")?;

    // 2. Hydrate context (files → text, folders → holograms)
    let hydrated: HydratedContext = hydrate_context(context_items)?;

    // 3. Build system prompt
    let system_prompt = build_chat_system_prompt(&hydrated.system_addition);

    // 4. Build message history
    let mut messages = build_message_history(history, message, &hydrated)?;

    // 5. Get available tools
    let tools = get_chat_tools();

    // 6. Create HTTP client
    let client = Client::new();

    // 7. ReAct Loop
    let mut final_response = String::new();

    for iteration in 0..MAX_ITERATIONS {
        eprintln!(
            "[ChatAgent] Iteration {}/{}",
            iteration + 1,
            MAX_ITERATIONS
        );

        // Rate limiting
        if iteration > 0 {
            sleep(Duration::from_millis(REQUEST_DELAY_MS)).await;
        }

        // Build request
        let request_body = json!({
            "model": model,
            "max_tokens": MAX_TOKENS,
            "system": system_prompt,
            "messages": messages,
            "tools": tools,
        });

        // Send request
        let response = client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, error_text));
        }

        let api_response: ApiResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Process response
        let mut has_tool_use = false;
        let mut tool_results: Vec<Value> = Vec::new();
        let mut assistant_content: Vec<Value> = Vec::new();

        for content_block in &api_response.content {
            match content_block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    let text = content_block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");

                    // Emit text chunks
                    app.emit("chat:token", json!({ "chunk": text }))
                        .map_err(|e| format!("Event emit failed: {}", e))?;

                    final_response.push_str(text);
                    assistant_content.push(content_block.clone());
                }
                Some("tool_use") => {
                    has_tool_use = true;

                    let tool_id = content_block
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("unknown");
                    let tool_name = content_block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");
                    let tool_input = content_block.get("input").cloned().unwrap_or(json!({}));

                    // Emit thought step (running)
                    app.emit(
                        "chat:thought",
                        json!({
                            "id": tool_id,
                            "tool": tool_name,
                            "input": format!("{:?}", tool_input),
                            "status": "running",
                            "timestamp": chrono::Utc::now().timestamp_millis(),
                        }),
                    )
                    .ok();

                    // Execute tool
                    let result = execute_chat_tool(tool_name, &tool_input).await;

                    // Emit result
                    let (result_content, is_error) = match &result {
                        ChatToolResult::Success(s) => (s.clone(), false),
                        ChatToolResult::Error(e) => (e.clone(), true),
                    };

                    app.emit(
                        "chat:thought",
                        json!({
                            "id": tool_id,
                            "tool": tool_name,
                            "output": &result_content[..result_content.len().min(500)],
                            "status": if is_error { "error" } else { "complete" },
                        }),
                    )
                    .ok();

                    assistant_content.push(content_block.clone());
                    tool_results.push(json!({
                        "type": "tool_result",
                        "tool_use_id": tool_id,
                        "content": result_content,
                        "is_error": is_error,
                    }));
                }
                _ => {
                    assistant_content.push(content_block.clone());
                }
            }
        }

        // Add assistant message to history
        messages.push(json!({
            "role": "assistant",
            "content": assistant_content,
        }));

        // If tool was used, add results and continue loop
        if has_tool_use && !tool_results.is_empty() {
            messages.push(json!({
                "role": "user",
                "content": tool_results,
            }));
        }

        // Check stop condition
        if api_response.stop_reason == Some("end_turn".to_string()) && !has_tool_use {
            eprintln!(
                "[ChatAgent] Completed after {} iterations",
                iteration + 1
            );
            break;
        }
    }

    // 8. Emit completion
    app.emit("chat:complete", json!({}))
        .map_err(|e| format!("Event emit failed: {}", e))?;

    Ok(final_response)
}

/// Build the chat system prompt
fn build_chat_system_prompt(context_addition: &str) -> String {
    format!(
        r#"You are Sentinel Chat, an intelligent assistant for file management and organization.

## Capabilities
- Search files semantically using the `search_hybrid` tool
- Read file contents using the `read_file` tool
- Inspect folder patterns using the `inspect_pattern` tool
- List directory contents using the `list_directory` tool
- Answer questions about the user's filesystem

## Guidelines
1. Use tools to gather information before answering
2. Be concise and helpful
3. When searching, explain what you're looking for
4. Cite specific files when referencing content
5. You are READ-ONLY - do not suggest making changes without explicit user request

## Security
- You can only access files the user has explicitly shared or that are in their allowed directories
- Never attempt to access system files or sensitive directories
{}"#,
        context_addition
    )
}

/// Build message history for API request
fn build_message_history(
    history: &[ConversationMessage],
    current_message: &str,
    hydrated: &HydratedContext,
) -> Result<Vec<Value>, String> {
    let mut messages: Vec<Value> = Vec::new();

    // Add previous messages (limit to last 20)
    let start = if history.len() > 20 {
        history.len() - 20
    } else {
        0
    };
    for msg in &history[start..] {
        messages.push(json!({
            "role": msg.role,
            "content": msg.content,
        }));
    }

    // Add current user message
    // If there are images, use multimodal format
    if hydrated.images.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": current_message,
        }));
    } else {
        let mut content: Vec<Value> = Vec::new();

        // Add images first
        for img in &hydrated.images {
            content.push(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": img.mime_type,
                    "data": img.base64,
                }
            }));
        }

        // Add text
        content.push(json!({
            "type": "text",
            "text": current_message,
        }));

        messages.push(json!({
            "role": "user",
            "content": content,
        }));
    }

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_prompt() {
        let prompt = build_chat_system_prompt("");
        assert!(prompt.contains("Sentinel Chat"));
        assert!(prompt.contains("search_hybrid"));
    }

    #[test]
    fn test_build_system_prompt_with_context() {
        let context = "\n\n## User Context\n\nFile: test.txt";
        let prompt = build_chat_system_prompt(context);
        assert!(prompt.contains("User Context"));
        assert!(prompt.contains("test.txt"));
    }
}
