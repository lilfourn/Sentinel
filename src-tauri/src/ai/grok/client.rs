//! Grok API Client
//!
//! Handles communication with xAI's Grok API, including:
//! - Vision API for document image analysis
//! - Rate limiting and retry logic
//! - Token usage tracking

use super::types::*;
use super::utils::extract_json_object;
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};

/// Grok API client with rate limiting
pub struct GrokClient {
    client: Client,
    config: GrokConfig,
    rate_limiter: Arc<RateLimiter>,
    tokens_used: AtomicU32,
}

impl GrokClient {
    /// Create a new Grok client
    pub fn new(config: GrokConfig) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        let rate_limiter = Arc::new(RateLimiter::new(
            config.max_concurrent_requests,
            config.requests_per_second,
        ));

        Ok(Self {
            client,
            config,
            rate_limiter,
            tokens_used: AtomicU32::new(0),
        })
    }

    /// Analyze a document image using Grok Vision
    pub async fn analyze_document_image(
        &self,
        image_data: &[u8],
        filename: &str,
        context: Option<&str>,
    ) -> Result<DocumentAnalysis, String> {
        self.rate_limiter.acquire().await;

        let base64_image = base64::engine::general_purpose::STANDARD.encode(image_data);

        // Detect image format from magic bytes
        let mime_type = detect_image_mime(image_data);
        let data_url = format!("data:{};base64,{}", mime_type, base64_image);

        let context_text = context.unwrap_or("");
        let prompt = format!(
            r#"Analyze this document image for intelligent file organization.

Filename: {}
{}

CRITICAL: Extract SPECIFIC names and identifiers, not generic descriptions!

Provide a JSON response:
{{
  "content_summary": "3-4 detailed sentences about: WHO is involved (specific company names like 'Acme Corporation', person names like 'John Smith'), WHAT the document is (specific project like 'Q1 Marketing Campaign', transaction like 'Invoice #12345'), WHEN (specific dates), and any AMOUNTS or numbers mentioned",
  "document_type": "one of: invoice, contract, report, letter, form, receipt, statement, proposal, presentation, spreadsheet, manual, certificate, license, permit, application, resume, photo, diagram, drawing, unknown",
  "key_entities": ["MUST include: specific company names (e.g., 'Acme Corp'), person names (e.g., 'Jane Doe'), project names, dates (e.g., '2024-01-15'), dollar amounts (e.g., '$5,432.00'), reference numbers"],
  "suggested_name": "See naming rules below",
  "confidence": 0.85
}}

## FILE NAMING RULES (for suggested_name):

FORMAT: [Entity]-[Description]-[Date]-[Type]
- Use HYPHENS (-) instead of spaces, NEVER use spaces
- Include date as YYYY-MM or YYYY-MM-DD when document has important date
- Start with the primary entity (company, person, property)
- End with document type
- Keep it descriptive but concise (max 60 chars before extension)

GOOD EXAMPLES:
✅ "Acme-Corp-Invoice-2024-03-15-5432"
✅ "Smith-John-Employment-Contract-2024-01"
✅ "123-Main-St-Lease-Agreement-2024"
✅ "Q1-2024-Financial-Report"
✅ "Project-Phoenix-Proposal-Draft"
✅ "TechStart-NDA-Signed-2024-02"

BAD EXAMPLES:
❌ "scan001" (meaningless)
❌ "Document 1" (spaces, generic)
❌ "invoice.pdf" (no context)
❌ "New Document" (generic)
❌ "IMG_2024" (camera default)

FOCUS ON: Company/client names, project names, people names, specific dates, dollar amounts!"#,
            filename,
            if context_text.is_empty() { String::new() } else { format!("Context: {}", context_text) }
        );

        let request = GrokChatRequest {
            model: self.config.model.clone(),
            messages: vec![GrokMessage {
                role: "user".to_string(),
                content: vec![
                    ContentPart::Text { text: prompt },
                    ContentPart::ImageUrl {
                        image_url: ImageUrlContent {
                            url: data_url,
                            detail: "low".to_string(), // Cost optimization
                        },
                    },
                ],
            }],
            max_tokens: 500,
            temperature: 0.1,
        };

        let response = self.send_request(&request).await?;

        // Track token usage
        self.tokens_used.fetch_add(response.usage.total_tokens, Ordering::Relaxed);

        // Parse the response
        let content = response.choices.first()
            .ok_or("No response from Grok")?
            .message.content.as_str();

        self.parse_analysis_response(content, filename)
    }

    /// Send request with retry logic
    async fn send_request(&self, request: &GrokChatRequest) -> Result<GrokChatResponse, String> {
        let mut retry_delay = Duration::from_secs(2);
        let max_retries = 3;

        for retry in 0..=max_retries {
            if retry > 0 {
                tokio::time::sleep(retry_delay).await;
                retry_delay *= 2;
            }

            let resp = self.client
                .post(format!("{}/v1/chat/completions", self.config.base_url))
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("Content-Type", "application/json")
                .json(request)
                .send()
                .await;

            match resp {
                Ok(r) if r.status() == 429 => {
                    tracing::warn!("Rate limited, retry {}/{}", retry + 1, max_retries);
                    continue;
                }
                Ok(r) if r.status().is_success() => {
                    return r.json().await
                        .map_err(|e| format!("Failed to parse response: {}", e));
                }
                Ok(r) => {
                    let status = r.status();
                    let text = r.text().await.unwrap_or_default();
                    return Err(format!("API error ({}): {}", status, text));
                }
                Err(e) => {
                    if retry == max_retries {
                        return Err(format!("Request failed after retries: {}", e));
                    }
                    continue;
                }
            }
        }

        Err("Max retries exceeded".to_string())
    }

    /// Parse analysis response from Grok
    fn parse_analysis_response(&self, content: &str, filename: &str) -> Result<DocumentAnalysis, String> {
        // Try to extract JSON from response
        let json_str = extract_json_object(content)?;

        #[derive(Deserialize)]
        struct RawAnalysis {
            content_summary: String,
            document_type: String,
            #[serde(default)]
            key_entities: Vec<String>,
            suggested_name: Option<String>,
            #[serde(default = "default_confidence")]
            confidence: f32,
        }

        fn default_confidence() -> f32 { 0.8 }

        let raw: RawAnalysis = serde_json::from_str(&json_str)
            .map_err(|e| format!("Failed to parse JSON: {}. Content: {}", e, content))?;

        Ok(DocumentAnalysis {
            file_path: String::new(), // Set by caller
            file_name: filename.to_string(),
            content_summary: raw.content_summary,
            document_type: DocumentType::from_str(&raw.document_type),
            key_entities: raw.key_entities,
            suggested_name: raw.suggested_name,
            confidence: raw.confidence,
            method: AnalysisMethod::GrokVision,
        })
    }
}

/// Rate limiter for API requests
struct RateLimiter {
    semaphore: Semaphore,
    min_interval: Duration,
    last_request: Mutex<Instant>,
}

impl RateLimiter {
    fn new(max_concurrent: usize, requests_per_second: f32) -> Self {
        Self {
            semaphore: Semaphore::new(max_concurrent),
            min_interval: Duration::from_secs_f32(1.0 / requests_per_second),
            last_request: Mutex::new(Instant::now() - Duration::from_secs(10)),
        }
    }

    async fn acquire(&self) {
        let _permit = self.semaphore.acquire().await.expect("Semaphore closed");

        let wait_time = {
            let mut last = self.last_request.lock().await;
            let elapsed = last.elapsed();
            let wait = self.min_interval.saturating_sub(elapsed);
            *last = Instant::now() + wait;
            wait
        };

        if !wait_time.is_zero() {
            tokio::time::sleep(wait_time).await;
        }
    }
}

// API request/response types

#[derive(Serialize)]
struct GrokChatRequest {
    model: String,
    messages: Vec<GrokMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
struct GrokMessage {
    role: String,
    content: Vec<ContentPart>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrlContent },
}

#[derive(Serialize)]
struct ImageUrlContent {
    url: String,
    detail: String,
}

#[derive(Deserialize)]
struct GrokChatResponse {
    choices: Vec<Choice>,
    usage: Usage,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct Usage {
    total_tokens: u32,
}

/// Detect image MIME type from magic bytes
fn detect_image_mime(data: &[u8]) -> &'static str {
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        "image/gif"
    } else {
        "image/png" // Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_image_mime() {
        assert_eq!(detect_image_mime(&[0x89, 0x50, 0x4E, 0x47]), "image/png");
        assert_eq!(detect_image_mime(&[0xFF, 0xD8, 0xFF]), "image/jpeg");
    }

    #[test]
    fn test_extract_json() {
        let text = r#"Here's the analysis:
```json
{"content_summary": "test", "document_type": "invoice"}
```
That's it."#;
        let json = extract_json_object(text).unwrap();
        assert!(json.contains("content_summary"));
    }

    #[test]
    fn test_document_type_from_str() {
        assert_eq!(DocumentType::from_str("invoice"), DocumentType::Invoice);
        assert_eq!(DocumentType::from_str("INVOICE"), DocumentType::Invoice);
        assert_eq!(DocumentType::from_str("unknown_type"), DocumentType::Unknown);
    }
}
