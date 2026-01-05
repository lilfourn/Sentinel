//! Explore Agent
//!
//! Parallel workers that analyze batches of documents and return summaries.
//! Each explore agent:
//! 1. Receives a batch of files
//! 2. Checks cache for existing analyses
//! 3. **NEW**: Tries text extraction first (PDF, Office docs)
//! 4. Falls back to Grok Vision for scanned/image docs
//! 5. Returns summaries in format: "filename | summary | suggested_name"

use super::cache::ContentCache;
use super::client::GrokClient;
use super::document_parser::{DocumentParser, ExtractionMethod, ParsedDocument};
use super::pdf_renderer::PdfRenderer;
use super::types::*;
use super::vision;
use futures::stream::{self, StreamExt};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;

/// Explore agent for analyzing document batches
#[allow(dead_code)]
pub struct ExploreAgent {
    client: Arc<GrokClient>,
    cache: Arc<ContentCache>,
    pdf_renderer: Arc<PdfRenderer>,
    document_parser: DocumentParser,
    batch_id: usize,
}

impl ExploreAgent {
    /// Create a new explore agent
    pub fn new(
        client: Arc<GrokClient>,
        cache: Arc<ContentCache>,
        pdf_renderer: Arc<PdfRenderer>,
        batch_id: usize,
    ) -> Self {
        Self {
            client,
            cache,
            pdf_renderer,
            document_parser: DocumentParser::new(),
            batch_id,
        }
    }

    /// Process a batch of files in parallel within the batch
    /// Uses semaphore to limit concurrent API calls while maximizing throughput
    pub async fn process_batch<F>(
        &self,
        files: Vec<std::path::PathBuf>,
        progress_callback: F,
    ) -> ExploreResult
    where
        F: Fn(AnalysisProgress) + Send + Sync,
    {
        let start = Instant::now();
        let total = files.len();
        let batch_id = self.batch_id;

        tracing::info!(
            "[ExploreAgent {}] Processing {} files in parallel",
            batch_id,
            total
        );

        // Limit concurrent analyses within batch (API rate limiting)
        let semaphore = Arc::new(Semaphore::new(4)); // 4 concurrent within batch
        let progress_counter = Arc::new(AtomicUsize::new(0));

        // Wrap shared resources in Arcs for parallel access
        let client = Arc::clone(&self.client);
        let cache = Arc::clone(&self.cache);
        let pdf_renderer = Arc::clone(&self.pdf_renderer);

        // Create analysis tasks
        let analysis_tasks: Vec<_> = files
            .into_iter()
            .map(|file_path| {
                let semaphore = Arc::clone(&semaphore);
                let counter = Arc::clone(&progress_counter);
                let client = Arc::clone(&client);
                let cache = Arc::clone(&cache);
                let pdf_renderer = Arc::clone(&pdf_renderer);

                async move {
                    // Acquire semaphore permit
                    let _permit = semaphore.acquire().await.ok()?;

                    // Update progress
                    counter.fetch_add(1, Ordering::Relaxed);

                    // Analyze the file
                    let result = Self::analyze_file_static(
                        &file_path,
                        &client,
                        &cache,
                        &pdf_renderer,
                        batch_id,
                    )
                    .await;

                    Some((file_path, result))
                }
            })
            .collect();

        // Execute all analyses concurrently
        let mut analyses = Vec::new();
        let mut failed_files = Vec::new();
        let mut tokens_used = 0u32;

        // Track consecutive auth errors for early abort
        let mut consecutive_auth_errors = 0;
        const MAX_AUTH_ERRORS: usize = 3;

        let mut analysis_stream = stream::iter(analysis_tasks)
            .buffer_unordered(8); // Allow up to 8 pending futures

        let mut last_progress = 0;
        let mut aborted = false;

        while let Some(result) = analysis_stream.next().await {
            // Emit progress
            let current = progress_counter.load(Ordering::Relaxed);
            if current > last_progress {
                progress_callback(AnalysisProgress {
                    phase: AnalysisPhase::AnalyzingContent,
                    current,
                    total,
                    current_file: None,
                    message: format!("Batch {}: Analyzing {}/{}", batch_id, current, total),
                });
                last_progress = current;
            }

            if let Some((file_path, analysis_result)) = result {
                match analysis_result {
                    Ok((analysis, file_tokens)) => {
                        tokens_used += file_tokens;
                        analyses.push(analysis);
                        // Reset auth error counter on success
                        consecutive_auth_errors = 0;
                    }
                    Err(e) => {
                        // Check if this is an auth error (invalid API key)
                        let is_auth_error = e.contains("Incorrect API key")
                            || e.contains("Invalid API key")
                            || e.contains("invalid_api_key")
                            || e.contains("401")
                            || (e.contains("400") && e.contains("API key"));

                        if is_auth_error {
                            consecutive_auth_errors += 1;
                            tracing::warn!(
                                "[ExploreAgent {}] Auth error ({}/{}): {}",
                                batch_id,
                                consecutive_auth_errors,
                                MAX_AUTH_ERRORS,
                                e
                            );

                            // Abort early on repeated auth errors
                            if consecutive_auth_errors >= MAX_AUTH_ERRORS {
                                tracing::error!(
                                    "[ExploreAgent {}] ABORTING: {} consecutive auth errors. Check your xAI API key.",
                                    batch_id,
                                    consecutive_auth_errors
                                );
                                // Mark remaining files as failed with clear error message
                                let abort_error = "Batch aborted: Invalid or missing xAI API key. Please check Settings > AI Keys.".to_string();
                                failed_files.push((file_path.to_string_lossy().to_string(), abort_error));
                                aborted = true;
                                break;
                            }
                        }

                        tracing::warn!(
                            "[ExploreAgent {}] Failed to analyze {}: {}",
                            batch_id,
                            file_path.display(),
                            e
                        );
                        failed_files.push((file_path.to_string_lossy().to_string(), e));
                    }
                }
            }
        }

        // If aborted, drop remaining tasks
        if aborted {
            drop(analysis_stream);
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        tracing::info!(
            "[ExploreAgent {}] Completed: {} analyzed, {} failed, {} tokens, {}ms",
            batch_id,
            analyses.len(),
            failed_files.len(),
            tokens_used,
            duration_ms
        );

        ExploreResult {
            batch_id,
            analyses,
            failed_files,
            total_tokens_used: tokens_used,
            duration_ms,
        }
    }

    /// Static version of analyze_file for parallel execution
    async fn analyze_file_static(
        path: &Path,
        client: &Arc<GrokClient>,
        cache: &Arc<ContentCache>,
        pdf_renderer: &Arc<PdfRenderer>,
        batch_id: usize,
    ) -> Result<(DocumentAnalysis, u32), String> {
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let ext = path.extension().and_then(|e| e.to_str());

        tracing::debug!(
            "[ExploreAgent {}] Analyzing file: {} (ext: {:?})",
            batch_id,
            filename,
            ext
        );

        // 1. Check cache first
        if let Ok(Some(mut cached)) = cache.get_cached(path) {
            cached.file_path = path.to_string_lossy().to_string();
            cached.method = AnalysisMethod::Cached;
            tracing::debug!(
                "[ExploreAgent {}] Cache HIT: {}",
                batch_id,
                filename
            );
            return Ok((cached, 0));
        }

        // 2. Try text extraction first (pure Rust, no API call needed)
        let ext_str = ext.map(|s| s.to_string());
        if super::document_parser::is_parseable(ext) {
            let path_for_parse = path.to_path_buf();
            let parsed_result = tokio::task::spawn_blocking(move || {
                let parser = DocumentParser::new();
                parser.parse(&path_for_parse)
            })
            .await
            .map_err(|e| format!("Parse task failed: {}", e))?;

            if let Ok(parsed) = parsed_result {
                if parsed.text.len() >= 100 && parsed.method != ExtractionMethod::Failed {
                    // Successfully extracted text - analyze with Grok text API
                    let text_preview: String = parsed.text.chars().take(4000).collect();

                    tracing::debug!(
                        "[ExploreAgent {}] Text extraction SUCCESS: {} chars from {}",
                        batch_id,
                        text_preview.len(),
                        filename
                    );

                    return Self::analyze_text_content_static(
                        path,
                        &filename,
                        &text_preview,
                        ext_str.as_deref(),
                        client,
                        cache,
                        batch_id,
                    )
                    .await;
                }
            }
        }

        // 3. Fall back to Vision API for scanned/image documents
        if vision::is_analyzable_extension(ext) {
            tracing::debug!(
                "[ExploreAgent {}] Falling back to Vision API for {}",
                batch_id,
                filename
            );
            return Self::analyze_with_vision_static(
                path,
                &filename,
                ext,
                client,
                cache,
                pdf_renderer,
                batch_id,
            )
            .await;
        }

        Err(format!(
            "Cannot analyze file type: {:?}",
            ext.unwrap_or("unknown")
        ))
    }

    /// Analyze text content using Grok text API (static version)
    async fn analyze_text_content_static(
        path: &Path,
        filename: &str,
        text: &str,
        _ext: Option<&str>,
        _client: &Arc<GrokClient>,
        cache: &Arc<ContentCache>,
        batch_id: usize,
    ) -> Result<(DocumentAnalysis, u32), String> {
        use reqwest::Client;
        use serde_json::json;

        let prompt = format!(
            r#"Analyze this document and extract SPECIFIC information for file organization.

FILENAME: {}

DOCUMENT CONTENT:
{}

Respond with ONLY this JSON format:
{{
  "content_summary": "3-4 sentences covering WHO, WHAT, WHEN, and amounts found",
  "document_type": "one of: invoice, contract, report, letter, form, receipt, statement, proposal, presentation, spreadsheet, manual, certificate, resume, photo, unknown",
  "key_entities": ["specific names, dates, amounts found"],
  "suggested_name": "Entity-Description-Date format, use hyphens not spaces"
}}"#,
            filename, text
        );

        let api_key = std::env::var("XAI_API_KEY")
            .or_else(|_| std::env::var("GROK_API_KEY"))
            .or_else(|_| std::env::var("VITE_XAI_API_KEY"))
            .map_err(|_| "No Grok API key found")?;

        let http_client = Client::new();
        let response = http_client
            .post("https://api.x.ai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&json!({
                "model": "grok-4-1-fast",
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 500,
                "temperature": 0.1
            }))
            .send()
            .await
            .map_err(|e| format!("API request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Grok API error ({}): {}", status, error_text));
        }

        #[derive(serde::Deserialize)]
        struct GrokResponse { choices: Vec<Choice> }
        #[derive(serde::Deserialize)]
        struct Choice { message: Message }
        #[derive(serde::Deserialize)]
        struct Message { content: String }

        let grok_resp: GrokResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Grok response: {}", e))?;

        let content = grok_resp
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or("No response from Grok")?;

        // Parse JSON from response
        let analysis = Self::parse_json_analysis_response(path, filename, &content)?;

        // Estimate tokens: ~input/4 + output/4 + overhead
        let estimated_tokens = ((text.len() / 4) + (content.len() / 4) + 100) as u32;

        // Cache the result
        if let Err(e) = cache.store(path, &analysis, estimated_tokens) {
            tracing::warn!(
                "[ExploreAgent {}] Failed to cache analysis: {}",
                batch_id,
                e
            );
        }

        Ok((analysis, estimated_tokens))
    }

    /// Parse JSON analysis response
    fn parse_json_analysis_response(
        path: &Path,
        filename: &str,
        content: &str,
    ) -> Result<DocumentAnalysis, String> {
        #[derive(serde::Deserialize)]
        struct AnalysisResponse {
            content_summary: String,
            document_type: String,
            key_entities: Vec<String>,
            suggested_name: Option<String>,
        }

        // Extract JSON from response (handle markdown code blocks)
        let json_str = if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                &content[start..=end]
            } else {
                content
            }
        } else {
            content
        };

        let parsed: AnalysisResponse = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse analysis JSON: {}", e))?;

        let document_type = match parsed.document_type.to_lowercase().as_str() {
            "invoice" => DocumentType::Invoice,
            "contract" => DocumentType::Contract,
            "report" => DocumentType::Report,
            "letter" => DocumentType::Letter,
            "form" => DocumentType::Form,
            "receipt" => DocumentType::Receipt,
            "statement" => DocumentType::Statement,
            "proposal" => DocumentType::Proposal,
            "presentation" => DocumentType::Presentation,
            "spreadsheet" => DocumentType::Spreadsheet,
            "manual" => DocumentType::Manual,
            "certificate" => DocumentType::Certificate,
            "resume" => DocumentType::Resume,
            "photo" => DocumentType::Photo,
            _ => DocumentType::Unknown,
        };

        Ok(DocumentAnalysis {
            file_path: path.to_string_lossy().to_string(),
            file_name: filename.to_string(),
            content_summary: parsed.content_summary,
            document_type,
            key_entities: parsed.key_entities,
            confidence: 0.85,
            suggested_name: parsed.suggested_name,
            method: AnalysisMethod::TextExtraction,
        })
    }

    /// Analyze using Vision API (static version)
    async fn analyze_with_vision_static(
        path: &Path,
        filename: &str,
        ext: Option<&str>,
        client: &Arc<GrokClient>,
        cache: &Arc<ContentCache>,
        pdf_renderer: &Arc<PdfRenderer>,
        batch_id: usize,
    ) -> Result<(DocumentAnalysis, u32), String> {
        // Get image data
        let image_data = if ext == Some("pdf") {
            // Render first page of PDF
            pdf_renderer.render_first_page(path).await?
        } else {
            // Read image file directly
            std::fs::read(path).map_err(|e| format!("Failed to read image: {}", e))?
        };

        // Use GrokClient's analyze_document_image method
        let mut analysis = client
            .analyze_document_image(&image_data, filename, None)
            .await?;

        // Update file path (client doesn't know the path)
        analysis.file_path = path.to_string_lossy().to_string();
        analysis.method = AnalysisMethod::GrokVision;

        // Vision API tokens estimate
        let estimated_tokens = 1500u32;

        // Cache the result
        if let Err(e) = cache.store(path, &analysis, estimated_tokens) {
            tracing::warn!(
                "[ExploreAgent {}] Failed to cache analysis: {}",
                batch_id,
                e
            );
        }

        Ok((analysis, estimated_tokens))
    }

    /// Analyze a single file
    /// Strategy: Text extraction first, Vision API fallback for scanned/images
    #[allow(dead_code)]
    async fn analyze_file(
        &self,
        path: &Path,
    ) -> Result<(DocumentAnalysis, u32), String> {
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let ext = path.extension().and_then(|e| e.to_str());

        tracing::info!(
            "[ExploreAgent {}] Analyzing file: {} (ext: {:?})",
            self.batch_id,
            filename,
            ext
        );

        // 1. Check cache first
        if let Ok(Some(mut cached)) = self.cache.get_cached(path) {
            cached.file_path = path.to_string_lossy().to_string();
            cached.method = AnalysisMethod::Cached;
            tracing::info!(
                "[ExploreAgent {}] Cache hit for {} - summary: {} chars, entities: {:?}",
                self.batch_id,
                filename,
                cached.content_summary.len(),
                cached.key_entities
            );
            return Ok((cached, 0)); // No tokens used for cache hit
        }

        // 2. Try text extraction for documents (PDF, Office, etc.)
        if super::document_parser::is_parseable(ext) {
            match self.analyze_with_text_extraction(path, &filename).await {
                Ok((analysis, tokens)) => {
                    tracing::info!(
                        "[ExploreAgent {}] TEXT EXTRACTION SUCCESS for {}: summary={} chars, entities={:?}, type={}",
                        self.batch_id,
                        filename,
                        analysis.content_summary.len(),
                        analysis.key_entities,
                        analysis.document_type.as_str()
                    );
                    return Ok((analysis, tokens));
                }
                Err(e) => {
                    tracing::warn!(
                        "[ExploreAgent {}] Text extraction FAILED for {}: {} - will try Vision API",
                        self.batch_id,
                        filename,
                        e
                    );
                    // Fall through to Vision API
                }
            }
        }

        // 3. Fall back to Vision API for scanned docs and images
        if vision::is_image_extension(ext) || ext.map(|e| e.to_lowercase()) == Some("pdf".to_string()) {
            tracing::info!(
                "[ExploreAgent {}] Using Vision API fallback for {}",
                self.batch_id,
                filename
            );
            let result = self.analyze_with_vision(path, &filename, ext).await;
            match &result {
                Ok((analysis, _)) => {
                    tracing::info!(
                        "[ExploreAgent {}] VISION API SUCCESS for {}: summary={} chars, entities={:?}",
                        self.batch_id,
                        filename,
                        analysis.content_summary.len(),
                        analysis.key_entities
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "[ExploreAgent {}] VISION API FAILED for {}: {}",
                        self.batch_id,
                        filename,
                        e
                    );
                }
            }
            return result;
        }

        Err(format!("Unsupported file type: {:?}", ext))
    }

    /// Analyze document using text extraction + Grok text analysis
    async fn analyze_with_text_extraction(
        &self,
        path: &Path,
        filename: &str,
    ) -> Result<(DocumentAnalysis, u32), String> {
        // Parse document in blocking task
        let path_clone = path.to_path_buf();
        let parsed: ParsedDocument = tokio::task::spawn_blocking(move || {
            let parser = DocumentParser::new();
            parser.parse(&path_clone)
        })
        .await
        .map_err(|e| format!("Task failed: {}", e))??;

        // Check if we got meaningful content
        if parsed.text.len() < 100 {
            return Err("Extracted text too short, likely scanned document".to_string());
        }

        // Get a content preview for AI analysis (up to 10K chars for rich context)
        let content_preview = self.document_parser.get_analysis_preview(&parsed, 10_000);

        // Send extracted text to Grok for intelligent analysis
        let analysis = self
            .analyze_text_content_with_grok(path, filename, &content_preview, &parsed)
            .await?;

        // Estimate tokens used (text is cheaper than vision)
        let tokens_used = (content_preview.len() / 4) as u32 + 500; // rough estimate

        // Cache the result
        let _ = self.cache.store(path, &analysis, tokens_used);

        Ok((analysis, tokens_used))
    }

    /// Send extracted text to Grok for intelligent analysis
    async fn analyze_text_content_with_grok(
        &self,
        path: &Path,
        filename: &str,
        content: &str,
        parsed: &ParsedDocument,
    ) -> Result<DocumentAnalysis, String> {
        use reqwest::Client;
        use serde_json::json;

        let prompt = format!(
            r#"Analyze this document and extract SPECIFIC information for intelligent file organization.

FILENAME: {}

DOCUMENT CONTENT:
{}

CRITICAL INSTRUCTIONS:
1. Extract SPECIFIC names - not generic descriptions
2. Company names like "Acme Corporation", "Smith & Associates"
3. Person names like "John Smith", "Dr. Sarah Chen"
4. Project/transaction names like "Project Phoenix", "Invoice #INV-2024-0542"
5. Specific dates like "January 15, 2024" or "Q1 2024"
6. Dollar amounts like "$15,432.00" or "€5,000"

Respond with ONLY this JSON format:
{{
  "content_summary": "4-5 detailed sentences covering: WHO (specific company/person names), WHAT (specific document purpose, project, or transaction), WHEN (dates), and HOW MUCH (amounts). Include ALL specific identifiers found.",
  "document_type": "one of: invoice, contract, report, letter, form, receipt, statement, proposal, presentation, spreadsheet, manual, certificate, resume, photo, unknown",
  "key_entities": ["Acme-Corp", "John-Smith", "Project-Phoenix", "2024-01-15", "$15432"],
  "suggested_name": "Follow naming rules below"
}}

## FILE NAMING RULES (for suggested_name):

FORMAT: [Entity]-[Description]-[Date]-[Type]
- Use HYPHENS (-) instead of spaces, NEVER use spaces or underscores
- Include date as YYYY-MM or YYYY-MM-DD when document has important date
- Start with the primary entity (company, person, property)
- End with document type abbreviation
- Keep it descriptive but concise (max 60 chars before extension)
- NO file extension in suggested_name

GOOD EXAMPLES:
✅ "Acme-Corp-Invoice-2024-03-15-5432"
✅ "Smith-John-Employment-Contract-2024-01"
✅ "123-Main-St-Lease-Agreement-2024"
✅ "Q1-2024-Financial-Report"
✅ "Project-Phoenix-Proposal-Draft"
✅ "TechStart-Inc-NDA-Signed-2024-02"
✅ "ABC-Properties-Rent-Roll-2024-Q1"

BAD EXAMPLES:
❌ "scan001" (meaningless)
❌ "Document 1" (spaces, generic)
❌ "invoice" (no context)
❌ "report_final" (underscores, generic)

The key_entities array is CRITICAL for folder organization. Include every specific name, date, and amount you find!"#,
            filename, content
        );

        let api_key = std::env::var("XAI_API_KEY")
            .or_else(|_| std::env::var("GROK_API_KEY"))
            .or_else(|_| std::env::var("VITE_XAI_API_KEY"))
            .map_err(|_| "No Grok API key found")?;

        let client = Client::new();
        let response = client
            .post("https://api.x.ai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&json!({
                "model": "grok-4-1-fast",
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 1000,
                "temperature": 0.1
            }))
            .send()
            .await
            .map_err(|e| format!("API request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Grok API error ({}): {}", status, text));
        }

        #[derive(serde::Deserialize)]
        struct GrokResponse {
            choices: Vec<Choice>,
        }
        #[derive(serde::Deserialize)]
        struct Choice {
            message: Message,
        }
        #[derive(serde::Deserialize)]
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

        // Parse the JSON response
        #[derive(serde::Deserialize)]
        struct AnalysisResponse {
            content_summary: String,
            document_type: String,
            key_entities: Vec<String>,
            suggested_name: Option<String>,
        }

        // Extract JSON from response (handle markdown code blocks)
        let json_str = if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                &content[start..=end]
            } else {
                &content
            }
        } else {
            &content
        };

        let analysis_resp: AnalysisResponse = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse analysis JSON: {}. Content: {}", e, content))?;

        Ok(DocumentAnalysis {
            file_path: path.to_string_lossy().to_string(),
            file_name: filename.to_string(),
            content_summary: analysis_resp.content_summary,
            document_type: DocumentType::from_str(&analysis_resp.document_type),
            key_entities: analysis_resp.key_entities,
            suggested_name: analysis_resp.suggested_name,
            confidence: if parsed.method == ExtractionMethod::NativeText {
                0.9
            } else {
                0.7
            },
            method: AnalysisMethod::TextExtraction,
        })
    }

    /// Analyze using Grok Vision API (for images and scanned PDFs)
    async fn analyze_with_vision(
        &self,
        path: &Path,
        filename: &str,
        ext: Option<&str>,
    ) -> Result<(DocumentAnalysis, u32), String> {
        let image_data = if ext.map(|e| e.to_lowercase()) == Some("pdf".to_string()) {
            // PDF: render first page (scanned doc)
            self.pdf_renderer.render_first_page(path).await?
        } else {
            // Image: load and prepare
            vision::load_image_for_vision(path).await?
        };

        // Estimate tokens
        let estimated_tokens = vision::estimate_image_tokens(image_data.len(), "low") + 200;

        // Call Grok Vision
        let mut analysis = self
            .client
            .analyze_document_image(&image_data, filename, None)
            .await?;

        analysis.file_path = path.to_string_lossy().to_string();

        // Store in cache
        let _ = self.cache.store(path, &analysis, estimated_tokens);

        Ok((analysis, estimated_tokens))
    }
}

/// Run multiple explore agents in parallel
pub async fn run_parallel_explores<F>(
    client: Arc<GrokClient>,
    cache: Arc<ContentCache>,
    pdf_renderer: Arc<PdfRenderer>,
    batches: Vec<ExploreBatch>,
    progress_callback: F,
) -> Vec<ExploreResult>
where
    F: Fn(AnalysisProgress) + Send + Sync + Clone + 'static,
{
    use futures::stream::{self, StreamExt};

    let results: Vec<ExploreResult> = stream::iter(batches)
        .map(|batch| {
            let client = Arc::clone(&client);
            let cache = Arc::clone(&cache);
            let pdf_renderer = Arc::clone(&pdf_renderer);
            let callback = progress_callback.clone();

            async move {
                let agent = ExploreAgent::new(client, cache, pdf_renderer, batch.batch_id);
                agent.process_batch(batch.files, callback).await
            }
        })
        .buffer_unordered(12) // Max 12 concurrent agents for faster processing
        .collect()
        .await;

    results
}

/// Split files into batches for parallel processing
pub fn create_batches(files: Vec<std::path::PathBuf>, batch_size: usize) -> Vec<ExploreBatch> {
    files
        .chunks(batch_size)
        .enumerate()
        .map(|(i, chunk)| ExploreBatch {
            batch_id: i,
            files: chunk.to_vec(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_batches() {
        let files: Vec<_> = (0..25)
            .map(|i| std::path::PathBuf::from(format!("file{}.pdf", i)))
            .collect();
        let batches = create_batches(files, 10);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].files.len(), 10);
        assert_eq!(batches[2].files.len(), 5);
    }
}
