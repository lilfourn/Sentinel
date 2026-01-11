//! OpenAI Chat Provider
//!
//! Implements the ReAct agent loop for OpenAI GPT models.
//! Handles streaming responses and tool/function calling.

use crate::ai::chat::context::{hydrate_context, ContextItem, HydratedContext};
use crate::ai::chat::tool_conversion::{
    parse_openai_tool_call, tool_result_to_openai_message, tools_to_openai_format,
};
use crate::ai::chat::tools::{execute_chat_tool, get_chat_tools, ChatToolResult};
use crate::ai::credentials::CredentialManager;
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use super::agent::{ChatAgentResult, ConversationMessage, TokenUsage};

/// Helper macro to emit events with proper error logging
macro_rules! emit_logged {
    ($app:expr, $event:expr, $payload:expr) => {
        if let Err(e) = $app.emit($event, $payload) {
            warn!(event = $event, error = %e, "Failed to emit event");
        }
    };
}

/// Maximum ReAct loop iterations
const MAX_ITERATIONS: usize = 8;

/// Delay between API requests (rate limiting)
const REQUEST_DELAY_MS: u64 = 500;

/// Maximum tokens per response
const MAX_TOKENS: u32 = 16000;

/// Maximum buffer sizes
const MAX_SSE_BUFFER_SIZE: usize = 1_000_000;
const MAX_TEXT_BLOCK_SIZE: usize = 500_000;
const MAX_FINAL_RESPONSE_SIZE: usize = 2_000_000;

/// OpenAI API URL
const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

/// OpenAI streaming chunk structure
#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    choices: Vec<OpenAIChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    delta: OpenAIDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAIDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAIFunctionDelta>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAIFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAIUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    #[allow(dead_code)]
    total_tokens: Option<u64>,
}

/// Run the OpenAI chat agent loop with streaming
pub async fn run_openai_chat_agent(
    app: &AppHandle,
    message: &str,
    context_items: &[ContextItem],
    model: &str,
    history: &[ConversationMessage],
    abort_flag: Option<Arc<AtomicBool>>,
) -> Result<ChatAgentResult, String> {
    info!(model = model, context_items = context_items.len(), "Starting OpenAI chat agent");

    let mut total_usage = TokenUsage::default();

    let is_aborted = || -> bool {
        abort_flag
            .as_ref()
            .map(|f| f.load(Ordering::SeqCst))
            .unwrap_or(false)
    };

    if is_aborted() {
        info!("Chat aborted before starting");
        emit_logged!(app, "chat:aborted", json!({"reason": "User requested abort"}));
        return Ok(ChatAgentResult {
            response: String::new(),
            usage: total_usage,
        });
    }

    // Get API key (checks compile-time embedded key, runtime env, and keychain)
    let api_key = CredentialManager::get_api_key("openai")
        .map_err(|_| "OpenAI API key not configured. GPT models require an API key.".to_string())?;

    // Hydrate context
    let hydrated: HydratedContext = hydrate_context(context_items)?;

    // Build message history
    let mut messages = build_openai_message_history(history, message, &hydrated)?;

    // Get tools in OpenAI format
    let claude_tools = get_chat_tools();
    let tools = tools_to_openai_format(&claude_tools);

    // Create HTTP client
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut final_response = String::new();

    for iteration in 0..MAX_ITERATIONS {
        if is_aborted() {
            info!(iteration = iteration + 1, "Chat aborted");
            emit_logged!(app, "chat:aborted", json!({"reason": "User requested abort"}));
            return Ok(ChatAgentResult {
                response: final_response,
                usage: total_usage,
            });
        }

        debug!(iteration = iteration + 1, max = MAX_ITERATIONS, "OpenAI ReAct iteration");

        if iteration > 0 {
            sleep(Duration::from_millis(REQUEST_DELAY_MS)).await;
        }

        // Build request
        let request_body = json!({
            "model": model,
            "max_completion_tokens": MAX_TOKENS,
            "messages": messages,
            "tools": tools,
            "stream": true,
            "stream_options": { "include_usage": true }
        });

        // Send streaming request
        let response = client
            .post(OPENAI_API_URL)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("OpenAI API error {}: {}", status, error_text));
        }

        // Process streaming response
        let (finish_reason, tool_calls, text_content, iteration_usage) =
            process_openai_stream(app, response, &mut final_response).await?;

        // Accumulate usage
        total_usage.input_tokens += iteration_usage.input_tokens;
        total_usage.output_tokens += iteration_usage.output_tokens;

        // If there were tool calls, execute them
        if !tool_calls.is_empty() {
            // Add assistant message with tool calls
            messages.push(json!({
                "role": "assistant",
                "tool_calls": tool_calls
            }));

            // Execute each tool and add results
            for tool_call in &tool_calls {
                let (id, name, args) = parse_openai_tool_call(tool_call);

                debug!(tool = name, id = id, "Executing tool");

                let result = execute_chat_tool(&name, &args).await;

                let (result_content, is_error) = match &result {
                    ChatToolResult::Success(s) => (s.clone(), false),
                    ChatToolResult::Error(e) => (e.clone(), true),
                };

                // Emit thought event
                let input_display = args.to_string();
                let input_display = if input_display.len() > 200 {
                    format!("{}...", &input_display[..200])
                } else {
                    input_display
                };

                emit_logged!(
                    app,
                    "chat:thought",
                    json!({
                        "id": &id,
                        "tool": &name,
                        "input": input_display,
                        "output": &result_content[..result_content.len().min(500)],
                        "status": if is_error { "error" } else { "complete" },
                        "timestamp": chrono::Utc::now().timestamp_millis(),
                    })
                );

                // Add tool result message
                messages.push(tool_result_to_openai_message(&id, &result_content, is_error));
            }

            // Continue loop to get response after tool results
            continue;
        }

        // No tool calls - add text response to history
        if !text_content.is_empty() {
            messages.push(json!({
                "role": "assistant",
                "content": text_content
            }));
        }

        // Check if done
        if finish_reason == Some("stop".to_string()) {
            info!(iterations = iteration + 1, "OpenAI chat completed");
            break;
        }
    }

    // Emit completion
    app.emit("chat:complete", json!({}))
        .map_err(|e| format!("Event emit failed: {}", e))?;

    info!(
        input_tokens = total_usage.input_tokens,
        output_tokens = total_usage.output_tokens,
        "OpenAI chat agent finished"
    );

    Ok(ChatAgentResult {
        response: final_response,
        usage: total_usage,
    })
}

/// Process OpenAI streaming response
/// Returns: (finish_reason, tool_calls, text_content, usage)
async fn process_openai_stream(
    app: &AppHandle,
    response: reqwest::Response,
    final_response: &mut String,
) -> Result<(Option<String>, Vec<Value>, String, TokenUsage), String> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut finish_reason: Option<String> = None;
    let mut text_content = String::new();
    let mut usage = TokenUsage::default();

    // Track tool calls being built (by index)
    let mut tool_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, arguments)

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
        let chunk_str = String::from_utf8_lossy(&chunk);

        if buffer.len() + chunk_str.len() > MAX_SSE_BUFFER_SIZE {
            return Err("SSE buffer exceeded maximum size".to_string());
        }
        buffer.push_str(&chunk_str);

        // Process complete lines
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim().to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            // Parse data lines
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    continue;
                }

                match serde_json::from_str::<OpenAIStreamChunk>(data) {
                    Ok(chunk) => {
                        // Extract usage if present (final chunk)
                        if let Some(u) = chunk.usage {
                            usage.input_tokens = u.prompt_tokens.unwrap_or(0);
                            usage.output_tokens = u.completion_tokens.unwrap_or(0);
                        }

                        for choice in chunk.choices {
                            // Update finish reason
                            if let Some(reason) = choice.finish_reason {
                                finish_reason = Some(reason);
                            }

                            // Handle text content
                            if let Some(content) = choice.delta.content {
                                // Emit token
                                emit_logged!(app, "chat:token", json!({ "chunk": content }));

                                // Accumulate with bounds check
                                if text_content.len() + content.len() <= MAX_TEXT_BLOCK_SIZE {
                                    text_content.push_str(&content);
                                }
                                if final_response.len() + content.len() <= MAX_FINAL_RESPONSE_SIZE {
                                    final_response.push_str(&content);
                                }
                            }

                            // Handle tool calls
                            if let Some(calls) = choice.delta.tool_calls {
                                for call in calls {
                                    let idx = call.index;

                                    // Ensure we have enough slots
                                    while tool_calls.len() <= idx {
                                        tool_calls.push((String::new(), String::new(), String::new()));
                                    }

                                    // Update ID if present
                                    if let Some(id) = call.id {
                                        tool_calls[idx].0 = id;
                                    }

                                    // Update function info
                                    if let Some(func) = call.function {
                                        if let Some(name) = func.name {
                                            tool_calls[idx].1 = name.clone();

                                            // Emit thought step (running)
                                            emit_logged!(
                                                app,
                                                "chat:thought",
                                                json!({
                                                    "id": &tool_calls[idx].0,
                                                    "tool": &name,
                                                    "input": "",
                                                    "status": "running",
                                                    "timestamp": chrono::Utc::now().timestamp_millis(),
                                                })
                                            );
                                        }
                                        if let Some(args) = func.arguments {
                                            tool_calls[idx].2.push_str(&args);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Skip malformed JSON
                    }
                }
            }
        }
    }

    // Convert accumulated tool calls to Value format
    let tool_call_values: Vec<Value> = tool_calls
        .into_iter()
        .filter(|(id, name, _)| !id.is_empty() && !name.is_empty())
        .map(|(id, name, args)| {
            json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": args
                }
            })
        })
        .collect();

    Ok((finish_reason, tool_call_values, text_content, usage))
}

/// Build the OpenAI chat system prompt
fn build_openai_system_prompt(context_addition: &str, history: &[ConversationMessage]) -> String {
    let previous_context = collect_previous_context(history);

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
{}{}"#,
        previous_context, context_addition
    )
}

/// Collect references to files attached in previous messages
fn collect_previous_context(history: &[ConversationMessage]) -> String {
    let mut context_refs: Vec<(String, String)> = Vec::new();

    for msg in history.iter().rev().take(10) {
        if msg.role == "user" && !msg.context_items.is_empty() {
            for item in &msg.context_items {
                if !context_refs.iter().any(|(_, p)| p == &item.path) {
                    context_refs.push((item.name.clone(), item.path.clone()));
                }
            }
        }
    }

    if context_refs.is_empty() {
        return String::new();
    }

    let mut section = String::from("\n\n## Previously Attached Files\n");
    section.push_str("The user has attached these files in earlier messages. ");
    section.push_str("Use the `read_file` tool if you need to access their content:\n\n");

    for (name, path) in &context_refs {
        section.push_str(&format!("- `{}` at `{}`\n", name, path));
    }

    section
}

/// Build message history for OpenAI API request
fn build_openai_message_history(
    history: &[ConversationMessage],
    current_message: &str,
    hydrated: &HydratedContext,
) -> Result<Vec<Value>, String> {
    let mut messages: Vec<Value> = Vec::new();

    // Add system message
    let system_prompt = build_openai_system_prompt(&hydrated.system_addition, history);
    messages.push(json!({
        "role": "system",
        "content": system_prompt
    }));

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
    // Note: OpenAI vision is handled differently, but for now just use text
    if hydrated.images.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": current_message,
        }));
    } else {
        // Build multimodal message with images
        let mut content: Vec<Value> = Vec::new();

        // Add images first
        for img in &hydrated.images {
            content.push(json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:{};base64,{}", img.mime_type, img.base64),
                    "detail": "auto"
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
    fn test_build_openai_system_prompt() {
        let prompt = build_openai_system_prompt("", &[]);
        assert!(prompt.contains("Sentinel Chat"));
        assert!(prompt.contains("search_hybrid"));
    }
}
