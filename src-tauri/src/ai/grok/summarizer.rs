//! Grok Summarizer Module
//!
//! Uses Grok grok-4-1-fast with low temperature (0.1) to format
//! OpenAI worker outputs into the exact DocumentAnalysis format
//! expected by the orchestrator agent.

use super::openai_worker::FileAnalysis;
use super::types::{AnalysisMethod, DocumentAnalysis, DocumentType};
use super::utils::extract_json_array;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

/// Grok summarizer for consistent output formatting
pub struct GrokSummarizer {
    client: Client,
    api_key: String,
    model: String,
}

impl GrokSummarizer {
    /// Create a new Grok summarizer
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to create HTTP client"),
            api_key,
            model: "grok-4-1-fast".to_string(),
        }
    }

    /// Convert OpenAI worker outputs to DocumentAnalysis format
    /// Uses low temperature (0.1) for consistent, predictable formatting
    pub async fn format_for_orchestrator(
        &self,
        analyses: Vec<FileAnalysis>,
    ) -> Result<Vec<DocumentAnalysis>, String> {
        if analyses.is_empty() {
            return Ok(vec![]);
        }

        // For small batches, convert directly without API call
        if analyses.len() <= 10 {
            return Ok(self.direct_convert(analyses));
        }

        // For larger batches, use Grok to validate and normalize
        let input_json = serde_json::to_string_pretty(&analyses)
            .map_err(|e| format!("Failed to serialize analyses: {}", e))?;

        let prompt = format!(
            r#"You are a formatting agent. Convert these file analyses to the exact DocumentAnalysis format.

Temperature: 0.1 (be precise, no creativity)

Input analyses:
{}

Output format (JSON array):
[
  {{
    "file_path": "...",
    "file_name": "original filename",
    "content_summary": "2-4 sentences describing the document content, key entities, dates, amounts",
    "document_type": "invoice|contract|report|letter|form|receipt|statement|proposal|presentation|spreadsheet|manual|certificate|license|permit|application|resume|photo|diagram|drawing|unknown",
    "key_entities": ["Entity-1", "Entity-2", "2024-03-15", "$5432"],
    "suggested_name": "Entity-Description-Date-Type (no extension)",
    "confidence": 0.9
  }}
]

CRITICAL RULES:
1. suggested_name must use HYPHENS (-) not spaces or underscores
2. suggested_name must NOT include file extension
3. key_entities must include ALL specific names, dates, and amounts
4. document_type must be one of the allowed values
5. confidence should be 0.85-0.95 based on content clarity

Return ONLY the JSON array, no other text."#,
            input_json
        );

        let response = self
            .client
            .post("https://api.x.ai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": self.model,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 8000,
                "temperature": 0.1
            }))
            .send()
            .await
            .map_err(|e| format!("Grok API request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            // Fallback to direct conversion if API fails
            tracing::warn!(
                "[GrokSummarizer] API error ({}): {} - using direct conversion",
                status,
                text
            );
            return Ok(self.direct_convert(analyses));
        }

        #[derive(Deserialize)]
        struct GrokResponse {
            choices: Vec<Choice>,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: Message,
        }
        #[derive(Deserialize)]
        struct Message {
            content: String,
        }

        let grok_resp: GrokResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Grok response: {}", e))?;

        let content = grok_resp
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or("No response from Grok")?;

        // Parse the formatted output
        let json_str = extract_json_array(&content).map_err(|e| {
            tracing::warn!("[GrokSummarizer] Failed to extract JSON: {} - using direct conversion", e);
            e
        })?;

        #[derive(Deserialize)]
        struct FormattedAnalysis {
            file_path: String,
            file_name: String,
            content_summary: String,
            document_type: String,
            key_entities: Vec<String>,
            suggested_name: Option<String>,
            confidence: Option<f32>,
        }

        let formatted: Vec<FormattedAnalysis> = serde_json::from_str(&json_str).map_err(|e| {
            tracing::warn!(
                "[GrokSummarizer] Failed to parse formatted JSON: {} - using direct conversion",
                e
            );
            format!("Parse error: {}", e)
        })?;

        Ok(formatted
            .into_iter()
            .map(|f| DocumentAnalysis {
                file_path: f.file_path,
                file_name: f.file_name,
                content_summary: f.content_summary,
                document_type: DocumentType::from_str(&f.document_type),
                key_entities: f.key_entities,
                suggested_name: f.suggested_name,
                confidence: f.confidence.unwrap_or(0.85),
                method: AnalysisMethod::TextExtraction,
            })
            .collect())
    }

    /// Direct conversion without API call (for small batches or fallback)
    fn direct_convert(&self, analyses: Vec<FileAnalysis>) -> Vec<DocumentAnalysis> {
        analyses
            .into_iter()
            .map(|a| {
                // Extract suggested name without extension
                let suggested_name = if a.new_name.is_empty() || a.new_name == a.old_name {
                    None
                } else {
                    // Remove extension from new_name
                    let name = std::path::Path::new(&a.new_name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or(a.new_name.clone());
                    Some(name)
                };

                DocumentAnalysis {
                    file_path: a.file_path,
                    file_name: a.old_name,
                    content_summary: a.summary,
                    document_type: DocumentType::from_str(&a.doc_type),
                    key_entities: a.entities,
                    suggested_name,
                    confidence: 0.85,
                    method: AnalysisMethod::TextExtraction,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_convert() {
        let summarizer = GrokSummarizer::new("test".to_string());

        let analyses = vec![FileAnalysis {
            file_path: "/test/scan001.pdf".to_string(),
            old_name: "scan001.pdf".to_string(),
            new_name: "Acme-Corp-Invoice-2024-03.pdf".to_string(),
            summary: "Invoice from Acme Corporation".to_string(),
            entities: vec!["Acme-Corp".to_string(), "2024-03".to_string()],
            doc_type: "invoice".to_string(),
        }];

        let result = summarizer.direct_convert(analyses);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_name, "scan001.pdf");
        assert_eq!(
            result[0].suggested_name,
            Some("Acme-Corp-Invoice-2024-03".to_string())
        );
        assert_eq!(result[0].document_type, DocumentType::Invoice);
    }

    #[test]
    fn test_direct_convert_no_rename() {
        let summarizer = GrokSummarizer::new("test".to_string());

        let analyses = vec![FileAnalysis {
            file_path: "/test/file.pdf".to_string(),
            old_name: "file.pdf".to_string(),
            new_name: "file.pdf".to_string(), // Same as old
            summary: "Generic document".to_string(),
            entities: vec![],
            doc_type: "unknown".to_string(),
        }];

        let result = summarizer.direct_convert(analyses);
        assert_eq!(result[0].suggested_name, None);
    }
}
