//! Integration Module
//!
//! High-level API for the multi-model file analysis pipeline.
//!
//! ## Architecture
//! 1. Scan ALL files (PDFs, images, Office docs, text)
//! 2. Extract text from documents (pure Rust)
//! 3. OpenAI GPT-5-nano workers analyze in parallel (5 files/batch, 2-20 workers)
//! 4. Grok grok-4-1-fast summarizes outputs (temp=0.1)
//! 5. Grok orchestrator creates folder structure + assignments

use super::cache::ContentCache;
use super::client::GrokClient;
use super::document_parser::{is_parseable, DocumentParser};
use super::explore_agent::{create_batches, run_parallel_explores, ExploreAgent};
use super::openai_worker::{
    calculate_worker_count, create_file_batches, run_parallel_workers, FileContent,
};
use super::orchestrator::{OrchestratorAgent, OrchestratorConfig};
use super::pdf_renderer::PdfRenderer;
use super::summarizer::GrokSummarizer;
use super::types::*;
use super::vision;
use futures::stream::{self, StreamExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use walkdir::WalkDir;

/// Main entry point for multi-model file organization
pub struct GrokOrganizer {
    client: Arc<GrokClient>,
    cache: Arc<ContentCache>,
    pdf_renderer: Arc<PdfRenderer>,
    config: GrokConfig,
    openai_api_key: Option<String>,
    grok_api_key: String,
}

impl GrokOrganizer {
    /// Create a new organizer
    pub fn new(api_key: String, cache_dir: &Path) -> Result<Self, String> {
        use crate::ai::credentials::CredentialManager;

        let config = GrokConfig {
            api_key: api_key.clone(),
            ..Default::default()
        };

        // Try to get OpenAI API key from credential manager first, then environment
        let openai_api_key = CredentialManager::get_api_key("openai")
            .ok()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .or_else(|| std::env::var("VITE_OPENAI_API_KEY").ok());

        if openai_api_key.is_some() {
            tracing::info!("[GrokOrganizer] OpenAI API key found - using multi-model pipeline");
        } else {
            tracing::info!("[GrokOrganizer] No OpenAI API key - using Grok-only pipeline");
        }

        let client = Arc::new(GrokClient::new(config.clone())?);
        let cache = Arc::new(ContentCache::open(cache_dir)?);
        let pdf_renderer = Arc::new(PdfRenderer::new());

        Ok(Self {
            client,
            cache,
            pdf_renderer,
            config,
            openai_api_key,
            grok_api_key: api_key,
        })
    }

    /// Scan a folder and identify files that can be analyzed
    pub async fn scan_folder(&self, folder: &Path) -> Result<ScanResult, String> {
        let mut analyzable_files = Vec::new();
        let mut text_files = Vec::new();
        let mut other_files = Vec::new();
        let mut total_size = 0u64;

        for entry in WalkDir::new(folder)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path().to_path_buf();
            let ext = path.extension().and_then(|e| e.to_str());
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            total_size += size;

            if vision::is_analyzable_extension(ext) {
                analyzable_files.push(path);
            } else if vision::is_text_extension(ext) {
                text_files.push(path);
            } else {
                other_files.push(path);
            }
        }

        // Check cache for already-analyzed files
        let cached_count = self
            .cache
            .filter_uncached(&analyzable_files)
            .map(|uncached| analyzable_files.len() - uncached.len())
            .unwrap_or(0);

        let needs_analysis = analyzable_files.len() - cached_count;

        // Estimate cost ($0.20/M input + $0.50/M output, ~1000 tokens per doc)
        let estimated_cost_cents = (needs_analysis as f64 * 0.035) as u32; // ~$0.035 per doc

        Ok(ScanResult {
            total_files: analyzable_files.len() + text_files.len() + other_files.len(),
            analyzable_files: analyzable_files.len(),
            text_files: text_files.len(),
            other_files: other_files.len(),
            cached_files: cached_count,
            needs_analysis,
            total_size_bytes: total_size,
            estimated_cost_cents,
            file_paths: analyzable_files,
        })
    }

    /// Run the full organization pipeline
    /// Uses OpenAI workers if OPENAI_API_KEY is available, otherwise falls back to Grok-only
    pub async fn organize<F>(
        &self,
        folder: &Path,
        user_instruction: &str,
        progress_callback: F,
    ) -> Result<OrganizationPlan, String>
    where
        F: Fn(AnalysisProgress) + Send + Sync + Clone + 'static,
    {
        // Choose pipeline based on available API keys
        if let Some(ref openai_key) = self.openai_api_key {
            self.organize_multi_model(folder, user_instruction, openai_key.clone(), progress_callback)
                .await
        } else {
            self.organize_grok_only(folder, user_instruction, progress_callback)
                .await
        }
    }

    /// Multi-model pipeline: OpenAI workers → Grok summarizer → Grok orchestrator
    async fn organize_multi_model<F>(
        &self,
        folder: &Path,
        user_instruction: &str,
        openai_key: String,
        progress_callback: F,
    ) -> Result<OrganizationPlan, String>
    where
        F: Fn(AnalysisProgress) + Send + Sync + Clone + 'static,
    {
        // 1. Scan folder for ALL files
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::Scanning,
            current: 0,
            total: 0,
            current_file: None,
            message: "Scanning folder...".to_string(),
        });

        let scan = self.scan_folder(folder).await?;
        let total_files = scan.total_files;

        tracing::info!(
            "[GrokOrganizer] Multi-model scan: {} total files ({} analyzable, {} text, {} other)",
            total_files,
            scan.analyzable_files,
            scan.text_files,
            scan.other_files
        );

        // Emit scan results with file counts
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::Scanning,
            current: total_files,
            total: total_files,
            current_file: None,
            message: format!(
                "Found {} files ({} documents, {} text, {} other)",
                total_files,
                scan.analyzable_files,
                scan.text_files,
                scan.other_files
            ),
        });

        // 2. Collect ALL file paths for analysis
        let mut all_file_paths: Vec<PathBuf> = scan.file_paths.clone();

        // Add text files from the folder
        for entry in WalkDir::new(folder)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path().to_path_buf();
            let ext = path.extension().and_then(|e| e.to_str());

            // Add text files that weren't in analyzable
            if vision::is_text_extension(ext) && !all_file_paths.contains(&path) {
                all_file_paths.push(path);
            }
        }

        tracing::info!(
            "[GrokOrganizer] Total files to analyze: {}",
            all_file_paths.len()
        );

        // 3. Extract text from all parseable files
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::CheckingCache,
            current: 0,
            total: all_file_paths.len(),
            current_file: None,
            message: "Extracting text from documents...".to_string(),
        });

        // Parallel text extraction with semaphore to limit concurrency
        // This is ~4-8x faster than sequential extraction for large folders
        let extraction_semaphore = Arc::new(Semaphore::new(8)); // 8 concurrent extractions
        let progress_counter = Arc::new(AtomicUsize::new(0));
        let total_files = all_file_paths.len();
        let cache = Arc::clone(&self.cache);

        // Define extraction result type
        #[derive(Debug)]
        enum ExtractionResult {
            Cached,
            Extracted(FileContent),
            NeedsVision(PathBuf),
            Skip,
        }

        // Process all files in parallel
        let extraction_tasks: Vec<_> = all_file_paths
            .iter()
            .map(|path| {
                let path = path.clone();
                let semaphore = Arc::clone(&extraction_semaphore);
                let cache = Arc::clone(&cache);
                let counter = Arc::clone(&progress_counter);

                async move {
                    // Acquire semaphore permit
                    let _permit = semaphore.acquire().await.ok()?;

                    let ext = path.extension().and_then(|e| e.to_str());
                    let filename = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    // Update progress counter
                    counter.fetch_add(1, Ordering::Relaxed);

                    // Check cache first
                    if let Ok(Some(_)) = cache.get_cached(&path) {
                        tracing::debug!("[GrokOrganizer] Cache hit: {}", filename);
                        return Some(ExtractionResult::Cached);
                    }

                    // Try text extraction for parseable files
                    if is_parseable(ext) {
                        // Run parser in blocking thread pool
                        let path_clone = path.clone();
                        let ext_str = ext.map(|s| s.to_string());
                        let parsed_result = tokio::task::spawn_blocking(move || {
                            let parser = DocumentParser::new();
                            parser.parse(&path_clone)
                        })
                        .await
                        .ok()?;

                        match parsed_result {
                            Ok(parsed) if parsed.text.len() >= 100 => {
                                return Some(ExtractionResult::Extracted(FileContent {
                                    path: path.clone(),
                                    filename,
                                    content: parsed.text.chars().take(2000).collect(),
                                    extension: ext_str.unwrap_or_default(),
                                }));
                            }
                            _ => {
                                // Text extraction failed, try Vision API
                                if vision::is_analyzable_extension(ext) {
                                    return Some(ExtractionResult::NeedsVision(path));
                                }
                            }
                        }
                    } else if vision::is_analyzable_extension(ext) {
                        // Image or scanned PDF - use Vision API
                        return Some(ExtractionResult::NeedsVision(path));
                    }

                    Some(ExtractionResult::Skip)
                }
            })
            .collect();

        // Execute all extractions concurrently with progress updates
        let mut file_contents: Vec<FileContent> = Vec::new();
        let mut vision_files: Vec<PathBuf> = Vec::new();

        // Use buffered stream to process results as they complete
        // Add timeout per task to prevent hanging
        let extraction_tasks_with_timeout: Vec<_> = extraction_tasks
            .into_iter()
            .map(|task| async move {
                tokio::time::timeout(std::time::Duration::from_secs(30), task)
                    .await
                    .ok()
                    .flatten()
            })
            .collect();

        let mut extraction_stream = stream::iter(extraction_tasks_with_timeout)
            .buffer_unordered(16); // Allow up to 16 pending futures

        let mut last_progress = 0;
        while let Some(result) = extraction_stream.next().await {
            // Emit progress every 5 files
            let current = progress_counter.load(Ordering::Relaxed);
            if current >= last_progress + 5 || current == total_files {
                progress_callback(AnalysisProgress {
                    phase: AnalysisPhase::CheckingCache,
                    current,
                    total: total_files,
                    current_file: None,
                    message: format!("Extracting text: {}/{}", current, total_files),
                });
                last_progress = current;
            }

            match result {
                Some(ExtractionResult::Extracted(content)) => {
                    file_contents.push(content);
                }
                Some(ExtractionResult::NeedsVision(path)) => {
                    vision_files.push(path);
                }
                Some(ExtractionResult::Cached) | Some(ExtractionResult::Skip) | None => {}
            }
        }

        // Emit progress transition - extraction complete, moving to next phase
        tracing::info!("[GrokOrganizer] Extraction phase complete, transitioning to analysis");
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::CheckingCache,
            current: total_files,
            total: total_files,
            current_file: None,
            message: "Text extraction complete, preparing analysis...".to_string(),
        });

        // Count cache hits for files we skipped
        let cache_hits = all_file_paths.len() - file_contents.len() - vision_files.len();

        tracing::info!(
            "[GrokOrganizer] Extracted text from {} files, {} need Vision API, {} cached",
            file_contents.len(),
            vision_files.len(),
            cache_hits
        );

        // Emit cache statistics
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::CheckingCache,
            current: cache_hits,
            total: all_file_paths.len(),
            current_file: None,
            message: format!(
                "Cache: {} hits, {} need analysis",
                cache_hits,
                file_contents.len() + vision_files.len()
            ),
        });

        // 4. Run OpenAI workers on extracted text (parallel)
        let mut all_analyses: Vec<DocumentAnalysis> = Vec::new();

        if !file_contents.is_empty() {
            let worker_count = calculate_worker_count(file_contents.len());
            let batch_size = 10; // 10 files per worker call for faster processing

            let batch_count = file_contents.len().div_ceil(batch_size);
            progress_callback(AnalysisProgress {
                phase: AnalysisPhase::AnalyzingContent,
                current: 0,
                total: file_contents.len(),
                current_file: None,
                message: format!(
                    "OpenAI Workers: Analyzing {} files ({} workers, {} batches)",
                    file_contents.len(),
                    worker_count,
                    batch_count
                ),
            });

            tracing::info!(
                "[GrokOrganizer] Starting {} OpenAI workers for {} files (batch size: {})",
                worker_count,
                file_contents.len(),
                batch_size
            );

            let batches = create_file_batches(file_contents, batch_size);
            let worker_results = run_parallel_workers(openai_key, batches, worker_count).await;

            // Collect successful results
            let mut file_analyses = Vec::new();
            for result in worker_results {
                match result {
                    Ok(analyses) => {
                        file_analyses.extend(analyses);
                    }
                    Err(e) => {
                        tracing::warn!("[GrokOrganizer] Worker batch failed: {}", e);
                    }
                }
            }

            tracing::info!(
                "[GrokOrganizer] OpenAI workers completed: {} files analyzed",
                file_analyses.len()
            );

            // 5. Use Grok summarizer to format outputs (temp=0.1)
            progress_callback(AnalysisProgress {
                phase: AnalysisPhase::Aggregating,
                current: 0,
                total: file_analyses.len(),
                current_file: None,
                message: format!(
                    "Grok Summarizer: Formatting {} analyses",
                    file_analyses.len()
                ),
            });

            let summarizer = GrokSummarizer::new(self.grok_api_key.clone());
            let formatted = summarizer.format_for_orchestrator(file_analyses).await?;

            // Cache the formatted analyses
            for analysis in &formatted {
                let path = PathBuf::from(&analysis.file_path);
                let _ = self.cache.store(&path, analysis, 0);
            }

            all_analyses.extend(formatted);
        } else {
            // No text content to analyze - emit progress to show we're moving on
            progress_callback(AnalysisProgress {
                phase: AnalysisPhase::AnalyzingContent,
                current: 0,
                total: 0,
                current_file: None,
                message: "No new text content to analyze (using cache)".to_string(),
            });
            tracing::info!("[GrokOrganizer] No text content to analyze (all cached or vision-only)");
        }

        // 6. Process Vision files with existing Grok pipeline
        if !vision_files.is_empty() {
            progress_callback(AnalysisProgress {
                phase: AnalysisPhase::AnalyzingContent,
                current: 0,
                total: vision_files.len(),
                current_file: None,
                message: format!("Analyzing {} image files with Vision API...", vision_files.len()),
            });

            let batches = create_batches(vision_files.clone(), self.config.batch_size);
            let explore_results = run_parallel_explores(
                Arc::clone(&self.client),
                Arc::clone(&self.cache),
                Arc::clone(&self.pdf_renderer),
                batches,
                progress_callback.clone(),
            )
            .await;

            // Check for auth errors across all batches
            let mut total_failed = 0;
            let mut auth_error_detected = false;

            for result in &explore_results {
                total_failed += result.failed_files.len();
                // Check if any failures are auth-related
                for (_, error) in &result.failed_files {
                    if error.contains("Invalid or missing xAI API key")
                        || error.contains("Incorrect API key")
                        || error.contains("API key")
                    {
                        auth_error_detected = true;
                        break;
                    }
                }
            }

            // Emit warning if auth errors detected
            if auth_error_detected && total_failed > 0 {
                progress_callback(AnalysisProgress {
                    phase: AnalysisPhase::AnalyzingContent,
                    current: 0,
                    total: vision_files.len(),
                    current_file: None,
                    message: format!(
                        "⚠️ Vision API failed: Invalid xAI API key. {} files skipped. Check Settings > AI Keys.",
                        total_failed
                    ),
                });
                tracing::error!(
                    "[GrokOrganizer] Vision analysis failed: Invalid xAI API key. {} files could not be analyzed.",
                    total_failed
                );
            } else if total_failed > 0 {
                tracing::warn!(
                    "[GrokOrganizer] Vision analysis: {} files failed (non-auth errors)",
                    total_failed
                );
            }

            for result in explore_results {
                all_analyses.extend(result.analyses);
            }
        }

        // 7. Load any cached analyses we skipped
        for path in &scan.file_paths {
            if let Ok(Some(cached)) = self.cache.get_cached(path) {
                // Check if we already have this file
                if !all_analyses.iter().any(|a| a.file_path == cached.file_path) {
                    all_analyses.push(cached);
                }
            }
        }

        tracing::info!(
            "[GrokOrganizer] Total analyses ready for orchestrator: {}",
            all_analyses.len()
        );

        // 8. Run orchestrator to create plan
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::Planning,
            current: 0,
            total: all_analyses.len(),
            current_file: None,
            message: format!(
                "Grok Orchestrator: Planning {} file assignments",
                all_analyses.len()
            ),
        });

        let orchestrator_config = OrchestratorConfig {
            user_instruction: user_instruction.to_string(),
            ..Default::default()
        };

        let orchestrator = OrchestratorAgent::new(Arc::clone(&self.client), orchestrator_config);

        let explore_result = ExploreResult {
            batch_id: 0,
            analyses: all_analyses,
            failed_files: vec![],
            total_tokens_used: 0,
            duration_ms: 0,
        };

        let plan = orchestrator.create_plan(vec![explore_result]).await?;

        // 9. Complete
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::Complete,
            current: plan.assignments.len(),
            total: plan.assignments.len(),
            current_file: None,
            message: format!(
                "Plan ready: {} folders, {} file assignments",
                plan.folder_structure.len(),
                plan.assignments.len()
            ),
        });

        Ok(plan)
    }

    /// Original Grok-only pipeline (fallback when no OpenAI key)
    async fn organize_grok_only<F>(
        &self,
        folder: &Path,
        user_instruction: &str,
        progress_callback: F,
    ) -> Result<OrganizationPlan, String>
    where
        F: Fn(AnalysisProgress) + Send + Sync + Clone + 'static,
    {
        // 1. Scan folder
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::Scanning,
            current: 0,
            total: 0,
            current_file: None,
            message: "Scanning folder...".to_string(),
        });

        let scan = self.scan_folder(folder).await?;

        tracing::info!(
            "[GrokOrganizer] Grok-only scan: {} analyzable, {} cached, {} need analysis",
            scan.analyzable_files,
            scan.cached_files,
            scan.needs_analysis
        );

        // 2. Check cache
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::CheckingCache,
            current: scan.cached_files,
            total: scan.analyzable_files,
            current_file: None,
            message: format!("{} files already analyzed", scan.cached_files),
        });

        // 3. Filter to uncached files
        let uncached_files = self.cache.filter_uncached(&scan.file_paths)?;

        // 4. Create batches and run explore agents in parallel
        if !uncached_files.is_empty() {
            progress_callback(AnalysisProgress {
                phase: AnalysisPhase::AnalyzingContent,
                current: 0,
                total: uncached_files.len(),
                current_file: None,
                message: format!("Analyzing {} files...", uncached_files.len()),
            });

            let batches = create_batches(uncached_files, self.config.batch_size);

            let explore_results = run_parallel_explores(
                Arc::clone(&self.client),
                Arc::clone(&self.cache),
                Arc::clone(&self.pdf_renderer),
                batches,
                progress_callback.clone(),
            )
            .await;

            // Log results and check for auth errors
            let total_analyzed: usize = explore_results.iter().map(|r| r.analyses.len()).sum();
            let total_failed: usize = explore_results.iter().map(|r| r.failed_files.len()).sum();
            let total_tokens: u32 = explore_results.iter().map(|r| r.total_tokens_used).sum();

            // Check for auth errors
            let mut auth_error_detected = false;
            for result in &explore_results {
                for (_, error) in &result.failed_files {
                    if error.contains("Invalid or missing xAI API key")
                        || error.contains("Incorrect API key")
                        || error.contains("API key")
                    {
                        auth_error_detected = true;
                        break;
                    }
                }
                if auth_error_detected {
                    break;
                }
            }

            // Emit warning if auth errors detected
            if auth_error_detected && total_failed > 0 {
                progress_callback(AnalysisProgress {
                    phase: AnalysisPhase::AnalyzingContent,
                    current: 0,
                    total: total_failed,
                    current_file: None,
                    message: format!(
                        "⚠️ Grok Vision API failed: Invalid xAI API key. {} files skipped. Check Settings > AI Keys.",
                        total_failed
                    ),
                });
                tracing::error!(
                    "[GrokOrganizer] Grok-only analysis failed: Invalid xAI API key. {} files could not be analyzed.",
                    total_failed
                );
            }

            tracing::info!(
                "[GrokOrganizer] Explore complete: {} analyzed, {} failed, {} tokens",
                total_analyzed,
                total_failed,
                total_tokens
            );
        }

        // 5. Gather all analyses (from cache and new)
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::Aggregating,
            current: 0,
            total: scan.analyzable_files,
            current_file: None,
            message: "Gathering analyses...".to_string(),
        });

        let mut all_analyses = Vec::new();
        for path in &scan.file_paths {
            if let Ok(Some(analysis)) = self.cache.get_cached(path) {
                all_analyses.push(analysis);
            }
        }

        // Also analyze text files
        for entry in WalkDir::new(folder)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str());

            if vision::is_text_extension(ext) {
                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    let filename = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();

                    all_analyses.push(DocumentAnalysis {
                        file_path: path.to_string_lossy().to_string(),
                        file_name: filename.clone(),
                        content_summary: content.chars().take(500).collect(),
                        document_type: DocumentType::Unknown,
                        key_entities: vec![],
                        suggested_name: None,
                        confidence: 0.5,
                        method: AnalysisMethod::TextExtraction,
                    });
                }
            }
        }

        // 6. Run orchestrator to create plan
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::Planning,
            current: 0,
            total: 1,
            current_file: None,
            message: "Creating organization plan...".to_string(),
        });

        let orchestrator_config = OrchestratorConfig {
            user_instruction: user_instruction.to_string(),
            ..Default::default()
        };

        let orchestrator = OrchestratorAgent::new(Arc::clone(&self.client), orchestrator_config);

        let explore_result = ExploreResult {
            batch_id: 0,
            analyses: all_analyses,
            failed_files: vec![],
            total_tokens_used: 0,
            duration_ms: 0,
        };

        let plan = orchestrator.create_plan(vec![explore_result]).await?;

        // 7. Complete
        progress_callback(AnalysisProgress {
            phase: AnalysisPhase::Complete,
            current: plan.assignments.len(),
            total: plan.assignments.len(),
            current_file: None,
            message: format!(
                "Plan ready: {} folders, {} file assignments",
                plan.folder_structure.len(),
                plan.assignments.len()
            ),
        });

        Ok(plan)
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> Result<super::cache::CacheStats, String> {
        self.cache.get_stats()
    }

    /// Clear the cache
    pub fn clear_cache(&self) -> Result<(), String> {
        self.cache.clear()
    }

    /// Analyze a single file
    pub async fn analyze_single(&self, path: &Path) -> Result<DocumentAnalysis, String> {
        // Check cache first
        if let Some(cached) = self.cache.get_cached(path)? {
            return Ok(cached);
        }

        let agent = ExploreAgent::new(
            Arc::clone(&self.client),
            Arc::clone(&self.cache),
            Arc::clone(&self.pdf_renderer),
            0,
        );

        let result = agent
            .process_batch(vec![path.to_path_buf()], |_| {})
            .await;

        result
            .analyses
            .into_iter()
            .next()
            .ok_or_else(|| "Failed to analyze file".to_string())
    }
}

/// Result of scanning a folder
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub total_files: usize,
    pub analyzable_files: usize,
    pub text_files: usize,
    pub other_files: usize,
    pub cached_files: usize,
    pub needs_analysis: usize,
    pub total_size_bytes: u64,
    pub estimated_cost_cents: u32,
    #[serde(skip)]
    pub file_paths: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_scan_folder() {
        let dir = tempdir().unwrap();

        // Create test files
        std::fs::write(dir.path().join("test.pdf"), "fake pdf").unwrap();
        std::fs::write(dir.path().join("doc.txt"), "text content").unwrap();
        std::fs::write(dir.path().join("image.jpg"), "fake image").unwrap();

        // Note: This test requires a valid API key to fully work
        // For unit testing, we just verify the scan logic
    }
}
