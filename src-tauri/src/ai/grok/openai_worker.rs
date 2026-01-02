//! OpenAI Worker Module
//!
//! Parallel workers using OpenAI GPT-5-nano for document analysis.
//! Each worker analyzes a batch of files (5 per batch) and returns
//! structured analysis including suggested filenames.

use super::utils::extract_json_array;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// Result of analyzing a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAnalysis {
    /// Original file path
    pub file_path: String,
    /// Original filename
    pub old_name: String,
    /// AI-suggested new filename (with extension)
    pub new_name: String,
    /// Content summary for folder organization
    pub summary: String,
    /// Key entities extracted (company names, dates, amounts)
    pub entities: Vec<String>,
    /// Document type classification
    pub doc_type: String,
}

/// File content to analyze
#[derive(Debug, Clone)]
pub struct FileContent {
    /// Full path to file
    pub path: PathBuf,
    /// Original filename
    pub filename: String,
    /// Extracted text content
    pub content: String,
    /// File extension
    pub extension: String,
}

/// OpenAI worker for parallel file analysis
pub struct OpenAIWorker {
    client: Client,
    api_key: String,
    model: String,
}

impl OpenAIWorker {
    /// Create a new OpenAI worker
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to create HTTP client"),
            api_key,
            model: "gpt-5-nano-2025-08-07".to_string(),
        }
    }

    /// Analyze a batch of files
    pub async fn analyze_batch(&self, files: Vec<FileContent>) -> Result<Vec<FileAnalysis>, String> {
        if files.is_empty() {
            return Ok(vec![]);
        }

        // Build prompt with all files in batch
        let file_contents = files
            .iter()
            .map(|f| {
                format!(
                    "=== FILE: {} ===\nExtension: {}\nContent:\n{}\n",
                    f.filename,
                    f.extension,
                    // Limit content per file to ~2000 chars to stay within context
                    if f.content.len() > 2000 {
                        format!("{}...[truncated]", &f.content[..2000])
                    } else {
                        f.content.clone()
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Analyze these documents and return a JSON array with one object per file.

[
  {{
    "file_path": "/path/to/file",
    "old_name": "scan001.pdf",
    "new_name": "Acme-Corp-Invoice-2024-03-15-5432.pdf",
    "summary": "Invoice from Acme Corporation dated March 15, 2024 for $5,432.00 for consulting services.",
    "entities": ["Acme-Corp", "2024-03-15", "$5432"],
    "doc_type": "invoice"
  }}
]

NAMING RULES for new_name:
- Use HYPHENS (-) instead of spaces, NEVER use spaces or underscores
- Include DATE (YYYY-MM-DD or YYYY-MM) when document has important date
- Start with primary entity (company, person, property)
- End with document type if it fits
- Keep original file extension
- Max 60 characters before extension
- NO generic names like "document", "scan", "file"

GOOD EXAMPLES:
- "Acme-Corp-Invoice-2024-03-15-5432.pdf"
- "Smith-John-Employment-Contract-2024-01.pdf"
- "123-Main-St-Lease-Agreement-2024.docx"
- "Q1-2024-Financial-Report.xlsx"
- "Project-Phoenix-Proposal-Draft.pdf"

BAD EXAMPLES:
- "scan001.pdf" (meaningless)
- "Document 1.pdf" (spaces, generic)
- "invoice.pdf" (no context)
- "report_final.pdf" (underscores, generic)

doc_type must be one of: invoice, contract, report, letter, form, receipt, statement, proposal, presentation, spreadsheet, manual, certificate, license, permit, application, resume, photo, diagram, drawing, unknown

FILES TO ANALYZE:
{}

Return ONLY the JSON array, no other text."#,
            file_contents
        );

        // Call OpenAI API
        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": self.model,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 2000,
                "temperature": 0.3
            }))
            .send()
            .await
            .map_err(|e| format!("OpenAI API request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("OpenAI API error ({}): {}", status, text));
        }

        #[derive(Deserialize)]
        struct OpenAIResponse {
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

        let api_response: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

        let content = api_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or("No response from OpenAI")?;

        // Parse JSON response
        let json_str = extract_json_array(&content)
            .map_err(|e| format!("Failed to extract JSON array: {}. Content: {}", e, content))?;
        let mut analyses: Vec<FileAnalysis> = serde_json::from_str(&json_str)
            .map_err(|e| format!("Failed to parse analysis JSON: {}. Content: {}", e, content))?;

        // Ensure file_path is set correctly from our input
        for (analysis, file) in analyses.iter_mut().zip(files.iter()) {
            analysis.file_path = file.path.to_string_lossy().to_string();
            // Ensure old_name matches what we sent
            if analysis.old_name.is_empty() {
                analysis.old_name = file.filename.clone();
            }
        }

        Ok(analyses)
    }
}

/// Calculate optimal worker count based on file count
pub fn calculate_worker_count(file_count: usize) -> usize {
    match file_count {
        0..=10 => 4,
        11..=25 => 8,
        26..=50 => 16,
        51..=100 => 24,
        _ => 32,
    }
}

/// Split files into batches for workers
pub fn create_file_batches(files: Vec<FileContent>, batch_size: usize) -> Vec<Vec<FileContent>> {
    files
        .chunks(batch_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

/// Run multiple workers in parallel with rate limiting
pub async fn run_parallel_workers(
    api_key: String,
    batches: Vec<Vec<FileContent>>,
    max_concurrent: usize,
) -> Vec<Result<Vec<FileAnalysis>, String>> {
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let api_key = Arc::new(api_key);

    let tasks: Vec<_> = batches
        .into_iter()
        .enumerate()
        .map(|(batch_id, batch)| {
            let sem = Arc::clone(&semaphore);
            let key = Arc::clone(&api_key);

            tokio::spawn(async move {
                // Acquire semaphore permit
                let _permit = sem.acquire().await.expect("Semaphore closed");

                tracing::info!(
                    "[OpenAI Worker {}] Processing batch of {} files",
                    batch_id,
                    batch.len()
                );

                let worker = OpenAIWorker::new((*key).clone());
                let result = worker.analyze_batch(batch).await;

                match &result {
                    Ok(analyses) => {
                        tracing::info!(
                            "[OpenAI Worker {}] Completed: {} files analyzed",
                            batch_id,
                            analyses.len()
                        );
                    }
                    Err(e) => {
                        tracing::error!("[OpenAI Worker {}] Failed: {}", batch_id, e);
                    }
                }

                // Rate limiting: minimal delay between batches for faster throughput
                tokio::time::sleep(Duration::from_millis(50)).await;

                result
            })
        })
        .collect();

    // Collect results
    let mut results = Vec::new();
    for task in tasks {
        match task.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(Err(format!("Worker task failed: {}", e))),
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_worker_count() {
        assert_eq!(calculate_worker_count(5), 2);
        assert_eq!(calculate_worker_count(10), 2);
        assert_eq!(calculate_worker_count(15), 5);
        assert_eq!(calculate_worker_count(25), 5);
        assert_eq!(calculate_worker_count(30), 10);
        assert_eq!(calculate_worker_count(75), 15);
        assert_eq!(calculate_worker_count(150), 20);
    }

    #[test]
    fn test_create_file_batches() {
        let files: Vec<FileContent> = (0..12)
            .map(|i| FileContent {
                path: PathBuf::from(format!("file{}.pdf", i)),
                filename: format!("file{}.pdf", i),
                content: "test content".to_string(),
                extension: "pdf".to_string(),
            })
            .collect();

        let batches = create_file_batches(files, 5);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 5);
        assert_eq!(batches[1].len(), 5);
        assert_eq!(batches[2].len(), 2);
    }

    #[test]
    fn test_extract_json_array() {
        let content = "Here is the result:\n```json\n[{\"test\": 1}]\n```";
        let result = extract_json_array(content).unwrap();
        assert_eq!(result, "[{\"test\": 1}]");

        let plain = "[{\"a\": 1}, {\"b\": 2}]";
        assert_eq!(extract_json_array(plain).unwrap(), plain);
    }
}
