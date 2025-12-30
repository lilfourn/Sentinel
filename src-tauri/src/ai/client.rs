use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::credentials::CredentialManager;

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
            ClaudeModel::Haiku => "claude-3-5-haiku-latest",
            ClaudeModel::Sonnet => "claude-sonnet-4-5",
        }
    }
}

/// Cache control for Anthropic prompt caching
/// See: https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching
#[derive(Serialize, Clone)]
pub struct CacheControl {
    #[serde(rename = "type")]
    control_type: String,
}

impl CacheControl {
    /// Create an ephemeral cache control marker
    /// Cached content expires after 5 minutes of inactivity
    pub fn ephemeral() -> Self {
        Self {
            control_type: "ephemeral".to_string(),
        }
    }
}

/// Message content for API request with optional cache control
#[derive(Serialize, Clone)]
pub struct MessageContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

impl MessageContent {
    /// Create a text content block (no caching)
    pub fn text(text: &str) -> Self {
        Self {
            content_type: "text".to_string(),
            text: text.to_string(),
            cache_control: None,
        }
    }

    /// Create a text content block with ephemeral caching
    /// Use this for large, repeated context like file trees
    pub fn text_cached(text: &str) -> Self {
        Self {
            content_type: "text".to_string(),
            text: text.to_string(),
            cache_control: Some(CacheControl::ephemeral()),
        }
    }
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
                content: vec![MessageContent::text(user_message)],
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

    /// Suggest naming conventions for a folder using Claude Haiku (fast)
    pub async fn suggest_naming_conventions(
        &self,
        folder_path: &str,
        file_listing: &str,
    ) -> Result<super::naming::NamingConventionSuggestions, String> {
        let user_prompt = super::prompts::build_naming_convention_prompt(folder_path, file_listing);

        eprintln!("[AI] Suggesting naming conventions for: {}", folder_path);

        let response = self
            .send_message(
                ClaudeModel::Haiku,
                super::prompts::NAMING_CONVENTION_SYSTEM_PROMPT,
                &user_prompt,
                1024,
            )
            .await?;

        eprintln!("[AI] Naming conventions response length: {} chars", response.len());

        // Parse the JSON response
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawResponse {
            total_files_analyzed: u32,
            suggestions: Vec<super::naming::NamingConvention>,
        }

        let raw: RawResponse = super::json_parser::extract_json(&response)
            .map_err(|e| format!("Failed to parse naming conventions: {}", e))?;

        Ok(super::naming::NamingConventionSuggestions {
            folder_path: folder_path.to_string(),
            total_files_analyzed: raw.total_files_analyzed,
            suggestions: raw.suggestions,
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
                content: vec![MessageContent::text("test")],
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
