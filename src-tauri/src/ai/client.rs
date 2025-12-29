use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::credentials::CredentialManager;
use super::tools::{ToolDefinition, ToolResult};
use crate::jobs::OrganizePlan;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Claude model identifiers
pub enum ClaudeModel {
    /// Claude 4.5 Haiku - fast, for context gathering
    Haiku,
    /// Claude 4.5 Sonnet - balanced, for rename and organize decisions
    Sonnet,
}

impl ClaudeModel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClaudeModel::Haiku => "claude-haiku-4-5",
            ClaudeModel::Sonnet => "claude-sonnet-4-5",
        }
    }
}

/// Message content for API request
#[derive(Serialize)]
struct MessageContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

/// Message in conversation
#[derive(Serialize)]
struct Message {
    role: String,
    content: Vec<MessageContent>,
}

/// API request body
#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

/// Content block in API response
#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

/// API response body
#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
    #[allow(dead_code)]
    stop_reason: Option<String>,
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

// ============ Tool Use API Structures ============

/// API request with tools support
#[derive(Serialize)]
struct ToolApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<ToolMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
}

/// Message with tool support (can contain multiple content types)
#[derive(Serialize, Clone)]
struct ToolMessage {
    role: String,
    content: Vec<ToolMessageContent>,
}

/// Content block for tool messages
#[derive(Serialize, Clone)]
#[serde(untagged)]
enum ToolMessageContent {
    Text {
        #[serde(rename = "type")]
        content_type: String,
        text: String,
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
    fn text(text: &str) -> Self {
        Self::Text {
            content_type: "text".to_string(),
            text: text.to_string(),
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

    fn tool_result(result: ToolResult) -> Self {
        Self::ToolResult {
            content_type: "tool_result".to_string(),
            tool_use_id: result.tool_use_id,
            content: result.content,
            is_error: result.is_error,
        }
    }
}

/// Extended content block in response (text or tool_use)
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

/// Extended API response with stop_reason
#[derive(Deserialize, Debug)]
struct ToolApiResponse {
    content: Vec<ContentBlockResponse>,
    stop_reason: String,
}

/// Anthropic API client
pub struct AnthropicClient {
    client: Client,
}

impl AnthropicClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Send a message to Claude
    pub async fn send_message(
        &self,
        model: ClaudeModel,
        system_prompt: &str,
        user_message: &str,
        max_tokens: u32,
    ) -> Result<String, String> {
        let api_key = CredentialManager::get_api_key("anthropic")?;

        let request = ApiRequest {
            model: model.as_str().to_string(),
            max_tokens,
            system: system_prompt.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: vec![MessageContent {
                    content_type: "text".to_string(),
                    text: user_message.to_string(),
                }],
            }],
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if let Ok(api_error) = serde_json::from_str::<ApiError>(&error_text) {
                return Err(format!("API error: {}", api_error.error.message));
            }
            return Err(format!("API error ({}): {}", status, error_text));
        }

        let api_response: ApiResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Extract text from response
        let text = api_response
            .content
            .iter()
            .filter_map(|block| {
                if block.content_type == "text" {
                    block.text.clone()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        Ok(text.trim().to_string())
    }

    /// Generate a rename suggestion using Claude Sonnet
    pub async fn suggest_rename(
        &self,
        filename: &str,
        extension: Option<&str>,
        size: u64,
        content_preview: Option<&str>,
    ) -> Result<String, String> {
        let user_prompt = super::prompts::build_rename_prompt(
            filename,
            extension,
            size,
            content_preview,
        );

        self.send_message(
            ClaudeModel::Sonnet,
            super::prompts::RENAME_SYSTEM_PROMPT,
            &user_prompt,
            100, // Short response expected
        )
        .await
    }

    /// Analyze folder context using Claude Haiku (fast)
    pub async fn analyze_folder_context(
        &self,
        folder_path: &str,
        ls_output: &str,
    ) -> Result<String, String> {
        let prompt = super::prompts::build_context_prompt(folder_path, ls_output);

        self.send_message(
            ClaudeModel::Haiku,
            "You are a file organization analyst. Be concise.",
            &prompt,
            500,
        )
        .await
    }

    /// Generate organization plan using Claude Sonnet
    pub async fn generate_organize_plan(
        &self,
        folder_path: &str,
        ls_output: &str,
        user_request: &str,
        context_analysis: Option<&str>,
    ) -> Result<String, String> {
        let prompt = super::prompts::build_organize_prompt(
            folder_path,
            ls_output,
            user_request,
            context_analysis,
        );

        eprintln!("[AI] Generating organize plan for: {}", folder_path);
        eprintln!("[AI] Prompt length: {} chars", prompt.len());

        let response = self.send_message(
            ClaudeModel::Sonnet,
            super::prompts::ORGANIZE_SYSTEM_PROMPT,
            &prompt,
            4096, // Increased for large folder operations
        )
        .await?;

        eprintln!("[AI] Response length: {} chars", response.len());
        eprintln!("[AI] Response preview: {}...", &response.chars().take(200).collect::<String>());

        Ok(response)
    }

    // ============ Agentic Tool-Use Methods ============

    /// Send a message with tools and get response
    async fn send_with_tools(
        &self,
        model: ClaudeModel,
        system_prompt: &str,
        messages: &[ToolMessage],
        tools: Option<&[ToolDefinition]>,
        max_tokens: u32,
    ) -> Result<ToolApiResponse, String> {
        let api_key = CredentialManager::get_api_key("anthropic")?;

        let request = ToolApiRequest {
            model: model.as_str().to_string(),
            max_tokens,
            system: system_prompt.to_string(),
            messages: messages.to_vec(),
            tools: tools.map(|t| t.to_vec()),
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if let Ok(api_error) = serde_json::from_str::<ApiError>(&error_text) {
                return Err(format!("API error: {}", api_error.error.message));
            }
            return Err(format!("API error ({}): {}", status, error_text));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Run agentic organize workflow with tool use
    pub async fn run_agentic_organize<F>(
        &self,
        target_folder: &str,
        user_request: &str,
        event_emitter: F,
    ) -> Result<OrganizePlan, String>
    where
        F: Fn(&str, &str),
    {
        let tools = super::tools::get_organize_tools();
        let system_prompt = super::prompts::AGENTIC_ORGANIZE_SYSTEM_PROMPT;

        let initial_message = format!(
            "Target folder: {}\nUser request: {}\n\nExplore the folder structure and generate an organization plan.",
            target_folder, user_request
        );

        let mut messages = vec![ToolMessage {
            role: "user".to_string(),
            content: vec![ToolMessageContent::text(&initial_message)],
        }];

        let allowed_path = Path::new(target_folder);

        // Agentic loop - max 10 iterations to prevent infinite loops
        for iteration in 0..10 {
            eprintln!("[AgenticLoop] Iteration {}", iteration + 1);
            event_emitter("thinking", &format!("Processing... (step {})", iteration + 1));

            let response = self
                .send_with_tools(
                    ClaudeModel::Sonnet,
                    system_prompt,
                    &messages,
                    Some(&tools),
                    4096,
                )
                .await?;

            eprintln!("[AgenticLoop] stop_reason: {}", response.stop_reason);

            // Check stop reason
            if response.stop_reason == "end_turn" {
                // Extract final JSON from text content
                let text = Self::extract_text_content(&response.content);
                eprintln!("[AgenticLoop] Final response length: {} chars", text.len());

                return Self::parse_organize_plan(&text, target_folder);
            }

            // Handle tool uses
            let mut tool_results: Vec<ToolResult> = Vec::new();
            let mut assistant_content: Vec<ToolMessageContent> = Vec::new();

            for block in &response.content {
                match block {
                    ContentBlockResponse::Text { text } => {
                        eprintln!("[AgenticLoop] Thinking: {}...", &text.chars().take(100).collect::<String>());
                        event_emitter("thinking", text);
                        assistant_content.push(ToolMessageContent::text(text));
                    }
                    ContentBlockResponse::ToolUse { id, name, input } => {
                        eprintln!("[AgenticLoop] Tool use: {} with {:?}", name, input);
                        event_emitter("executing", &format!("Running {}", name));
                        assistant_content.push(ToolMessageContent::tool_use(id, name, input));

                        let result = super::tool_executor::execute_tool(name, input, allowed_path);

                        let tool_result = match result {
                            Ok(output) => {
                                eprintln!("[AgenticLoop] Tool success: {} bytes", output.len());
                                ToolResult::success(id.clone(), output)
                            }
                            Err(e) => {
                                eprintln!("[AgenticLoop] Tool error: {}", e);
                                ToolResult::error(id.clone(), e)
                            }
                        };

                        tool_results.push(tool_result);
                    }
                }
            }

            // Add assistant message with tool uses
            messages.push(ToolMessage {
                role: "assistant".to_string(),
                content: assistant_content,
            });

            // Add tool results as user message
            messages.push(ToolMessage {
                role: "user".to_string(),
                content: tool_results
                    .into_iter()
                    .map(ToolMessageContent::tool_result)
                    .collect(),
            });
        }

        Err("Agentic loop exceeded maximum iterations".to_string())
    }

    /// Extract text from response content blocks
    fn extract_text_content(content: &[ContentBlockResponse]) -> String {
        content
            .iter()
            .filter_map(|block| match block {
                ContentBlockResponse::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Parse organize plan from JSON response
    fn parse_organize_plan(text: &str, target_folder: &str) -> Result<OrganizePlan, String> {
        use super::json_parser::extract_json;
        use crate::jobs::OrganizeOperation;

        #[derive(serde::Deserialize)]
        struct RawPlan {
            description: String,
            operations: Vec<RawOperation>,
        }

        #[derive(serde::Deserialize)]
        struct RawOperation {
            #[serde(rename = "type")]
            op_type: String,
            source: Option<String>,
            destination: Option<String>,
            path: Option<String>,
            #[serde(alias = "newName", alias = "new_name")]
            new_name: Option<String>,
        }

        let raw: RawPlan = extract_json(text)?;

        let operations: Vec<OrganizeOperation> = raw
            .operations
            .into_iter()
            .enumerate()
            .map(|(i, op)| OrganizeOperation {
                op_id: format!("op-{}", i + 1),
                op_type: op.op_type,
                source: op.source,
                destination: op.destination,
                path: op.path,
                new_name: op.new_name,
            })
            .collect();

        Ok(OrganizePlan {
            plan_id: format!("plan-{}", chrono::Utc::now().timestamp_millis()),
            description: raw.description,
            operations,
            target_folder: target_folder.to_string(),
        })
    }

    /// Validate API key by making a minimal request
    pub async fn validate_api_key(api_key: &str) -> Result<bool, String> {
        let client = Client::new();

        let request = ApiRequest {
            model: ClaudeModel::Haiku.as_str().to_string(),
            max_tokens: 10,
            system: "Say 'ok'".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: vec![MessageContent {
                    content_type: "text".to_string(),
                    text: "test".to_string(),
                }],
            }],
        };

        let response = client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        Ok(response.status().is_success())
    }
}

impl Default for AnthropicClient {
    fn default() -> Self {
        Self::new()
    }
}
