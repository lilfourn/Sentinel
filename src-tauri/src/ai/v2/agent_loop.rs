//! V4 Agentic loop for semantic, rule-based file organization.
//!
//! V4 Upgrades (Map-Reduce Architecture):
//! - **Smart Sampling**: Stratified sampling for folders > 300 files
//! - **Coverage-based iteration**: Loop until 95%+ files are organized
//! - **Janitor pass**: Handle remaining unmatched files
//! - **O(1) complexity**: Context size constant regardless of folder size
//!
//! V3 Features (retained):
//! - **Prompt caching**: Marks initial context with cache_control: ephemeral
//! - **Header-based rate limiting**: Uses RateLimitManager for dynamic delays
//! - **FolderDigest**: Pre-computed analytics for one-shot planning
//! - **LocalVectorIndex**: Real semantic search via fastembed
//!
//! This module implements the main agent loop that:
//! 1. Builds a ShadowVFS from the target folder
//! 2. Checks file count to decide between full tree or sampling mode
//! 3. Runs the coverage loop with Claude using V2 tools
//! 4. Returns the finalized OrganizePlan

use crate::ai::client::{CacheControl, ClaudeModel};
use crate::ai::credentials::CredentialManager;
use crate::jobs::OrganizePlan;

use super::analytics::DigestGenerator;
use super::architect::{self, Blueprint};
use super::compression;
use super::prompts::{
    build_v2_summary_context, build_v3_initial_context, build_v4_sampled_context,
    build_v4_janitor_context, build_v5_hologram_context, V2_AGENTIC_SYSTEM_PROMPT,
    V4_SAMPLING_SYSTEM_PROMPT, V5_HOLOGRAM_SYSTEM_PROMPT,
};
use super::rate_limiter::RateLimitManager;
use super::sampling::{self, should_use_sampling};
use super::tools::{execute_v2_tool, get_v2_organize_tools, V2ToolResult};
use super::vfs::ShadowVFS;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Maximum retries for rate limit errors
const MAX_RETRIES: u32 = 3;

/// Maximum iterations before giving up
const MAX_ITERATIONS: usize = 10;

/// Maximum tokens for response
const MAX_TOKENS: u32 = 8192;

/// API request with tools support
#[derive(Serialize)]
struct ToolApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<ToolMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<crate::ai::tools::ToolDefinition>>,
}

/// Message with tool support
#[derive(Serialize, Clone)]
struct ToolMessage {
    role: String,
    content: Vec<ToolMessageContent>,
}

/// Content block for tool messages
/// V3: Added cache_control support for prompt caching
#[derive(Serialize, Clone)]
#[serde(untagged)]
enum ToolMessageContent {
    Text {
        #[serde(rename = "type")]
        content_type: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ToolUse {
        #[serde(rename = "type")]
        content_type: String,
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        #[serde(rename = "type")]
        content_type: String,
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

impl ToolMessageContent {
    /// Create a text content block (no caching)
    fn text(text: &str) -> Self {
        Self::Text {
            content_type: "text".to_string(),
            text: text.to_string(),
            cache_control: None,
        }
    }

    /// Create a text content block with ephemeral caching
    /// V3: Use this for large, repeated context like initial file tree
    fn text_cached(text: &str) -> Self {
        Self::Text {
            content_type: "text".to_string(),
            text: text.to_string(),
            cache_control: Some(CacheControl::ephemeral()),
        }
    }

    fn tool_use(id: &str, name: &str, input: &serde_json::Value) -> Self {
        Self::ToolUse {
            content_type: "tool_use".to_string(),
            id: id.to_string(),
            name: name.to_string(),
            input: input.clone(),
        }
    }

    fn tool_result(tool_use_id: &str, content: &str, is_error: bool) -> Self {
        Self::ToolResult {
            content_type: "tool_result".to_string(),
            tool_use_id: tool_use_id.to_string(),
            content: content.to_string(),
            is_error: if is_error { Some(true) } else { None },
        }
    }
}

/// Content block in response
#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
enum ContentBlockResponse {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// API response
#[derive(Deserialize, Debug)]
struct ToolApiResponse {
    content: Vec<ContentBlockResponse>,
    stop_reason: String,
}

/// API error response
#[derive(Deserialize)]
struct ApiError {
    error: ApiErrorDetail,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
}

/// Event types emitted during the agent loop
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Agent is indexing files
    Indexing(String),
    /// Agent is searching files
    Searching(String),
    /// Agent is applying rules
    ApplyingRules(String),
    /// Agent is previewing operations
    Previewing(String),
    /// Agent is committing the plan
    Committing(String),
    /// Agent is thinking (text output)
    Thinking(String),
    /// Agent encountered an error
    Error(String),
}

/// Expandable detail for event emission
#[derive(Clone)]
pub struct ExpandableDetail {
    pub label: String,
    pub value: String,
}

impl serde::Serialize for ExpandableDetail {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ExpandableDetail", 2)?;
        state.serialize_field("label", &self.label)?;
        state.serialize_field("value", &self.value)?;
        state.end()
    }
}

/// Progress event for analysis progress bar
/// This is emitted via Tauri events to update the progress bar in the UI
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    /// Current phase (scanning, analyzing, applying_rules, etc.)
    pub phase: String,
    /// Current progress value
    pub current: usize,
    /// Total expected value
    pub total: usize,
    /// Human-readable message
    pub message: String,
}

/// Run the V4 agentic organize workflow
///
/// V4 improvements (Map-Reduce Architecture):
/// - **Smart Sampling**: Uses stratified sampling for folders > 300 files
/// - **Coverage-based iteration**: Loops until 95%+ files are organized
/// - **Janitor pass**: Handles remaining unmatched files
/// - **O(1) complexity**: Context size constant regardless of folder size
///
/// V3 features (retained):
/// - Prompt caching for 90% token reduction
/// - Dynamic rate limiting based on API response headers
/// - Expandable details for UI transparency
///
/// V7 features:
/// - **Progress emitter**: Optional callback to emit analysis-progress events for UI progress bar
///
/// This function:
/// 1. Builds a ShadowVFS from the target folder
/// 2. Checks file count to decide between full tree or sampling mode
/// 3. Runs the coverage loop with Claude using V2 tools
/// 4. Returns the final OrganizePlan
pub async fn run_v2_agentic_organize<F, P>(
    target_folder: &Path,
    user_request: &str,
    event_emitter: F,
    progress_emitter: Option<P>,
) -> Result<OrganizePlan, String>
where
    F: Fn(&str, &str, Option<Vec<ExpandableDetail>>),
    P: Fn(ProgressEvent),
{
    // 1. Build ShadowVFS from target folder
    event_emitter("indexing", "Scanning folder structure...", Some(vec![
        ExpandableDetail { label: "Path".to_string(), value: target_folder.to_string_lossy().to_string() },
    ]));
    eprintln!("[V4AgentLoop] Building VFS for: {}", target_folder.display());

    let mut vfs = ShadowVFS::new(target_folder).map_err(|e| {
        format!("Failed to scan folder: {}", e)
    })?;

    let file_count = vfs.file_count();
    let dir_count = vfs.directory_count();

    // Emit initial progress
    if let Some(ref emit_progress) = progress_emitter {
        emit_progress(ProgressEvent {
            phase: "scanning".to_string(),
            current: 0,
            total: file_count,
            message: format!("Found {} files in {} directories", file_count, dir_count),
        });
    }

    // V5: Check if hologram compression should be used (pattern-heavy folders)
    // V4: Fall back to sampling for large folders without patterns
    let all_files = vfs.all_files_vec();
    let use_hologram = compression::should_use_hologram(&all_files, 300);
    let use_sampling = should_use_sampling(file_count);

    let mode_str = if use_hologram {
        "V5 Hologram (pattern-folded)"
    } else if use_sampling {
        "V4 Map-Reduce (sampled)"
    } else {
        "Full tree"
    };

    event_emitter("indexing", &format!("Found {} files", file_count), Some(vec![
        ExpandableDetail { label: "Files".to_string(), value: file_count.to_string() },
        ExpandableDetail { label: "Directories".to_string(), value: dir_count.to_string() },
        ExpandableDetail { label: "Mode".to_string(), value: mode_str.to_string() },
    ]));

    eprintln!("[AgentLoop] File count: {}, using {} mode", file_count, mode_str);

    // V6: Run Architect phase to generate Blueprint from user instruction
    // This designs the high-level organization strategy before any agent loops
    let blueprint = architect::run_architect(
        target_folder,
        user_request,
        &vfs,
        &event_emitter,
    ).await?;

    // Embed Blueprint folder descriptions for vector matching in Builder phase
    let blueprint = architect::embed_blueprint(&blueprint, &vfs)?;

    eprintln!(
        "[AgentLoop] Blueprint created: {} with {} folders",
        blueprint.strategy_name,
        blueprint.structure.len()
    );

    // V5: For pattern-heavy large folders, use hologram compression
    if use_hologram {
        return run_v5_hologram_loop_with_blueprint(target_folder, user_request, &event_emitter, &mut vfs, &blueprint, progress_emitter.as_ref()).await;
    }

    // V4: For large folders without patterns, use sampling mode
    if use_sampling {
        return run_v4_sampled_loop_with_blueprint(target_folder, user_request, &event_emitter, &mut vfs, &blueprint, progress_emitter.as_ref()).await;
    }

    // 2. Generate FolderDigest for rich analytics (V3 - for small folders)
    event_emitter("indexing", "Analyzing folder contents...", None);
    let digest_generator = DigestGenerator::new();
    let digest = digest_generator
        .generate(target_folder, Some(vfs.vector_index()))
        .unwrap_or_else(|e| {
            eprintln!("[V4AgentLoop] Digest generation failed: {}, using minimal digest", e);
            // Return minimal digest on error
            super::analytics::FolderDigest {
                root_path: target_folder.to_string_lossy().to_string(),
                file_count,
                dir_count: vfs.directory_count(),
                total_size: 0,
                ext_counts: std::collections::HashMap::new(),
                mime_breakdown: std::collections::HashMap::new(),
                date_range: (0, 0),
                common_prefixes: Vec::new(),
                content_previews: Vec::new(),
                semantic_tags: Vec::new(),
                max_depth: 0,
                hidden_count: 0,
            }
        });
    eprintln!(
        "[V4AgentLoop] Generated digest: {} files, {} dirs, {} extensions",
        digest.file_count, digest.dir_count, digest.ext_counts.len()
    );

    // 3. Generate compressed tree for context
    let compressed_tree = vfs.generate_compressed_tree();
    eprintln!(
        "[V4AgentLoop] Generated tree context: {} chars",
        compressed_tree.len()
    );

    // 4. Build V3 initial context with digest and Blueprint
    // V6: Enrich user request with Blueprint context for full tree mode
    let enriched_request = format!(
        "{}{}",
        user_request,
        format_blueprint_context(&blueprint)
    );

    let initial_context = build_v3_initial_context(
        &target_folder.to_string_lossy(),
        &compressed_tree,
        &digest,
        &enriched_request,
    );

    // 5. Initialize conversation
    let tools = get_v2_organize_tools();
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    let api_key = CredentialManager::get_api_key("anthropic")?;

    // V3: Initialize rate limiter for header-based dynamic delays
    let mut rate_limiter = RateLimitManager::new();

    // V3: Mark initial context with cache_control: ephemeral for prompt caching
    let mut messages = vec![ToolMessage {
        role: "user".to_string(),
        content: vec![ToolMessageContent::text_cached(&initial_context)],
    }];

    // 6. Agentic loop (V3 flow for small folders)
    for iteration in 0..MAX_ITERATIONS {
        // V3: Use dynamic rate limiting instead of fixed delay
        if iteration > 0 {
            let delay = rate_limiter.get_delay();
            eprintln!("[V4AgentLoop] Waiting {:?} before next request...", delay);
            tokio::time::sleep(delay).await;
        }

        eprintln!("[V4AgentLoop] Iteration {}", iteration + 1);

        // Emit progress for each iteration
        if let Some(ref emit_progress) = progress_emitter {
            let organized = vfs.organized_count();
            emit_progress(ProgressEvent {
                phase: "analyzing".to_string(),
                current: organized,
                total: file_count,
                message: format!("Iteration {}/{} - {} of {} files organized", iteration + 1, MAX_ITERATIONS, organized, file_count),
            });
        }

        // After first iteration, replace full tree context with compact summary
        // This saves ~15,000 tokens per request (from 60KB tree to 500 byte summary)
        if iteration == 1 {
            let summary_context = build_v2_summary_context(
                &target_folder.to_string_lossy(),
                vfs.file_count(),
                vfs.directory_count(),
                user_request,
            );
            messages[0] = ToolMessage {
                role: "user".to_string(),
                content: vec![ToolMessageContent::text(&summary_context)],
            };
            eprintln!("[V4AgentLoop] Replaced tree context with summary ({} chars)", summary_context.len());
        }

        // Prune old messages to prevent context overflow (keep initial + last N)
        const MAX_MESSAGES: usize = 7; // Initial message + 3 roundtrips (6 messages)
        if messages.len() > MAX_MESSAGES {
            let initial_message = messages.remove(0);
            let to_remove = messages.len() - (MAX_MESSAGES - 1);
            messages.drain(0..to_remove);
            messages.insert(0, initial_message);
            eprintln!("[V4AgentLoop] Pruned messages: kept {} of {} total", messages.len(), messages.len() + to_remove);
        }

        // Use Haiku for initial analysis (cheaper, faster), Sonnet for final planning
        let model = if iteration < 2 {
            ClaudeModel::Haiku  // 10x cheaper for exploration
        } else {
            ClaudeModel::Sonnet  // Better reasoning for planning
        };
        eprintln!("[V3AgentLoop] Using model: {:?}", model.as_str());

        // Send request to Claude
        let request = ToolApiRequest {
            model: model.as_str().to_string(),
            max_tokens: MAX_TOKENS,
            system: V2_AGENTIC_SYSTEM_PROMPT.to_string(),
            messages: messages.clone(),
            tools: Some(tools.clone()),
        };

        // Send request with exponential backoff for rate limits
        let mut retry_delay = Duration::from_secs(5);
        let mut last_error = String::new();
        let mut response_result = None;

        for retry in 0..=MAX_RETRIES {
            if retry > 0 {
                eprintln!("[V3AgentLoop] Rate limited, retrying in {:?} (attempt {}/{})", retry_delay, retry, MAX_RETRIES);
                event_emitter("thinking", &format!("Rate limited, waiting {:?}...", retry_delay), Some(vec![
                    ExpandableDetail { label: "Retry".to_string(), value: format!("{}/{}", retry, MAX_RETRIES) },
                    ExpandableDetail { label: "Delay".to_string(), value: format!("{:?}", retry_delay) },
                ]));
                tokio::time::sleep(retry_delay).await;
                retry_delay *= 2; // Exponential backoff
            }

            let resp = client
                .post(ANTHROPIC_API_URL)
                .header("x-api-key", &api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await;

            match resp {
                Ok(r) if r.status() == 429 => {
                    // Rate limited - get retry-after header if available
                    if let Some(retry_after) = r.headers().get("retry-after") {
                        if let Ok(secs) = retry_after.to_str().unwrap_or("5").parse::<u64>() {
                            retry_delay = Duration::from_secs(secs);
                        }
                    }
                    last_error = "Rate limit exceeded".to_string();
                    continue;
                }
                Ok(r) => {
                    response_result = Some(r);
                    break;
                }
                Err(e) => {
                    last_error = format!("Request failed: {}", e);
                    continue;
                }
            }
        }

        let response = response_result.ok_or_else(|| format!("Max retries exceeded: {}", last_error))?;

        // V3: Update rate limiter from response headers before consuming response
        rate_limiter.update_from_response(&response);

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if let Ok(api_error) = serde_json::from_str::<ApiError>(&error_text) {
                return Err(format!("API error: {}", api_error.error.message));
            }
            return Err(format!("API error ({}): {}", status, error_text));
        }

        let api_response: ToolApiResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        eprintln!("[V3AgentLoop] stop_reason: {}", api_response.stop_reason);

        // Process response content
        let mut assistant_content: Vec<ToolMessageContent> = Vec::new();
        let mut tool_results: Vec<ToolMessageContent> = Vec::new();

        for block in &api_response.content {
            match block {
                ContentBlockResponse::Text { text } => {
                    if !text.trim().is_empty() {
                        let preview: String = text.chars().take(200).collect();
                        eprintln!("[V3AgentLoop] Thinking: {}...", &preview);

                        if preview.len() > 20 {
                            event_emitter("thinking", &preview, None);
                        }
                    }
                    assistant_content.push(ToolMessageContent::text(text));
                }

                ContentBlockResponse::ToolUse { id, name, input } => {
                    eprintln!("[V3AgentLoop] Tool use: {}", name);
                    assistant_content.push(ToolMessageContent::tool_use(id, name, input));

                    // Emit appropriate event based on tool name
                    let _event_type = match name.as_str() {
                        "query_semantic_index" => {
                            let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("files");
                            let max_results = input.get("max_results").and_then(|v| v.as_u64()).unwrap_or(10);
                            event_emitter("searching", &format!("Searching for '{}'", query), Some(vec![
                                ExpandableDetail { label: "Query".to_string(), value: query.to_string() },
                                ExpandableDetail { label: "Max results".to_string(), value: max_results.to_string() },
                            ]));
                            "searching"
                        }
                        "apply_organization_rules" => {
                            let rules = input.get("rules").and_then(|v| v.as_array());
                            let count = rules.map(|a| a.len()).unwrap_or(0);
                            let rule_names: Vec<String> = rules.map(|arr| {
                                arr.iter()
                                    .filter_map(|r| r.get("name").and_then(|n| n.as_str()))
                                    .take(3)
                                    .map(|s| s.to_string())
                                    .collect()
                            }).unwrap_or_default();
                            event_emitter("applying_rules", &format!("Applying {} rules", count), Some(vec![
                                ExpandableDetail { label: "Rules".to_string(), value: count.to_string() },
                                ExpandableDetail { label: "Names".to_string(), value: rule_names.join(", ") },
                            ]));
                            "applying_rules"
                        }
                        "preview_operations" => {
                            let group_by = input.get("group_by").and_then(|v| v.as_str()).unwrap_or("operation");
                            event_emitter("previewing", "Generating preview...", Some(vec![
                                ExpandableDetail { label: "Group by".to_string(), value: group_by.to_string() },
                            ]));
                            "previewing"
                        }
                        "commit_plan" => {
                            let description = input.get("description").and_then(|v| v.as_str()).unwrap_or("");
                            event_emitter("committing", "Finalizing plan...", Some(vec![
                                ExpandableDetail { label: "Description".to_string(), value: description.to_string() },
                            ]));
                            "committing"
                        }
                        _ => "executing"
                    };

                    // Execute the tool
                    let result = execute_v2_tool(name, input, &mut vfs);

                    match result {
                        V2ToolResult::Continue(output) => {
                            eprintln!("[V3AgentLoop] Tool success: {} bytes", output.len());
                            tool_results.push(ToolMessageContent::tool_result(
                                id,
                                &output,
                                false,
                            ));
                        }
                        V2ToolResult::Commit(plan) => {
                            eprintln!(
                                "[V3AgentLoop] Plan committed: {} operations",
                                plan.operations.len()
                            );
                            // Count operation types
                            let mut move_count = 0;
                            let mut create_count = 0;
                            let mut rename_count = 0;
                            for op in &plan.operations {
                                match op.op_type.as_str() {
                                    "move" => move_count += 1,
                                    "create_folder" => create_count += 1,
                                    "rename" => rename_count += 1,
                                    _ => {}
                                }
                            }
                            event_emitter(
                                "committing",
                                &format!("Plan created with {} operations", plan.operations.len()),
                                Some(vec![
                                    ExpandableDetail { label: "Total ops".to_string(), value: plan.operations.len().to_string() },
                                    ExpandableDetail { label: "Moves".to_string(), value: move_count.to_string() },
                                    ExpandableDetail { label: "Creates".to_string(), value: create_count.to_string() },
                                    ExpandableDetail { label: "Renames".to_string(), value: rename_count.to_string() },
                                ]),
                            );
                            // Emit final progress
                            if let Some(ref emit_progress) = progress_emitter {
                                emit_progress(ProgressEvent {
                                    phase: "complete".to_string(),
                                    current: file_count,
                                    total: file_count,
                                    message: format!("Plan complete: {} operations", plan.operations.len()),
                                });
                            }
                            return Ok(plan);
                        }
                        V2ToolResult::Error(err) => {
                            let context = format!(
                                "Tool error (files: {}, ops: {}): {}",
                                vfs.files().len(),
                                vfs.operations().len(),
                                err
                            );
                            eprintln!("[V3AgentLoop] {}", context);
                            event_emitter("error", &context, Some(vec![
                                ExpandableDetail { label: "Files scanned".to_string(), value: vfs.files().len().to_string() },
                                ExpandableDetail { label: "Pending ops".to_string(), value: vfs.operations().len().to_string() },
                                ExpandableDetail { label: "Error".to_string(), value: err.clone() },
                            ]));
                            tool_results.push(ToolMessageContent::tool_result(
                                id,
                                &context,
                                true,
                            ));
                        }
                    }
                }
            }
        }

        // Check if we should end
        if api_response.stop_reason == "end_turn" && tool_results.is_empty() {
            // Agent finished without committing - try to commit what we have
            if !vfs.operations().is_empty() {
                eprintln!("[V3AgentLoop] Auto-committing {} operations", vfs.operations().len());
                let plan = OrganizePlan {
                    plan_id: format!("plan-{}", chrono::Utc::now().timestamp_millis()),
                    description: "Auto-generated organization plan".to_string(),
                    operations: vfs
                        .operations()
                        .iter()
                        .map(|op| crate::jobs::OrganizeOperation {
                            op_id: op.op_id.clone(),
                            op_type: op.op_type.to_string(),
                            source: op.source.clone(),
                            destination: op.destination.clone(),
                            path: op.path.clone(),
                            new_name: op.new_name.clone(),
                        })
                        .collect(),
                    // organization_root is the target folder - all organization stays within it
                    target_folder: vfs.organization_root().to_string_lossy().to_string(),
                    simplification_recommended: None,
                };
                return Ok(plan);
            }

            return Err(format!(
                "Agent finished after searching {} files but created no operations. {}",
                vfs.files().len(),
                if vfs.operations().is_empty() {
                    "The folder may already be well-organized, or the files didn't match any organization rules."
                } else {
                    "Try organizing with different rules or a smaller subfolder."
                }
            ));
        }

        // Add assistant message
        messages.push(ToolMessage {
            role: "assistant".to_string(),
            content: assistant_content,
        });

        // Add tool results if any
        if !tool_results.is_empty() {
            messages.push(ToolMessage {
                role: "user".to_string(),
                content: tool_results,
            });
        }
    }

    Err("Organization took too long. Please try with a smaller folder or simpler request.".to_string())
}

/// V4 Map-Reduce loop for large folders
///
/// This function handles folders with > 300 files using stratified sampling
/// instead of sending the full file tree. It iterates until 95%+ coverage
/// is achieved.
///
/// ## Algorithm
///
/// 1. Generate a statistical sample of unmatched files
/// 2. Send sample to AI with V4 prompt
/// 3. AI writes broad rules (extension-based, pattern-based)
/// 4. Apply rules to ALL files in memory (the "Reduce" step)
/// 5. Check coverage - if < 95%, generate new sample from unmatched files
/// 6. Repeat until coverage target reached or max iterations
async fn run_v4_sampled_loop<F, P>(
    target_folder: &Path,
    user_request: &str,
    event_emitter: &F,
    vfs: &mut ShadowVFS,
    progress_emitter: Option<&P>,
) -> Result<OrganizePlan, String>
where
    F: Fn(&str, &str, Option<Vec<ExpandableDetail>>),
    P: Fn(ProgressEvent),
{
    let file_count = vfs.file_count();

    event_emitter("thinking", "Using Map-Reduce mode for large folder", Some(vec![
        ExpandableDetail { label: "Files".to_string(), value: file_count.to_string() },
        ExpandableDetail { label: "Strategy".to_string(), value: "Stratified sampling".to_string() },
    ]));

    eprintln!("[V4SampledLoop] Starting Map-Reduce flow for {} files", file_count);

    // Initialize HTTP client and rate limiter
    let tools = get_v2_organize_tools();
    let client = Client::builder()
        .timeout(Duration::from_secs(180)) // Longer timeout for large folders
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    let api_key = CredentialManager::get_api_key("anthropic")?;
    let mut rate_limiter = RateLimitManager::new();

    // V4 uses fewer iterations but with higher coverage per iteration
    const V4_MAX_ITERATIONS: usize = 5;

    for iteration in 0..V4_MAX_ITERATIONS {
        // Rate limiting between iterations
        if iteration > 0 {
            let delay = rate_limiter.get_delay();
            eprintln!("[V4SampledLoop] Waiting {:?} before iteration {}", delay, iteration + 1);
            tokio::time::sleep(delay).await;
        }

        // Check coverage - stop if we've reached target
        let coverage = vfs.coverage();
        let organized = vfs.organized_count();
        let unmatched_count = file_count - organized;

        // Emit progress for UI progress bar
        if let Some(emit_progress) = progress_emitter {
            emit_progress(ProgressEvent {
                phase: "analyzing".to_string(),
                current: organized,
                total: file_count,
                message: format!(
                    "Iteration {} - {:.0}% organized ({} of {} files)",
                    iteration + 1,
                    coverage * 100.0,
                    organized,
                    file_count
                ),
            });
        }

        event_emitter("thinking", "Analyzing files...", None);

        if vfs.coverage_target_reached() {
            eprintln!("[V4SampledLoop] Coverage target reached: {:.1}%", coverage * 100.0);
            break;
        }

        // Stop if no unmatched files remain
        if unmatched_count == 0 {
            eprintln!("[V4SampledLoop] All files organized");
            break;
        }

        eprintln!(
            "[V4SampledLoop] Iteration {}: coverage={:.1}%, unmatched={}",
            iteration + 1, coverage * 100.0, unmatched_count
        );

        // Generate sample from unmatched files
        let all_files = vfs.all_files_vec();
        let sample = if iteration == 0 {
            // First iteration: sample all files
            sampling::generate_sample(&all_files, 0)
        } else {
            // Subsequent iterations: sample only unmatched files (janitor pass)
            sampling::generate_unmatched_sample(&all_files, vfs.matched_paths())
        };

        // Build context
        let context = if iteration == 0 {
            build_v4_sampled_context(
                &target_folder.to_string_lossy(),
                &sample,
                iteration,
                user_request,
            )
        } else {
            build_v4_janitor_context(
                &target_folder.to_string_lossy(),
                &sample,
                coverage,
                user_request,
            )
        };

        eprintln!("[V4SampledLoop] Context size: {} chars, {} samples", context.len(), sample.samples.len());

        // Use V4 system prompt for sampled mode
        let system_prompt = V4_SAMPLING_SYSTEM_PROMPT;

        // Build messages
        let messages = vec![ToolMessage {
            role: "user".to_string(),
            content: vec![ToolMessageContent::text_cached(&context)],
        }];

        // Use Sonnet for better rule generation
        let model = ClaudeModel::Sonnet;

        // Send request
        let request = ToolApiRequest {
            model: model.as_str().to_string(),
            max_tokens: MAX_TOKENS,
            system: system_prompt.to_string(),
            messages,
            tools: Some(tools.clone()),
        };

        // Make API call with retry logic
        let response = send_api_request_with_retry(&client, &api_key, &request, &mut rate_limiter).await?;

        // Update rate limiter
        rate_limiter.update_from_response(&response);

        // Process response
        let api_response: ToolApiResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        eprintln!("[V4SampledLoop] stop_reason: {}", api_response.stop_reason);

        // Track if any rules were applied this iteration
        let prev_organized = vfs.organized_count();

        // Process tool calls
        for block in &api_response.content {
            if let ContentBlockResponse::ToolUse { id: _, name, input } = block {
                eprintln!("[V4SampledLoop] Tool use: {} with input: {}", name, serde_json::to_string_pretty(input).unwrap_or_default());

                // Emit event for UI
                match name.as_str() {
                    "apply_organization_rules" => {
                        let rules = input.get("rules").and_then(|v| v.as_array());
                        let count = rules.map(|a| a.len()).unwrap_or(0);
                        if count == 0 {
                            eprintln!("[V4SampledLoop] WARNING: apply_organization_rules called with 0 rules! Input keys: {:?}", input.as_object().map(|o| o.keys().collect::<Vec<_>>()));
                        }
                        event_emitter("applying_rules", &format!("Applying {} rules", count), Some(vec![
                            ExpandableDetail { label: "Rules".to_string(), value: count.to_string() },
                        ]));
                    }
                    "commit_plan" => {
                        event_emitter("committing", "Finalizing plan...", None);
                    }
                    _ => {}
                }

                // Execute tool
                let result = execute_v2_tool(name, input, vfs);

                match result {
                    V2ToolResult::Commit(plan) => {
                        eprintln!("[V4SampledLoop] Plan committed: {} operations", plan.operations.len());
                        event_emitter("committing", &format!("Plan created with {} operations", plan.operations.len()), Some(vec![
                            ExpandableDetail { label: "Operations".to_string(), value: plan.operations.len().to_string() },
                            ExpandableDetail { label: "Coverage".to_string(), value: format!("{:.1}%", vfs.coverage() * 100.0) },
                        ]));
                        return Ok(plan);
                    }
                    V2ToolResult::Continue(_output) => {
                        // Continue processing
                    }
                    V2ToolResult::Error(err) => {
                        eprintln!("[V4SampledLoop] Tool error: {}", err);
                        event_emitter("error", &format!("Tool error: {}", err), None);
                        // Continue to next tool call
                    }
                }
            }
        }

        // Check if any new files were matched
        let new_organized = vfs.organized_count();
        let matched_this_iteration = new_organized - prev_organized;

        eprintln!(
            "[V4SampledLoop] Iteration {} matched {} new files (total: {})",
            iteration + 1, matched_this_iteration, new_organized
        );

        // Anti-infinite loop: if no new files matched and not first iteration, break
        if matched_this_iteration == 0 && iteration > 0 {
            eprintln!("[V4SampledLoop] No new matches, stopping iteration");
            break;
        }
    }

    // Final check: commit what we have
    if !vfs.operations().is_empty() {
        let coverage = vfs.coverage();
        eprintln!(
            "[V4SampledLoop] Final commit: {} operations, {:.1}% coverage",
            vfs.operations().len(), coverage * 100.0
        );

        // Handle remaining unmatched files if coverage is low
        let unmatched_count = vfs.file_count() - vfs.organized_count();
        if unmatched_count > 0 && coverage < 0.95 {
            event_emitter("thinking", &format!("{} files will remain in place", unmatched_count), Some(vec![
                ExpandableDetail { label: "Unmatched".to_string(), value: unmatched_count.to_string() },
                ExpandableDetail { label: "Coverage".to_string(), value: format!("{:.1}%", coverage * 100.0) },
            ]));
        }

        let plan = OrganizePlan {
            plan_id: format!("plan-v4-{}", chrono::Utc::now().timestamp_millis()),
            description: format!(
                "V4 Map-Reduce organization plan ({:.1}% coverage, {} files organized)",
                coverage * 100.0,
                vfs.organized_count()
            ),
            operations: vfs
                .operations()
                .iter()
                .map(|op| crate::jobs::OrganizeOperation {
                    op_id: op.op_id.clone(),
                    op_type: op.op_type.to_string(),
                    source: op.source.clone(),
                    destination: op.destination.clone(),
                    path: op.path.clone(),
                    new_name: op.new_name.clone(),
                })
                .collect(),
            // organization_root is the target folder - all organization stays within it
            target_folder: vfs.organization_root().to_string_lossy().to_string(),
            simplification_recommended: None,
        };

        event_emitter("committing", &format!("Plan ready: {} operations", plan.operations.len()), Some(vec![
            ExpandableDetail { label: "Operations".to_string(), value: plan.operations.len().to_string() },
            ExpandableDetail { label: "Coverage".to_string(), value: format!("{:.1}%", coverage * 100.0) },
        ]));

        return Ok(plan);
    }

    // No operations created
    Err(format!(
        "V4 Map-Reduce completed but created no operations for {} files. \
         The folder may already be well-organized.",
        vfs.file_count()
    ))
}

/// Helper function to send API request with retry logic
async fn send_api_request_with_retry(
    client: &Client,
    api_key: &str,
    request: &ToolApiRequest,
    _rate_limiter: &mut RateLimitManager,
) -> Result<reqwest::Response, String> {
    let mut retry_delay = Duration::from_secs(5);

    for retry in 0..=MAX_RETRIES {
        if retry > 0 {
            eprintln!("[V4SampledLoop] Retrying in {:?} (attempt {}/{})", retry_delay, retry, MAX_RETRIES);
            tokio::time::sleep(retry_delay).await;
            retry_delay *= 2;
        }

        let resp = client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(request)
            .send()
            .await;

        match resp {
            Ok(r) if r.status() == 429 => {
                if let Some(retry_after) = r.headers().get("retry-after") {
                    if let Ok(secs) = retry_after.to_str().unwrap_or("5").parse::<u64>() {
                        retry_delay = Duration::from_secs(secs);
                    }
                }
                continue;
            }
            Ok(r) if r.status().is_success() => {
                return Ok(r);
            }
            Ok(r) => {
                let status = r.status();
                let error_text = r.text().await.unwrap_or_default();
                return Err(format!("API error ({}): {}", status, error_text));
            }
            Err(e) => {
                if retry == MAX_RETRIES {
                    return Err(format!("Request failed after {} retries: {}", MAX_RETRIES, e));
                }
                continue;
            }
        }
    }

    Err("Max retries exceeded".to_string())
}

/// V5 Hologram loop for pattern-heavy large folders
///
/// This function uses Adaptive Pattern Folding to compress file lists
/// by detecting sequential patterns and representing them as ranges.
///
/// ## Token Savings
///
/// | Scenario | V4 Sampling | V5 Hologram | Savings |
/// |----------|-------------|-------------|---------|
/// | 1,000 sequential images | ~2,600 tokens | ~150 tokens | 94% |
/// | 5,000 mixed files | ~2,600 tokens | ~400 tokens | 85% |
///
/// ## Algorithm
///
/// 1. Generate hologram from all files (pattern folding)
/// 2. Send compressed context to AI
/// 3. AI writes rules based on pattern templates
/// 4. Apply rules to ALL files (the "Reduce" step)
/// 5. Commit when coverage >= 95%
async fn run_v5_hologram_loop<F, P>(
    target_folder: &Path,
    user_request: &str,
    event_emitter: &F,
    vfs: &mut ShadowVFS,
    progress_emitter: Option<&P>,
) -> Result<OrganizePlan, String>
where
    F: Fn(&str, &str, Option<Vec<ExpandableDetail>>),
    P: Fn(ProgressEvent),
{
    let file_count = vfs.file_count();

    // Generate hologram (pattern-folded representation)
    let all_files = vfs.all_files_vec();
    let hologram = compression::generate_hologram(&all_files);

    event_emitter(
        "analyzing",
        &format!(
            "Compressed {} files into {} patterns + {} outliers",
            hologram.stats.total_files,
            hologram.stats.pattern_count,
            hologram.stats.outlier_count
        ),
        Some(vec![
            ExpandableDetail {
                label: "Files".to_string(),
                value: file_count.to_string(),
            },
            ExpandableDetail {
                label: "Patterns".to_string(),
                value: hologram.stats.pattern_count.to_string(),
            },
            ExpandableDetail {
                label: "Coverage".to_string(),
                value: format!("{:.1}%", hologram.stats.pattern_coverage * 100.0),
            },
        ]),
    );

    eprintln!(
        "[V5HologramLoop] Generated hologram: {} patterns, {:.1}% coverage, {} outliers",
        hologram.stats.pattern_count,
        hologram.stats.pattern_coverage * 100.0,
        hologram.stats.outlier_count
    );

    // Initialize HTTP client and rate limiter
    let tools = get_v2_organize_tools();
    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    let api_key = CredentialManager::get_api_key("anthropic")?;
    let mut rate_limiter = RateLimitManager::new();

    // V5 uses fewer iterations since patterns are pre-computed
    const V5_MAX_ITERATIONS: usize = 3;

    for iteration in 0..V5_MAX_ITERATIONS {
        // Rate limiting between iterations
        if iteration > 0 {
            let delay = rate_limiter.get_delay();
            eprintln!("[V5HologramLoop] Waiting {:?} before iteration {}", delay, iteration + 1);
            tokio::time::sleep(delay).await;
        }

        // Check coverage - stop if we've reached target
        let coverage = vfs.coverage();
        let organized = vfs.organized_count();

        // Emit progress for UI progress bar
        if let Some(emit_progress) = progress_emitter {
            emit_progress(ProgressEvent {
                phase: "analyzing".to_string(),
                current: organized,
                total: file_count,
                message: format!(
                    "Iteration {} - {:.0}% organized ({} of {} files)",
                    iteration + 1,
                    coverage * 100.0,
                    organized,
                    file_count
                ),
            });
        }

        event_emitter("thinking", "Analyzing files...", None);

        if vfs.coverage_target_reached() {
            eprintln!("[V5HologramLoop] Coverage target reached: {:.1}%", coverage * 100.0);
            break;
        }

        eprintln!(
            "[V5HologramLoop] Iteration {}: coverage={:.1}%, organized={}",
            iteration + 1,
            coverage * 100.0,
            organized
        );

        // Build V5 hologram context
        let context = build_v5_hologram_context(
            &target_folder.to_string_lossy(),
            &hologram,
            user_request,
        );

        eprintln!(
            "[V5HologramLoop] Context size: {} chars (vs ~50K for full tree)",
            context.len()
        );

        // Build messages with cached context
        let messages = vec![ToolMessage {
            role: "user".to_string(),
            content: vec![ToolMessageContent::text_cached(&context)],
        }];

        // Use Sonnet for better rule generation
        let model = ClaudeModel::Sonnet;

        // Send request
        let request = ToolApiRequest {
            model: model.as_str().to_string(),
            max_tokens: MAX_TOKENS,
            system: V5_HOLOGRAM_SYSTEM_PROMPT.to_string(),
            messages,
            tools: Some(tools.clone()),
        };

        // Make API call with retry logic
        let response = send_api_request_with_retry(&client, &api_key, &request, &mut rate_limiter).await?;

        // Update rate limiter
        rate_limiter.update_from_response(&response);

        // Process response
        let api_response: ToolApiResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        eprintln!("[V5HologramLoop] stop_reason: {}", api_response.stop_reason);

        // Track if any rules were applied this iteration
        let prev_organized = vfs.organized_count();

        // Process tool calls
        for block in &api_response.content {
            if let ContentBlockResponse::ToolUse { id: _, name, input } = block {
                eprintln!("[V5HologramLoop] Tool use: {} with input: {}", name, serde_json::to_string_pretty(input).unwrap_or_default());

                // Emit event for UI
                match name.as_str() {
                    "apply_organization_rules" => {
                        let rules = input.get("rules").and_then(|v| v.as_array());
                        let count = rules.map(|a| a.len()).unwrap_or(0);
                        if count == 0 {
                            eprintln!("[V5HologramLoop] WARNING: apply_organization_rules called with 0 rules! Input keys: {:?}", input.as_object().map(|o| o.keys().collect::<Vec<_>>()));
                        }
                        event_emitter(
                            "applying_rules",
                            &format!("Applying {} rules to {} files", count, file_count),
                            Some(vec![ExpandableDetail {
                                label: "Rules".to_string(),
                                value: count.to_string(),
                            }]),
                        );
                    }
                    "inspect_pattern_sample" => {
                        let pattern = input
                            .get("pattern_regex")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        event_emitter(
                            "searching",
                            &format!("Inspecting pattern: {}", pattern),
                            None,
                        );
                    }
                    "commit_plan" => {
                        event_emitter("committing", "Finalizing plan...", None);
                    }
                    _ => {}
                }

                // Execute tool
                let result = execute_v2_tool(name, input, vfs);

                match result {
                    V2ToolResult::Commit(plan) => {
                        eprintln!(
                            "[V5HologramLoop] Plan committed: {} operations",
                            plan.operations.len()
                        );
                        event_emitter(
                            "committing",
                            &format!("Plan created with {} operations", plan.operations.len()),
                            Some(vec![
                                ExpandableDetail {
                                    label: "Operations".to_string(),
                                    value: plan.operations.len().to_string(),
                                },
                                ExpandableDetail {
                                    label: "Coverage".to_string(),
                                    value: format!("{:.1}%", vfs.coverage() * 100.0),
                                },
                            ]),
                        );
                        return Ok(plan);
                    }
                    V2ToolResult::Continue(_output) => {
                        // Continue processing
                    }
                    V2ToolResult::Error(err) => {
                        eprintln!("[V5HologramLoop] Tool error: {}", err);
                        event_emitter("error", &format!("Tool error: {}", err), None);
                    }
                }
            }
        }

        // Check if any new files were matched
        let new_organized = vfs.organized_count();
        let matched_this_iteration = new_organized - prev_organized;

        eprintln!(
            "[V5HologramLoop] Iteration {} matched {} new files (total: {})",
            iteration + 1,
            matched_this_iteration,
            new_organized
        );

        // Anti-infinite loop: if no new files matched and not first iteration, break
        if matched_this_iteration == 0 && iteration > 0 {
            eprintln!("[V5HologramLoop] No new matches, stopping iteration");
            break;
        }
    }

    // Final check: commit what we have
    if !vfs.operations().is_empty() {
        let coverage = vfs.coverage();
        eprintln!(
            "[V5HologramLoop] Final commit: {} operations, {:.1}% coverage",
            vfs.operations().len(),
            coverage * 100.0
        );

        let plan = OrganizePlan {
            plan_id: format!("plan-v5-{}", chrono::Utc::now().timestamp_millis()),
            description: format!(
                "V5 Hologram organization plan ({:.1}% coverage, {} patterns detected)",
                coverage * 100.0,
                hologram.stats.pattern_count
            ),
            operations: vfs
                .operations()
                .iter()
                .map(|op| crate::jobs::OrganizeOperation {
                    op_id: op.op_id.clone(),
                    op_type: op.op_type.to_string(),
                    source: op.source.clone(),
                    destination: op.destination.clone(),
                    path: op.path.clone(),
                    new_name: op.new_name.clone(),
                })
                .collect(),
            // organization_root is the target folder - all organization stays within it
            target_folder: vfs.organization_root().to_string_lossy().to_string(),
            simplification_recommended: None,
        };

        event_emitter(
            "committing",
            &format!("Plan ready: {} operations", plan.operations.len()),
            Some(vec![
                ExpandableDetail {
                    label: "Operations".to_string(),
                    value: plan.operations.len().to_string(),
                },
                ExpandableDetail {
                    label: "Coverage".to_string(),
                    value: format!("{:.1}%", coverage * 100.0),
                },
            ]),
        );

        return Ok(plan);
    }

    // No operations created
    Err(format!(
        "V5 Hologram completed but created no operations for {} files with {} patterns. \
         The folder may already be well-organized.",
        file_count,
        hologram.stats.pattern_count
    ))
}

// ============================================================================
// V6 Blueprint-Aware Wrappers
// ============================================================================

/// Format Blueprint as additional context for the user request
fn format_blueprint_context(blueprint: &Blueprint) -> String {
    let mut context = String::new();

    context.push_str("\n\n## Organization Blueprint (from Architect)\n\n");
    context.push_str(&format!("**Strategy**: {}\n\n", blueprint.strategy_name));

    context.push_str("**Target Structure**:\n");
    for folder in &blueprint.structure {
        context.push_str(&format!(
            "- `{}` - {}\n",
            folder.path, folder.semantic_description
        ));
    }

    context.push_str(&format!(
        "\n**Extraction Rules**:\n```\n{}\n```\n",
        blueprint.extraction_rules
    ));

    context.push_str("\n**Important**: Follow this Blueprint when creating organization rules. ");
    context.push_str("Use the target folders defined above.\n");

    context
}

/// V6 Blueprint-aware wrapper for V4 sampled loop
async fn run_v4_sampled_loop_with_blueprint<F, P>(
    target_folder: &Path,
    user_request: &str,
    event_emitter: &F,
    vfs: &mut ShadowVFS,
    blueprint: &Blueprint,
    progress_emitter: Option<&P>,
) -> Result<OrganizePlan, String>
where
    F: Fn(&str, &str, Option<Vec<ExpandableDetail>>),
    P: Fn(ProgressEvent),
{
    // Enrich user request with Blueprint context
    let enriched_request = format!(
        "{}{}",
        user_request,
        format_blueprint_context(blueprint)
    );

    eprintln!(
        "[V6] Running V4 sampled loop with Blueprint: {}",
        blueprint.strategy_name
    );

    run_v4_sampled_loop(target_folder, &enriched_request, event_emitter, vfs, progress_emitter).await
}

/// V6 Blueprint-aware wrapper for V5 hologram loop
async fn run_v5_hologram_loop_with_blueprint<F, P>(
    target_folder: &Path,
    user_request: &str,
    event_emitter: &F,
    vfs: &mut ShadowVFS,
    blueprint: &Blueprint,
    progress_emitter: Option<&P>,
) -> Result<OrganizePlan, String>
where
    F: Fn(&str, &str, Option<Vec<ExpandableDetail>>),
    P: Fn(ProgressEvent),
{
    // Enrich user request with Blueprint context
    let enriched_request = format!(
        "{}{}",
        user_request,
        format_blueprint_context(blueprint)
    );

    eprintln!(
        "[V6] Running V5 hologram loop with Blueprint: {}",
        blueprint.strategy_name
    );

    run_v5_hologram_loop(target_folder, &enriched_request, event_emitter, vfs, progress_emitter).await
}

// ============================================================================
// V6 Hybrid Mode: GPT-5-nano Exploration + Claude Planning
// ============================================================================

/// V6 Hybrid organization loop using pre-analyzed files from GPT-5-nano
///
/// This function receives file analyses from OpenAI workers and uses Claude
/// to create organization rules based on the extracted entities and summaries.
///
/// ## Benefits
/// - **Cost**: GPT-5-nano is ~10x cheaper for bulk file analysis
/// - **Quality**: Claude Sonnet excels at rule creation and reasoning
/// - **Context**: Summaries + entities give Claude better context than raw filenames
///
/// ## Flow
/// 1. Receive FileAnalysis[] from GPT-5-nano workers
/// 2. Build enriched context with entities and document types
/// 3. Claude creates organization rules using entity data
/// 4. Apply rules to VFS
/// 5. Return OrganizePlan
pub async fn run_v6_hybrid_organization<F, P>(
    target_folder: &Path,
    user_request: &str,
    analyses: Vec<crate::ai::grok::FileAnalysis>,
    event_emitter: F,
    progress_emitter: Option<P>,
) -> Result<OrganizePlan, String>
where
    F: Fn(&str, &str, Option<Vec<ExpandableDetail>>),
    P: Fn(ProgressEvent),
{
    use super::prompts::{build_hybrid_context, V6_HYBRID_SYSTEM_PROMPT};

    // 1. Build ShadowVFS from target folder
    event_emitter("indexing", "Building virtual filesystem...", Some(vec![
        ExpandableDetail { label: "Path".to_string(), value: target_folder.to_string_lossy().to_string() },
    ]));

    let mut vfs = ShadowVFS::new(target_folder).map_err(|e| {
        format!("Failed to scan folder: {}", e)
    })?;

    let file_count = vfs.file_count();

    // Emit progress
    if let Some(ref emit_progress) = progress_emitter {
        emit_progress(ProgressEvent {
            phase: "planning".to_string(),
            current: 0,
            total: file_count,
            message: format!("Using {} pre-analyzed files from GPT-5-nano", analyses.len()),
        });
    }

    event_emitter("analyzing", &format!(
        "Received {} file analyses from GPT-5-nano",
        analyses.len()
    ), Some(vec![
        ExpandableDetail { label: "Files".to_string(), value: file_count.to_string() },
        ExpandableDetail { label: "Analyzed".to_string(), value: analyses.len().to_string() },
        ExpandableDetail { label: "Mode".to_string(), value: "V6 Hybrid (GPT→Claude)".to_string() },
    ]));

    eprintln!(
        "[V6HybridLoop] Starting with {} files, {} analyses from GPT-5-nano",
        file_count, analyses.len()
    );

    // 2. Build hybrid context from GPT-5-nano analyses
    let context = build_hybrid_context(
        &target_folder.to_string_lossy(),
        &analyses,
        user_request,
    );

    eprintln!(
        "[V6HybridLoop] Built hybrid context: {} chars",
        context.len()
    );

    // 3. Initialize HTTP client and tools
    let tools = get_v2_organize_tools();
    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    let api_key = CredentialManager::get_api_key("anthropic")?;
    let mut rate_limiter = RateLimitManager::new();

    // V6 uses fewer iterations since analyses are pre-computed
    const V6_MAX_ITERATIONS: usize = 3;

    for iteration in 0..V6_MAX_ITERATIONS {
        // Rate limiting between iterations
        if iteration > 0 {
            let delay = rate_limiter.get_delay();
            eprintln!("[V6HybridLoop] Waiting {:?} before iteration {}", delay, iteration + 1);
            tokio::time::sleep(delay).await;
        }

        // Check coverage
        let coverage = vfs.coverage();
        let organized = vfs.organized_count();

        // Emit progress for UI
        if let Some(ref emit_progress) = progress_emitter {
            emit_progress(ProgressEvent {
                phase: "planning".to_string(),
                current: organized,
                total: file_count,
                message: format!(
                    "Claude planning iteration {} - {:.0}% organized",
                    iteration + 1,
                    coverage * 100.0
                ),
            });
        }

        event_emitter("thinking", "Claude is creating organization rules...", None);

        if vfs.coverage_target_reached() {
            eprintln!("[V6HybridLoop] Coverage target reached: {:.1}%", coverage * 100.0);
            break;
        }

        eprintln!(
            "[V6HybridLoop] Iteration {}: coverage={:.1}%, organized={}",
            iteration + 1,
            coverage * 100.0,
            organized
        );

        // Build messages with cached context
        let messages = vec![ToolMessage {
            role: "user".to_string(),
            content: vec![ToolMessageContent::text_cached(&context)],
        }];

        // Use Sonnet for planning (Claude's strength)
        let model = ClaudeModel::Sonnet;

        // Send request
        let request = ToolApiRequest {
            model: model.as_str().to_string(),
            max_tokens: MAX_TOKENS,
            system: V6_HYBRID_SYSTEM_PROMPT.to_string(),
            messages,
            tools: Some(tools.clone()),
        };

        // Make API call with retry logic
        let response = send_api_request_with_retry(&client, &api_key, &request, &mut rate_limiter).await?;

        // Update rate limiter
        rate_limiter.update_from_response(&response);

        // Process response
        let api_response: ToolApiResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        eprintln!("[V6HybridLoop] stop_reason: {}", api_response.stop_reason);

        // Track if any rules were applied this iteration
        let prev_organized = vfs.organized_count();

        // Process tool calls
        for block in &api_response.content {
            if let ContentBlockResponse::ToolUse { id: _, name, input } = block {
                eprintln!("[V6HybridLoop] Tool use: {}", name);

                // Emit event for UI
                match name.as_str() {
                    "apply_organization_rules" => {
                        let rules = input.get("rules").and_then(|v| v.as_array());
                        let count = rules.map(|a| a.len()).unwrap_or(0);
                        event_emitter(
                            "applying_rules",
                            &format!("Applying {} rules based on entity analysis", count),
                            Some(vec![ExpandableDetail {
                                label: "Rules".to_string(),
                                value: count.to_string(),
                            }]),
                        );
                    }
                    "commit_plan" => {
                        event_emitter("committing", "Finalizing plan...", None);
                    }
                    _ => {}
                }

                // Execute tool
                let result = execute_v2_tool(name, input, &mut vfs);

                match result {
                    V2ToolResult::Commit(plan) => {
                        eprintln!(
                            "[V6HybridLoop] Plan committed: {} operations",
                            plan.operations.len()
                        );
                        event_emitter(
                            "committing",
                            &format!("Plan created with {} operations", plan.operations.len()),
                            Some(vec![
                                ExpandableDetail {
                                    label: "Operations".to_string(),
                                    value: plan.operations.len().to_string(),
                                },
                                ExpandableDetail {
                                    label: "Coverage".to_string(),
                                    value: format!("{:.1}%", vfs.coverage() * 100.0),
                                },
                            ]),
                        );
                        return Ok(plan);
                    }
                    V2ToolResult::Continue(_output) => {
                        // Continue processing
                    }
                    V2ToolResult::Error(err) => {
                        eprintln!("[V6HybridLoop] Tool error: {}", err);
                        event_emitter("error", &format!("Tool error: {}", err), None);
                    }
                }
            }
        }

        // Check if any new files were matched
        let new_organized = vfs.organized_count();
        let matched_this_iteration = new_organized - prev_organized;

        eprintln!(
            "[V6HybridLoop] Iteration {} matched {} new files (total: {})",
            iteration + 1,
            matched_this_iteration,
            new_organized
        );

        // Anti-infinite loop
        if matched_this_iteration == 0 && iteration > 0 {
            eprintln!("[V6HybridLoop] No new matches, stopping iteration");
            break;
        }
    }

    // Final check: commit what we have
    if !vfs.operations().is_empty() {
        let coverage = vfs.coverage();
        eprintln!(
            "[V6HybridLoop] Final commit: {} operations, {:.1}% coverage",
            vfs.operations().len(),
            coverage * 100.0
        );

        let plan = OrganizePlan {
            plan_id: format!("plan-v6-hybrid-{}", chrono::Utc::now().timestamp_millis()),
            description: format!(
                "V6 Hybrid organization (GPT-5-nano → Claude): {:.1}% coverage, {} files analyzed",
                coverage * 100.0,
                analyses.len()
            ),
            operations: vfs
                .operations()
                .iter()
                .map(|op| crate::jobs::OrganizeOperation {
                    op_id: op.op_id.clone(),
                    op_type: op.op_type.to_string(),
                    source: op.source.clone(),
                    destination: op.destination.clone(),
                    path: op.path.clone(),
                    new_name: op.new_name.clone(),
                })
                .collect(),
            target_folder: vfs.organization_root().to_string_lossy().to_string(),
            simplification_recommended: None,
        };

        event_emitter(
            "committing",
            &format!("Plan ready: {} operations", plan.operations.len()),
            Some(vec![
                ExpandableDetail {
                    label: "Operations".to_string(),
                    value: plan.operations.len().to_string(),
                },
                ExpandableDetail {
                    label: "Coverage".to_string(),
                    value: format!("{:.1}%", coverage * 100.0),
                },
            ]),
        );

        return Ok(plan);
    }

    // No operations created - check if simplification might help
    let files = vfs.files();
    let dir_count = files.iter().filter(|f| f.is_directory).count();
    let actual_file_count = files.iter().filter(|f| !f.is_directory).count();

    // Compute max depth from file paths
    let root_depth = target_folder.components().count();
    let max_depth = files
        .iter()
        .filter(|f| !f.is_directory)
        .map(|f| {
            let path = std::path::Path::new(&f.path);
            path.components().count().saturating_sub(root_depth)
        })
        .max()
        .unwrap_or(0);

    // Recommend simplification if:
    // - Folder depth > 3 levels, OR
    // - Too many directories relative to files (sparse structure, < 5 files per folder avg)
    // Note: Use multiplication to avoid integer division truncation issues
    const MIN_FILES_PER_FOLDER: usize = 5;
    let simplification_recommended = max_depth > 3
        || (dir_count > 0 && actual_file_count > 0 && actual_file_count < dir_count * MIN_FILES_PER_FOLDER);

    let description = if simplification_recommended {
        format!(
            "No content organization needed for {} files. Folder structure could be simplified (depth: {}, {} directories).",
            file_count, max_depth, dir_count
        )
    } else {
        format!(
            "Folder is already well-organized ({} files).",
            file_count
        )
    };

    Ok(OrganizePlan {
        plan_id: format!("no-ops-{}", chrono::Utc::now().timestamp_millis()),
        description,
        operations: vec![],
        target_folder: vfs.organization_root().to_string_lossy().to_string(),
        simplification_recommended: Some(simplification_recommended),
    })
}

// ============================================================================
// Folder Simplification Mode
// ============================================================================

/// Run folder structure simplification
///
/// This is called when content is already organized but folder structure
/// could be improved (deeply nested, sparse folders, etc.)
pub async fn run_simplification_loop<F>(
    target_folder: &Path,
    event_emitter: F,
) -> Result<OrganizePlan, String>
where
    F: Fn(&str, &str, Option<Vec<ExpandableDetail>>),
{
    use super::prompts::SIMPLIFICATION_SYSTEM_PROMPT;

    // Build VFS
    event_emitter("indexing", "Scanning folder structure...", None);
    let mut vfs = ShadowVFS::new(target_folder).map_err(|e| {
        format!("Failed to scan folder: {}", e)
    })?;

    // Gather structure info for the prompt
    let files = vfs.files();
    let dir_count = files.iter().filter(|f| f.is_directory).count();
    let file_count = files.iter().filter(|f| !f.is_directory).count();

    // Build structure analysis for prompt
    let root_str = target_folder.to_string_lossy();
    let structure_sample: Vec<String> = files
        .iter()
        .take(100)
        .map(|f| f.path.replace(&*root_str, ""))
        .collect();

    let context = format!(
        r#"# Folder Structure Simplification

## Target Folder
{}

## Statistics
- Files: {}
- Directories: {}

## Sample Paths (first 100)
{}

## User Goal
Simplify this folder structure by flattening deep nesting and consolidating sparse folders.
"#,
        target_folder.display(),
        file_count,
        dir_count,
        structure_sample.join("\n")
    );

    event_emitter("thinking", "Analyzing structure for simplification...", Some(vec![
        ExpandableDetail { label: "Files".to_string(), value: file_count.to_string() },
        ExpandableDetail { label: "Directories".to_string(), value: dir_count.to_string() },
    ]));

    // Get API key
    let api_key = crate::ai::CredentialManager::get_api_key("anthropic")
        .map_err(|_| "Anthropic API key not configured".to_string())?;

    // Initialize client
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let rate_limiter = RateLimitManager::new();
    let tools = get_v2_organize_tools();

    // Build messages
    let mut messages = vec![ToolMessage {
        role: "user".to_string(),
        content: vec![ToolMessageContent::text(&context)],
    }];

    // Agent loop (max 5 iterations for simplification)
    for iteration in 0..5 {
        if iteration > 0 {
            let delay = rate_limiter.get_delay();
            tokio::time::sleep(delay).await;
        }

        event_emitter("planning", &format!("Simplification iteration {}", iteration + 1), None);

        // Build request
        let request = ToolApiRequest {
            model: ClaudeModel::Sonnet.as_str().to_string(),
            max_tokens: MAX_TOKENS,
            system: SIMPLIFICATION_SYSTEM_PROMPT.to_string(),
            messages: messages.clone(),
            tools: Some(tools.clone()),
        };

        // Send request
        let response = client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("API error: {}", error_text));
        }

        let api_response: ToolApiResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Collect assistant content for the conversation
        let mut assistant_content = Vec::new();
        // Collect tool results to feed back to the model
        let mut tool_results: Vec<ToolMessageContent> = Vec::new();

        // Process tool calls
        for block in &api_response.content {
            if let ContentBlockResponse::ToolUse { id, name, input } = block {
                eprintln!("[SimplifyLoop] Tool use: {}", name);

                // Emit event for UI
                match name.as_str() {
                    "apply_organization_rules" => {
                        let rules = input.get("rules").and_then(|v| v.as_array());
                        let count = rules.map(|a| a.len()).unwrap_or(0);
                        event_emitter(
                            "applying_rules",
                            &format!("Applying {} simplification rules", count),
                            Some(vec![ExpandableDetail {
                                label: "Rules".to_string(),
                                value: count.to_string(),
                            }]),
                        );
                    }
                    "commit_plan" => {
                        event_emitter("committing", "Finalizing simplification plan...", None);
                    }
                    _ => {}
                }

                // Execute tool
                let result = execute_v2_tool(name, input, &mut vfs);

                match result {
                    V2ToolResult::Commit(plan) => {
                        eprintln!(
                            "[SimplifyLoop] Plan committed: {} operations",
                            plan.operations.len()
                        );
                        event_emitter(
                            "committing",
                            &format!("Simplification plan: {} operations", plan.operations.len()),
                            None,
                        );
                        return Ok(plan);
                    }
                    V2ToolResult::Continue(output) => {
                        // Add tool result to feed back to the model
                        tool_results.push(ToolMessageContent::tool_result(id, &output, false));
                    }
                    V2ToolResult::Error(err) => {
                        eprintln!("[SimplifyLoop] Tool error: {}", err);
                        event_emitter("error", &format!("Tool error: {}", err), None);
                        // Still add the error as a tool result so the model knows what happened
                        tool_results.push(ToolMessageContent::tool_result(id, &format!("Error: {}", err), true));
                    }
                }
            }
        }

        // Add assistant response
        for block in api_response.content.iter() {
            match block {
                ContentBlockResponse::Text { text } => {
                    assistant_content.push(ToolMessageContent::text(text));
                }
                ContentBlockResponse::ToolUse { id, name, input } => {
                    assistant_content.push(ToolMessageContent::tool_use(id, name, input));
                }
            }
        }

        messages.push(ToolMessage {
            role: "assistant".to_string(),
            content: assistant_content,
        });

        // Add tool results as user message so the model can see them
        if !tool_results.is_empty() {
            messages.push(ToolMessage {
                role: "user".to_string(),
                content: tool_results,
            });
        }

        // Check if we should end
        if api_response.stop_reason == "end_turn" {
            break;
        }
    }

    // If we have operations, commit them
    if !vfs.operations().is_empty() {
        let plan = OrganizePlan {
            plan_id: format!("simplify-{}", chrono::Utc::now().timestamp_millis()),
            description: format!(
                "Folder structure simplification: {} operations",
                vfs.operations().len()
            ),
            operations: vfs
                .operations()
                .iter()
                .map(|op| crate::jobs::OrganizeOperation {
                    op_id: op.op_id.clone(),
                    op_type: op.op_type.to_string(),
                    source: op.source.clone(),
                    destination: op.destination.clone(),
                    path: op.path.clone(),
                    new_name: op.new_name.clone(),
                })
                .collect(),
            target_folder: vfs.organization_root().to_string_lossy().to_string(),
            simplification_recommended: None,
        };

        event_emitter("committing", &format!("Plan ready: {} operations", plan.operations.len()), None);
        return Ok(plan);
    }

    // No simplification needed
    Ok(OrganizePlan {
        plan_id: format!("no-simplify-{}", chrono::Utc::now().timestamp_millis()),
        description: "Folder structure is already optimal.".to_string(),
        operations: vec![],
        target_folder: vfs.organization_root().to_string_lossy().to_string(),
        simplification_recommended: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require API key and network access
    // These tests verify the module structure and basic functionality

    #[test]
    fn test_tool_message_content() {
        let text = ToolMessageContent::text("Hello");
        assert!(matches!(text, ToolMessageContent::Text { .. }));

        let tool_use = ToolMessageContent::tool_use(
            "123",
            "test_tool",
            &serde_json::json!({"key": "value"}),
        );
        assert!(matches!(tool_use, ToolMessageContent::ToolUse { .. }));

        let result = ToolMessageContent::tool_result("123", "success", false);
        assert!(matches!(result, ToolMessageContent::ToolResult { .. }));
    }
}
