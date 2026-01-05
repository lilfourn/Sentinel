//! OpenAI Worker Module
//!
//! Parallel workers using OpenAI GPT-5-nano for document analysis.
//! Each worker analyzes a batch of files (5 per batch) and returns
//! structured analysis including suggested filenames.

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
    #[serde(alias = "file_path")]
    pub file_path: String,
    /// Original filename
    #[serde(alias = "old_name")]
    pub old_name: String,
    /// AI-suggested new filename (with extension)
    #[serde(alias = "new_name")]
    pub new_name: String,
    /// Content summary for folder organization
    #[serde(alias = "summary")]
    pub summary: String,
    /// Key entities extracted (company names, dates, amounts)
    #[serde(alias = "entities")]
    pub entities: Vec<String>,
    /// Document type classification
    #[serde(alias = "doc_type")]
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

/// Calculate optimal worker count based on file count
/// GPT-5-nano allows 6K RPM (100 req/sec), so we can be aggressive with parallelism
pub fn calculate_worker_count(file_count: usize) -> usize {
    match file_count {
        0..=10 => 8,
        11..=25 => 16,
        26..=50 => 32,
        51..=100 => 48,
        _ => 64,
    }
}

/// Split files into batches for workers
pub fn create_file_batches(files: Vec<FileContent>, batch_size: usize) -> Vec<Vec<FileContent>> {
    files
        .chunks(batch_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

/// Run multiple workers in parallel with shared HTTP client (no progress tracking)
pub async fn run_parallel_workers(
    api_key: String,
    batches: Vec<Vec<FileContent>>,
    max_concurrent: usize,
) -> Vec<Result<Vec<FileAnalysis>, String>> {
    run_parallel_workers_with_progress(api_key, batches, max_concurrent, None).await
}

/// Run multiple workers in parallel with real-time progress tracking
/// Uses FuturesUnordered to emit progress as each batch completes
pub async fn run_parallel_workers_with_progress(
    api_key: String,
    batches: Vec<Vec<FileContent>>,
    max_concurrent: usize,
    progress_sender: Option<tokio::sync::mpsc::Sender<(usize, usize)>>,
) -> Vec<Result<Vec<FileAnalysis>, String>> {
    use futures::stream::{FuturesUnordered, StreamExt};

    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let api_key = Arc::new(api_key);
    let total_batches = batches.len();

    // Create SHARED HTTP client (reuses connections, avoids TLS overhead)
    let shared_client = Arc::new(
        Client::builder()
            .timeout(Duration::from_secs(60))
            .pool_max_idle_per_host(max_concurrent)
            .build()
            .expect("Failed to create HTTP client")
    );

    // Use FuturesUnordered to process results as they complete
    let mut futures = FuturesUnordered::new();

    for (batch_id, batch) in batches.into_iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let key = Arc::clone(&api_key);
        let client = Arc::clone(&shared_client);
        let file_count = batch.len();

        futures.push(tokio::spawn(async move {
            // Acquire semaphore permit
            let _permit = sem.acquire().await.expect("Semaphore closed");

            eprintln!("[OpenAI Worker {}] Processing batch of {} files", batch_id, file_count);

            let result = analyze_batch_with_client(&client, &key, batch).await;

            match &result {
                Ok(analyses) => {
                    eprintln!("[OpenAI Worker {}] Completed: {} files analyzed", batch_id, analyses.len());
                }
                Err(e) => {
                    eprintln!("[OpenAI Worker {}] Failed: {}", batch_id, e);
                }
            }

            // Minimal delay (10ms) for rate limiting
            tokio::time::sleep(Duration::from_millis(10)).await;

            (batch_id, file_count, result)
        }));
    }

    // Collect results as they complete and emit progress
    let mut results: Vec<Option<Result<Vec<FileAnalysis>, String>>> = vec![None; total_batches];
    let mut completed = 0;
    let mut files_analyzed = 0;

    while let Some(task_result) = futures.next().await {
        match task_result {
            Ok((batch_id, file_count, result)) => {
                files_analyzed += file_count;
                results[batch_id] = Some(result);
            }
            Err(e) => {
                eprintln!("[OpenAI] Task join error: {}", e);
                // Find first empty slot for error
                if let Some(slot) = results.iter_mut().find(|r| r.is_none()) {
                    *slot = Some(Err(format!("Worker task failed: {}", e)));
                }
            }
        }
        completed += 1;

        // Send progress update
        if let Some(ref sender) = progress_sender {
            let _ = sender.send((completed, total_batches)).await;
        }

        eprintln!("[OpenAI] Progress: {}/{} batches complete ({} files)", completed, total_batches, files_analyzed);
    }

    // Unwrap results (all should be Some now)
    results.into_iter().map(|r| r.unwrap_or_else(|| Err("Missing result".to_string()))).collect()
}

/// Analyze a batch using a shared HTTP client (avoids creating new clients per batch)
async fn analyze_batch_with_client(
    client: &Client,
    api_key: &str,
    files: Vec<FileContent>,
) -> Result<Vec<FileAnalysis>, String> {
    if files.is_empty() {
        return Ok(vec![]);
    }

    let model = "gpt-5-nano-2025-08-07";  // Fast, cheap model for batch analysis

    // Build prompt with all files in batch
    let file_contents = files
        .iter()
        .map(|f| {
            format!(
                "=== FILE: {} ===\nExtension: {}\nContent:\n{}\n",
                f.filename,
                f.extension,
                if f.content.len() > 1000 {
                    format!("{}...[truncated]", &f.content[..1000])
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

    // Call OpenAI API using shared client with timeout
    eprintln!("[OpenAI] Sending request for {} files to model {}", files.len(), model);

    let request_future = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": "You are a document analysis assistant. Respond only with valid JSON arrays."},
                {"role": "user", "content": prompt}
            ],
            "max_completion_tokens": 25000
        }))
        .send();

    // Wrap in timeout to prevent indefinite hang
    let response = match tokio::time::timeout(Duration::from_secs(90), request_future).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            eprintln!("[OpenAI] Request error: {}", e);
            return Err(format!("OpenAI API request failed: {}", e));
        }
        Err(_) => {
            eprintln!("[OpenAI] Request TIMEOUT after 90 seconds");
            return Err("OpenAI API request timed out after 90 seconds".to_string());
        }
    };

    eprintln!("[OpenAI] Got response with status: {}", response.status());

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        eprintln!("[OpenAI] API error: {} - {}", status, text);
        return Err(format!("OpenAI API error ({}): {}", status, text));
    }

    #[derive(serde::Deserialize, Debug)]
    struct OpenAIResponse {
        choices: Vec<Choice>,
    }
    #[derive(serde::Deserialize, Debug)]
    struct Choice {
        message: Message,
        #[allow(dead_code)] // Required for JSON deserialization but value not used
        finish_reason: Option<String>,
    }
    #[derive(serde::Deserialize, Debug)]
    struct Message {
        content: Option<String>,
    }

    let response_text = response.text().await
        .map_err(|e| format!("Failed to read OpenAI response: {}", e))?;

    let api_response: OpenAIResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

    let content = api_response.choices.first()
        .and_then(|c| c.message.content.clone())
        .ok_or_else(|| "No response from OpenAI".to_string())?;

    if content.trim().is_empty() {
        return Err("OpenAI returned empty content".to_string());
    }

    // Parse JSON response
    let json_str = super::utils::extract_json_array(&content)
        .map_err(|e| format!("Failed to extract JSON array: {}", e))?;
    let mut analyses: Vec<FileAnalysis> = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse analysis JSON: {}", e))?;

    // Ensure file_path is set correctly from our input
    for (analysis, file) in analyses.iter_mut().zip(files.iter()) {
        analysis.file_path = file.path.to_string_lossy().to_string();
        if analysis.old_name.is_empty() {
            analysis.old_name = file.filename.clone();
        }
    }

    Ok(analyses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::utils::extract_json_array;

    #[test]
    fn test_calculate_worker_count() {
        // 0..=10 => 8 workers
        assert_eq!(calculate_worker_count(5), 8);
        assert_eq!(calculate_worker_count(10), 8);
        // 11..=25 => 16 workers
        assert_eq!(calculate_worker_count(15), 16);
        assert_eq!(calculate_worker_count(25), 16);
        // 26..=50 => 32 workers
        assert_eq!(calculate_worker_count(30), 32);
        assert_eq!(calculate_worker_count(50), 32);
        // 51..=100 => 48 workers
        assert_eq!(calculate_worker_count(75), 48);
        assert_eq!(calculate_worker_count(100), 48);
        // >100 => 64 workers
        assert_eq!(calculate_worker_count(150), 64);
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
