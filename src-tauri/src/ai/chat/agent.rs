//! ReAct Agent Loop for Chat with Streaming Support
//!
//! Implements the Reason + Act loop pattern:
//! 1. LLM reasons about the query
//! 2. LLM decides to call a tool (or respond)
//! 3. Tool is executed, result fed back
//! 4. Loop until final response
//!
//! Supports true streaming of text responses via SSE parsing.

use crate::ai::chat::context::{hydrate_context, ContextItem, HydratedContext};
use crate::ai::chat::tools::{execute_chat_tool, get_chat_tools, ChatToolResult};
use crate::ai::credentials::CredentialManager;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

/// Maximum ReAct loop iterations
const MAX_ITERATIONS: usize = 8;

/// Delay between API requests (rate limiting)
const REQUEST_DELAY_MS: u64 = 500;

/// Maximum tokens per response (must be > thinking.budget_tokens when extended thinking is enabled)
const MAX_TOKENS: u32 = 16000;

/// Extended thinking budget (max thinking tokens when enabled)
const THINKING_BUDGET: u32 = 10000;

/// Anthropic API URL
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Anthropic API version (updated for extended thinking support)
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Message in conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
}

/// Streaming event types from Anthropic API
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: Value },
    #[serde(rename = "content_block_start")]
    ContentBlockStart { index: usize, content_block: Value },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: Value },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta { delta: Value, usage: Option<Value> },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: Value },
}

/// Run the chat agent loop with streaming
///
/// # Arguments
/// * `app` - Tauri app handle for emitting events
/// * `message` - User's message
/// * `context_items` - Drag-dropped/mentioned context
/// * `model` - Model ID ("claude-haiku-4-5" or "claude-sonnet-4-5")
/// * `history` - Previous conversation messages
/// * `extended_thinking` - Whether to enable extended thinking mode
///
/// # Events Emitted
/// * `chat:thought` - Tool usage
/// * `chat:token` - Response chunk (streamed)
/// * `chat:thinking` - Extended thinking content (if enabled)
/// * `chat:complete` - Finished
/// * `chat:error` - Error occurred
pub async fn run_chat_agent(
    app: &AppHandle,
    message: &str,
    context_items: &[ContextItem],
    model: &str,
    history: &[ConversationMessage],
    extended_thinking: bool,
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

    // 7. ReAct Loop with streaming
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

        // Build request with streaming
        let mut request_body = json!({
            "model": model,
            "max_tokens": MAX_TOKENS,
            "system": system_prompt,
            "messages": messages,
            "tools": tools,
            "stream": true,
        });

        // Conditionally enable extended thinking
        if extended_thinking {
            request_body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": THINKING_BUDGET
            });
            eprintln!("[ChatAgent] Extended thinking enabled with {} budget tokens", THINKING_BUDGET);
        }

        // Send streaming request
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

        // Process streaming response
        let (stop_reason, has_tool_use, assistant_content, tool_results, iteration_text) =
            process_stream(app, response, &mut final_response).await?;

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
        if stop_reason == Some("end_turn".to_string()) && !has_tool_use {
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

/// Process the streaming response from Anthropic API
async fn process_stream(
    app: &AppHandle,
    response: reqwest::Response,
    final_response: &mut String,
) -> Result<(Option<String>, bool, Vec<Value>, Vec<Value>, String), String> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut stop_reason: Option<String> = None;
    let mut has_tool_use = false;
    let mut assistant_content: Vec<Value> = Vec::new();
    let mut tool_results: Vec<Value> = Vec::new();
    let mut iteration_text = String::new();

    // Track current content blocks being built
    let mut current_text_block: Option<String> = None;
    let mut current_tool_block: Option<(String, String, String)> = None; // (id, name, input_json_string)
    let mut current_thinking_block: Option<String> = None; // Extended thinking content
    let mut thinking_signature: Option<String> = None;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
        let chunk_str = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_str);

        // Process complete lines (SSE format)
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim().to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            // Skip empty lines and event type lines
            if line.is_empty() || line.starts_with("event:") {
                continue;
            }

            // Parse data lines
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    continue;
                }

                match serde_json::from_str::<StreamEvent>(data) {
                    Ok(event) => {
                        match event {
                            StreamEvent::ContentBlockStart { content_block, .. } => {
                                let block_type = content_block
                                    .get("type")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("");

                                match block_type {
                                    "thinking" => {
                                        // Extended thinking block started
                                        current_thinking_block = Some(String::new());
                                        eprintln!("[ChatAgent] Extended thinking started");

                                        // Emit thinking started event
                                        app.emit(
                                            "chat:thinking",
                                            json!({
                                                "status": "started",
                                                "timestamp": chrono::Utc::now().timestamp_millis(),
                                            }),
                                        )
                                        .ok();
                                    }
                                    "text" => {
                                        current_text_block = Some(String::new());
                                    }
                                    "tool_use" => {
                                        has_tool_use = true;
                                        let id = content_block
                                            .get("id")
                                            .and_then(|i| i.as_str())
                                            .unwrap_or("unknown")
                                            .to_string();
                                        let name = content_block
                                            .get("name")
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("unknown")
                                            .to_string();

                                        // Emit thought step (running) - input will be updated when complete
                                        app.emit(
                                            "chat:thought",
                                            json!({
                                                "id": &id,
                                                "tool": &name,
                                                "input": "",  // Placeholder until we have full input
                                                "status": "running",
                                                "timestamp": chrono::Utc::now().timestamp_millis(),
                                            }),
                                        )
                                        .ok();

                                        current_tool_block = Some((id, name, String::new()));
                                    }
                                    _ => {}
                                }
                            }
                            StreamEvent::ContentBlockDelta { delta, .. } => {
                                let delta_type = delta
                                    .get("type")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("");

                                match delta_type {
                                    "thinking_delta" => {
                                        // Extended thinking content streaming
                                        if let Some(thinking) =
                                            delta.get("thinking").and_then(|t| t.as_str())
                                        {
                                            // Emit thinking chunk for streaming
                                            app.emit(
                                                "chat:thinking",
                                                json!({
                                                    "status": "streaming",
                                                    "chunk": thinking,
                                                }),
                                            )
                                            .ok();

                                            // Accumulate thinking
                                            if let Some(ref mut block) = current_thinking_block {
                                                block.push_str(thinking);
                                            }
                                        }
                                    }
                                    "signature_delta" => {
                                        // Thinking block signature (for verification)
                                        if let Some(sig) =
                                            delta.get("signature").and_then(|s| s.as_str())
                                        {
                                            if let Some(ref mut sig_acc) = thinking_signature {
                                                sig_acc.push_str(sig);
                                            } else {
                                                thinking_signature = Some(sig.to_string());
                                            }
                                        }
                                    }
                                    "text_delta" => {
                                        if let Some(text) =
                                            delta.get("text").and_then(|t| t.as_str())
                                        {
                                            // Emit text chunk for streaming
                                            app.emit("chat:token", json!({ "chunk": text })).ok();

                                            // Accumulate text
                                            if let Some(ref mut block) = current_text_block {
                                                block.push_str(text);
                                            }
                                            final_response.push_str(text);
                                            iteration_text.push_str(text);
                                        }
                                    }
                                    "input_json_delta" => {
                                        if let Some((_, _, ref mut input_json)) = current_tool_block {
                                            if let Some(partial) =
                                                delta.get("partial_json").and_then(|p| p.as_str())
                                            {
                                                // Accumulate JSON string fragments
                                                input_json.push_str(partial);
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            StreamEvent::ContentBlockStop { .. } => {
                                // Finalize thinking block
                                if let Some(thinking) = current_thinking_block.take() {
                                    let sig = thinking_signature.take();
                                    eprintln!(
                                        "[ChatAgent] Extended thinking completed: {} chars",
                                        thinking.len()
                                    );

                                    // Emit thinking completed event
                                    app.emit(
                                        "chat:thinking",
                                        json!({
                                            "status": "complete",
                                            "content": &thinking,
                                        }),
                                    )
                                    .ok();

                                    // Add to assistant content for conversation history
                                    // Note: Thinking blocks should be preserved for context
                                    assistant_content.push(json!({
                                        "type": "thinking",
                                        "thinking": thinking,
                                        "signature": sig.unwrap_or_default(),
                                    }));
                                }

                                // Finalize text block
                                if let Some(text) = current_text_block.take() {
                                    assistant_content.push(json!({
                                        "type": "text",
                                        "text": text,
                                    }));
                                }

                                // Finalize and execute tool block
                                if let Some((id, name, input_json)) = current_tool_block.take() {
                                    // Parse the accumulated JSON string
                                    let tool_input: Value = if input_json.is_empty() {
                                        json!({})
                                    } else {
                                        serde_json::from_str(&input_json).unwrap_or_else(|e| {
                                            eprintln!("[ChatAgent] Failed to parse tool input JSON: {} - raw: {}", e, &input_json[..input_json.len().min(200)]);
                                            json!({})
                                        })
                                    };

                                    eprintln!("[ChatAgent] Tool '{}' input parsed: {:?}", name, tool_input);

                                    // Add to assistant content
                                    assistant_content.push(json!({
                                        "type": "tool_use",
                                        "id": &id,
                                        "name": &name,
                                        "input": &tool_input,
                                    }));

                                    // Execute tool
                                    let result = execute_chat_tool(&name, &tool_input).await;

                                    // Emit result
                                    let (result_content, is_error) = match &result {
                                        ChatToolResult::Success(s) => (s.clone(), false),
                                        ChatToolResult::Error(e) => (e.clone(), true),
                                    };

                                    // Format input for display
                                    let input_display = tool_input.to_string();
                                    let input_display = if input_display.len() > 200 {
                                        format!("{}...", &input_display[..200])
                                    } else {
                                        input_display
                                    };

                                    app.emit(
                                        "chat:thought",
                                        json!({
                                            "id": &id,
                                            "tool": &name,
                                            "input": input_display,
                                            "output": &result_content[..result_content.len().min(500)],
                                            "status": if is_error { "error" } else { "complete" },
                                            "timestamp": chrono::Utc::now().timestamp_millis(),
                                        }),
                                    )
                                    .ok();

                                    tool_results.push(json!({
                                        "type": "tool_result",
                                        "tool_use_id": &id,
                                        "content": result_content,
                                        "is_error": is_error,
                                    }));
                                }
                            }
                            StreamEvent::MessageDelta { delta, .. } => {
                                if let Some(reason) =
                                    delta.get("stop_reason").and_then(|r| r.as_str())
                                {
                                    stop_reason = Some(reason.to_string());
                                }
                            }
                            StreamEvent::MessageStop => {
                                // Message complete
                            }
                            StreamEvent::Error { error } => {
                                let error_msg = error
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("Unknown streaming error");
                                return Err(format!("Stream error: {}", error_msg));
                            }
                            _ => {}
                        }
                    }
                    Err(_) => {
                        // Skip malformed JSON - common during streaming
                    }
                }
            }
        }
    }

    Ok((
        stop_reason,
        has_tool_use,
        assistant_content,
        tool_results,
        iteration_text,
    ))
}

/// Build the chat system prompt
fn build_chat_system_prompt(context_addition: &str) -> String {
    format!(
        r#"You are Sentinel Chat, an intelligent assistant for file management and organization.

## Tools Available
- **search_hybrid**: Semantic + keyword search in files
- **read_file**: Read file contents
- **list_directory**: List directory contents
- **inspect_pattern**: Sample files matching a regex pattern
- **bash**: Execute shell commands (ls, find, cat, head, wc, git status, etc.)
- **grep**: Search file contents with regex (uses ripgrep for speed)

## Guidelines
1. Use `grep` for searching inside file contents
2. Use `bash` with `ls -la` or `find` for exploring directories
3. Use `read_file` for reading specific file contents
4. Cite file paths when referencing content
5. You are READ-ONLY - never suggest rm, mv, or destructive commands
6. Be concise and helpful

## Security
- Only access files the user has shared or in allowed directories
- Destructive commands are blocked for safety
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
