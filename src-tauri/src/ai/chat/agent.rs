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
use crate::ai::http_client::anthropic_client;
use futures::StreamExt;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Cached tool definitions to avoid repeated JSON generation
///
/// Tool definitions are static and don't change during runtime, so we cache
/// them once at first use. This saves ~50us per request from repeated
/// serde_json serialization.
static CACHED_TOOLS: Lazy<Vec<Value>> = Lazy::new(|| {
    debug!("Initializing cached tool definitions");
    get_chat_tools()
});

/// Helper macro to emit events with proper error logging
/// Logs failures but doesn't propagate errors (we don't want to crash streams on emit failures)
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

/// Maximum tokens per response (must be > thinking.budget_tokens when extended thinking is enabled)
const MAX_TOKENS: u32 = 16000;

/// Extended thinking budget (max thinking tokens when enabled)
const THINKING_BUDGET: u32 = 10000;

/// Maximum buffer sizes to prevent OOM from malformed API responses
const MAX_SSE_BUFFER_SIZE: usize = 1_000_000; // 1MB for SSE line buffer
const MAX_TEXT_BLOCK_SIZE: usize = 500_000; // 500KB for text accumulation
const MAX_THINKING_BLOCK_SIZE: usize = 1_000_000; // 1MB for thinking (can be large)
const MAX_TOOL_INPUT_SIZE: usize = 100_000; // 100KB for tool input JSON
const MAX_FINAL_RESPONSE_SIZE: usize = 2_000_000; // 2MB for final accumulated response

/// Anthropic API URL
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Anthropic API version (updated for extended thinking support)
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Token batching configuration
/// Batches token emissions to reduce IPC overhead (~90% fewer events)
const TOKEN_BATCH_WINDOW_MS: u64 = 16; // ~60fps update rate
const TOKEN_BATCH_MAX_CHARS: usize = 50; // Flush when buffer exceeds this

/// Token batcher for reducing streaming event frequency
/// Instead of emitting for every token (~500 per response),
/// batches them and emits every 16ms or 50 chars (~30-50 events)
struct TokenBatcher {
    buffer: String,
    last_emit: std::time::Instant,
}

impl TokenBatcher {
    fn new() -> Self {
        Self {
            buffer: String::with_capacity(256),
            last_emit: std::time::Instant::now(),
        }
    }

    /// Add a chunk to the buffer, potentially flushing if threshold met
    fn add(&mut self, chunk: &str, app: &AppHandle) {
        self.buffer.push_str(chunk);

        let should_flush = self.buffer.len() >= TOKEN_BATCH_MAX_CHARS
            || self.last_emit.elapsed() > Duration::from_millis(TOKEN_BATCH_WINDOW_MS);

        if should_flush && !self.buffer.is_empty() {
            emit_logged!(app, "chat:token", json!({ "chunk": &self.buffer }));
            self.buffer.clear();
            self.last_emit = std::time::Instant::now();
        }
    }

    /// Flush any remaining content in the buffer
    fn flush(&mut self, app: &AppHandle) {
        if !self.buffer.is_empty() {
            emit_logged!(app, "chat:token", json!({ "chunk": &self.buffer }));
            self.buffer.clear();
        }
    }
}

/// Message in conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    /// Previous attachments (files, folders, images) from this message
    #[serde(default)]
    pub context_items: Vec<ContextItem>,
}

/// Token usage tracking from API response
#[derive(Debug, Clone, Default, Serialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

/// Chat agent result with response and accurate token usage
#[derive(Debug)]
pub struct ChatAgentResult {
    pub response: String,
    pub usage: TokenUsage,
}

/// Streaming event types from Anthropic API
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: Value },
    #[serde(rename = "content_block_start")]
    ContentBlockStart { #[allow(dead_code)] index: usize, content_block: Value },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { #[allow(dead_code)] index: usize, delta: Value },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { #[allow(dead_code)] index: usize },
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
/// * `abort_flag` - Optional flag to signal abort request
///
/// # Returns
/// * `ChatAgentResult` - Contains the response text and accurate token usage from API
///
/// # Events Emitted
/// * `chat:thought` - Tool usage
/// * `chat:token` - Response chunk (streamed)
/// * `chat:thinking` - Extended thinking content (if enabled)
/// * `chat:complete` - Finished
/// * `chat:error` - Error occurred
/// * `chat:aborted` - Aborted by user
pub async fn run_chat_agent(
    app: &AppHandle,
    message: &str,
    context_items: &[ContextItem],
    model: &str,
    history: &[ConversationMessage],
    extended_thinking: bool,
    abort_flag: Option<Arc<AtomicBool>>,
) -> Result<ChatAgentResult, String> {
    info!(model = model, context_items = context_items.len(), "Starting chat agent");

    // Track total token usage across all iterations
    let mut total_usage = TokenUsage::default();

    // Helper to check if abort was requested
    let is_aborted = || -> bool {
        abort_flag
            .as_ref()
            .map(|f| f.load(Ordering::SeqCst))
            .unwrap_or(false)
    };

    // Check abort at start
    if is_aborted() {
        info!("Chat aborted before starting");
        emit_logged!(app, "chat:aborted", json!({"reason": "User requested abort"}));
        return Ok(ChatAgentResult {
            response: String::new(),
            usage: total_usage,
        });
    }

    // 1. Get API key
    let api_key = CredentialManager::get_api_key("anthropic")?;

    // 2. Hydrate context (files → text, folders → holograms)
    let hydrated: HydratedContext = hydrate_context(context_items)?;

    // 3. Collect previous context references from history
    let previous_context = collect_previous_context(history);
    if !previous_context.is_empty() {
        debug!("Found previous context from conversation history");
    }

    // 4. Build system prompt with previous context
    let system_prompt = build_chat_system_prompt(&hydrated.system_addition, &previous_context);

    // 5. Build message history
    let mut messages = build_message_history(history, message, &hydrated)?;

    // 6. Get cached tool definitions (lazy-initialized singleton)
    let tools = &*CACHED_TOOLS;

    // 7. Use shared HTTP client with connection pooling
    let client = anthropic_client();

    // 8. ReAct Loop with streaming
    let mut final_response = String::new();

    for iteration in 0..MAX_ITERATIONS {
        // Check abort at start of each iteration
        if is_aborted() {
            info!(iteration = iteration + 1, "Chat aborted");
            emit_logged!(app, "chat:aborted", json!({"reason": "User requested abort"}));
            return Ok(ChatAgentResult {
                response: final_response,
                usage: total_usage,
            });
        }

        debug!(iteration = iteration + 1, max = MAX_ITERATIONS, "ReAct iteration");

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
            debug!(budget_tokens = THINKING_BUDGET, "Extended thinking enabled");
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

        // Process streaming response (returns usage for this iteration)
        let (stop_reason, has_tool_use, assistant_content, tool_results, _iteration_text, iteration_usage) =
            process_stream(app, response, &mut final_response).await?;

        // Accumulate token usage from this iteration
        total_usage.input_tokens += iteration_usage.input_tokens;
        total_usage.output_tokens += iteration_usage.output_tokens;
        total_usage.cache_creation_input_tokens += iteration_usage.cache_creation_input_tokens;
        total_usage.cache_read_input_tokens += iteration_usage.cache_read_input_tokens;

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
            info!(iterations = iteration + 1, "Chat completed");
            break;
        }
    }

    // 9. Emit completion
    app.emit("chat:complete", json!({}))
        .map_err(|e| format!("Event emit failed: {}", e))?;

    info!(
        input_tokens = total_usage.input_tokens,
        output_tokens = total_usage.output_tokens,
        cache_created = total_usage.cache_creation_input_tokens,
        cache_read = total_usage.cache_read_input_tokens,
        "Chat agent finished"
    );

    Ok(ChatAgentResult {
        response: final_response,
        usage: total_usage,
    })
}

/// Initial SSE buffer capacity (16KB - typical for streaming responses)
const INITIAL_SSE_BUFFER_CAPACITY: usize = 16_384;

/// Initial text block capacity (8KB - typical response paragraph)
const INITIAL_TEXT_BLOCK_CAPACITY: usize = 8_192;

/// Initial thinking block capacity (32KB - thinking can be verbose)
const INITIAL_THINKING_BLOCK_CAPACITY: usize = 32_768;

/// Process the streaming response from Anthropic API
/// Returns: (stop_reason, has_tool_use, assistant_content, tool_results, iteration_text, usage)
async fn process_stream(
    app: &AppHandle,
    response: reqwest::Response,
    final_response: &mut String,
) -> Result<(Option<String>, bool, Vec<Value>, Vec<Value>, String, TokenUsage), String> {
    let mut stream = response.bytes_stream();

    // Pre-allocate buffer with typical capacity to reduce reallocations
    let mut buffer = String::with_capacity(INITIAL_SSE_BUFFER_CAPACITY);
    let mut stop_reason: Option<String> = None;
    let mut has_tool_use = false;
    let mut assistant_content: Vec<Value> = Vec::with_capacity(4); // Typical: 1-3 content blocks
    let mut tool_results: Vec<Value> = Vec::with_capacity(2); // Typical: 0-2 tool calls
    let mut iteration_text = String::with_capacity(INITIAL_TEXT_BLOCK_CAPACITY);

    // Track token usage from API response
    let mut usage = TokenUsage::default();

    // Track current content blocks being built (with pre-allocation)
    let mut current_text_block: Option<String> = None;
    let mut current_tool_block: Option<(String, String, String)> = None; // (id, name, input_json_string)
    let mut current_thinking_block: Option<String> = None; // Extended thinking content
    let mut thinking_signature: Option<String> = None;

    // Token batcher for reducing event frequency (~90% fewer IPC calls)
    let mut token_batcher = TokenBatcher::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
        let chunk_str = String::from_utf8_lossy(&chunk);

        // Bounded buffer check to prevent OOM from malformed responses
        if buffer.len() + chunk_str.len() > MAX_SSE_BUFFER_SIZE {
            return Err(format!(
                "SSE buffer exceeded maximum size of {} bytes (possible malformed response)",
                MAX_SSE_BUFFER_SIZE
            ));
        }
        buffer.push_str(&chunk_str);

        // Process complete lines (SSE format) - zero-copy where possible
        while let Some(newline_pos) = buffer.find('\n') {
            // Extract line without allocation for simple checks
            let line_slice = buffer[..newline_pos].trim();

            // Skip empty lines and event type lines (no allocation needed)
            if line_slice.is_empty() || line_slice.starts_with("event:") {
                // Drain processed portion efficiently
                buffer.drain(..=newline_pos);
                continue;
            }

            // Parse data lines - clone data before draining buffer
            let data_opt = line_slice.strip_prefix("data: ").map(|s| s.to_string());

            // Drain the processed line from buffer (single operation vs two string allocations)
            buffer.drain(..=newline_pos);

            if let Some(data) = data_opt {
                if data == "[DONE]" {
                    continue;
                }

                match serde_json::from_str::<StreamEvent>(&data) {
                    Ok(event) => {
                        match event {
                            StreamEvent::ContentBlockStart { content_block, .. } => {
                                let block_type = content_block
                                    .get("type")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("");

                                match block_type {
                                    "thinking" => {
                                        // Extended thinking block started - pre-allocate for typical size
                                        current_thinking_block = Some(String::with_capacity(INITIAL_THINKING_BLOCK_CAPACITY));
                                        debug!("Extended thinking started");

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
                                        // Pre-allocate text block for typical paragraph size
                                        current_text_block = Some(String::with_capacity(INITIAL_TEXT_BLOCK_CAPACITY));
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
                                            emit_logged!(app, "chat:thinking", json!({
                                                "status": "streaming",
                                                "chunk": thinking,
                                            }));

                                            // Accumulate thinking with bounds check
                                            if let Some(ref mut block) = current_thinking_block {
                                                if block.len() + thinking.len() <= MAX_THINKING_BLOCK_SIZE {
                                                    block.push_str(thinking);
                                                } else {
                                                    warn!(max_size = MAX_THINKING_BLOCK_SIZE, "Thinking block truncated");
                                                }
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
                                            // Batch text chunks to reduce IPC overhead
                                            // Emits every 16ms or 50 chars instead of per-token
                                            token_batcher.add(text, app);

                                            // Accumulate text with bounds checks
                                            if let Some(ref mut block) = current_text_block {
                                                if block.len() + text.len() <= MAX_TEXT_BLOCK_SIZE {
                                                    block.push_str(text);
                                                }
                                            }
                                            if final_response.len() + text.len() <= MAX_FINAL_RESPONSE_SIZE {
                                                final_response.push_str(text);
                                            } else {
                                                warn!(max_size = MAX_FINAL_RESPONSE_SIZE, "Response truncated");
                                            }
                                            iteration_text.push_str(text);
                                        }
                                    }
                                    "input_json_delta" => {
                                        if let Some((_, _, ref mut input_json)) = current_tool_block {
                                            if let Some(partial) =
                                                delta.get("partial_json").and_then(|p| p.as_str())
                                            {
                                                // Accumulate JSON string fragments with bounds check
                                                if input_json.len() + partial.len() <= MAX_TOOL_INPUT_SIZE {
                                                    input_json.push_str(partial);
                                                } else {
                                                    warn!(max_size = MAX_TOOL_INPUT_SIZE, "Tool input truncated");
                                                }
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
                                    debug!(chars = thinking.len(), "Extended thinking completed");

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
                                            warn!(error = %e, "Failed to parse tool input JSON");
                                            json!({})
                                        })
                                    };

                                    debug!(tool = name, input = ?tool_input, "Tool input parsed");

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

                                    // Format input for display (UTF-8 safe truncation)
                                    let input_display = tool_input.to_string();
                                    let input_display = if input_display.len() > 200 {
                                        let truncate_at = (0..=200).rev().find(|&i| input_display.is_char_boundary(i)).unwrap_or(0);
                                        format!("{}...", &input_display[..truncate_at])
                                    } else {
                                        input_display
                                    };

                                    // UTF-8 safe output truncation
                                    let output_max = result_content.len().min(500);
                                    let output_truncate_at = (0..=output_max).rev().find(|&i| result_content.is_char_boundary(i)).unwrap_or(0);

                                    app.emit(
                                        "chat:thought",
                                        json!({
                                            "id": &id,
                                            "tool": &name,
                                            "input": input_display,
                                            "output": &result_content[..output_truncate_at],
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
                            StreamEvent::MessageStart { message } => {
                                // Extract input token usage from message_start
                                if let Some(msg_usage) = message.get("usage") {
                                    if let Some(input) = msg_usage.get("input_tokens").and_then(|t| t.as_u64()) {
                                        usage.input_tokens = input;
                                    }
                                    if let Some(cache_creation) = msg_usage.get("cache_creation_input_tokens").and_then(|t| t.as_u64()) {
                                        usage.cache_creation_input_tokens = cache_creation;
                                    }
                                    if let Some(cache_read) = msg_usage.get("cache_read_input_tokens").and_then(|t| t.as_u64()) {
                                        usage.cache_read_input_tokens = cache_read;
                                    }
                                }
                            }
                            StreamEvent::MessageDelta { delta, usage: delta_usage } => {
                                // Extract stop reason
                                if let Some(reason) =
                                    delta.get("stop_reason").and_then(|r| r.as_str())
                                {
                                    stop_reason = Some(reason.to_string());
                                }

                                // Extract output token usage from message_delta
                                if let Some(msg_usage) = delta_usage {
                                    if let Some(output) = msg_usage.get("output_tokens").and_then(|t| t.as_u64()) {
                                        usage.output_tokens = output;
                                    }
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

    // Flush any remaining batched tokens
    token_batcher.flush(app);

    Ok((
        stop_reason,
        has_tool_use,
        assistant_content,
        tool_results,
        iteration_text,
        usage,
    ))
}

/// Build the chat system prompt
fn build_chat_system_prompt(context_addition: &str, previous_context: &str) -> String {
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
    let mut context_refs: Vec<(String, String)> = Vec::new(); // (name, path)

    // Collect context from recent user messages (last 10 messages)
    for msg in history.iter().rev().take(10) {
        if msg.role == "user" && !msg.context_items.is_empty() {
            for item in &msg.context_items {
                // Avoid duplicates
                if !context_refs.iter().any(|(_, p)| p == &item.path) {
                    context_refs.push((item.name.clone(), item.path.clone()));
                }
            }
        }
    }

    if context_refs.is_empty() {
        return String::new();
    }

    // Build a section informing the AI about previously attached files
    let mut section = String::from("\n\n## Previously Attached Files\n");
    section.push_str("The user has attached these files in earlier messages in this conversation. ");
    section.push_str("Use the `read_file` tool if you need to access their content:\n\n");

    for (name, path) in &context_refs {
        section.push_str(&format!("- `{}` at `{}`\n", name, path));
    }

    section
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
        let prompt = build_chat_system_prompt("", "");
        assert!(prompt.contains("Sentinel Chat"));
        assert!(prompt.contains("search_hybrid"));
    }

    #[test]
    fn test_build_system_prompt_with_context() {
        let context = "\n\n## User Context\n\nFile: test.txt";
        let prompt = build_chat_system_prompt(context, "");
        assert!(prompt.contains("User Context"));
        assert!(prompt.contains("test.txt"));
    }

    #[test]
    fn test_build_system_prompt_with_previous_context() {
        let previous = "\n\n## Previously Attached Files\n- `doc.pdf` at `/path/doc.pdf`\n";
        let prompt = build_chat_system_prompt("", previous);
        assert!(prompt.contains("Previously Attached Files"));
        assert!(prompt.contains("doc.pdf"));
    }

    #[test]
    fn test_collect_previous_context() {
        let history = vec![
            ConversationMessage {
                role: "user".to_string(),
                content: "analyze this file".to_string(),
                context_items: vec![ContextItem {
                    id: "1".to_string(),
                    item_type: "file".to_string(),
                    path: "/docs/report.pdf".to_string(),
                    name: "report.pdf".to_string(),
                    strategy: "read".to_string(),
                    size: Some(1024),
                    mime_type: Some("application/pdf".to_string()),
                }],
            },
            ConversationMessage {
                role: "assistant".to_string(),
                content: "I analyzed the file".to_string(),
                context_items: vec![],
            },
            ConversationMessage {
                role: "user".to_string(),
                content: "what about page 5?".to_string(),
                context_items: vec![],
            },
        ];

        let context = collect_previous_context(&history);
        assert!(context.contains("Previously Attached Files"));
        assert!(context.contains("report.pdf"));
        assert!(context.contains("/docs/report.pdf"));
    }

    #[test]
    fn test_collect_previous_context_empty() {
        let history = vec![
            ConversationMessage {
                role: "user".to_string(),
                content: "hello".to_string(),
                context_items: vec![],
            },
        ];

        let context = collect_previous_context(&history);
        assert!(context.is_empty());
    }
}
