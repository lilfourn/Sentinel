//! Architect Module - High-level planning for deep organization.
//!
//! The Architect receives user instructions and a semantic sample of files,
//! then outputs a Blueprint defining the target folder structure and
//! extraction rules for the Builder phase.
//!
//! ## Algorithm
//!
//! 1. Generate stratified sample from VFS (max 60 diverse files)
//! 2. Read file headers (first 1KB) for text files
//! 3. Build prompt with: user instruction + folder stats + file samples
//! 4. Call Sonnet for planning (critical reasoning)
//! 5. Parse JSON response into Blueprint
//!
//! The Blueprint is then used by the Builder to slot files efficiently.

use crate::ai::client::ClaudeModel;
use crate::ai::credentials::CredentialManager;
use crate::ai::rules::VirtualFile;

use super::agent_loop::ExpandableDetail;
use super::rate_limiter::RateLimitManager;
use super::sampling;
use super::vfs::ShadowVFS;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Maximum file header size to read (1KB)
const MAX_HEADER_SIZE: usize = 1024;

/// Maximum retries for rate limit errors
const MAX_RETRIES: u32 = 3;

/// Blueprint output from the Architect phase.
/// Defines the target organization structure and rules for the Builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blueprint {
    /// Human-readable name for this organization strategy
    pub strategy_name: String,

    /// Target folder structure with semantic descriptions
    pub structure: Vec<BlueprintFolder>,

    /// DSL rules for extracting/categorizing files
    pub extraction_rules: String,

    /// Optional description of the overall strategy
    #[serde(default)]
    pub description: Option<String>,

    /// Confidence score from the Architect (0.0-1.0)
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_confidence() -> f32 {
    0.8
}

/// A folder in the target structure with semantic context
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintFolder {
    /// Relative path from root (e.g., "Documents/Invoices")
    pub path: String,

    /// Semantic description for vector matching (e.g., "tax invoices, billing statements")
    pub semantic_description: String,

    /// Expected file extensions (hint for matching)
    #[serde(default)]
    pub expected_extensions: Vec<String>,

    /// Pre-computed embedding for fast vector matching (populated by embed_blueprint)
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
}

/// A sampled file with header content for Architect context
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSample {
    pub name: String,
    pub extension: Option<String>,
    pub size: u64,
    pub size_formatted: String,
    pub modified_at: Option<String>,
    pub header_preview: Option<String>,
}

/// Folder statistics for Architect context
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderStats {
    pub total_files: usize,
    pub total_size_mb: f64,
    pub extension_breakdown: Vec<(String, usize)>,
    pub date_range: Option<(String, String)>,
}

/// API request structure
#[derive(Serialize)]
struct ArchitectApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

/// API response structure
#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
    #[allow(dead_code)]
    stop_reason: String,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

/// API error structure
#[derive(Deserialize)]
struct ApiError {
    error: ApiErrorDetail,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
}

/// Run the Architect phase to generate a Blueprint.
///
/// # Arguments
/// * `target_folder` - Path to the folder being organized
/// * `user_instruction` - User's organization request
/// * `vfs` - ShadowVFS for file access
/// * `event_emitter` - Callback for UI progress events
///
/// # Returns
/// A Blueprint defining the target structure and extraction rules
pub async fn run_architect<F>(
    target_folder: &Path,
    user_instruction: &str,
    vfs: &ShadowVFS,
    event_emitter: F,
) -> Result<Blueprint, String>
where
    F: Fn(&str, &str, Option<Vec<ExpandableDetail>>),
{
    eprintln!("[Architect] Starting blueprint generation...");
    eprintln!("[Architect] User instruction: {}", user_instruction);

    // Emit progress event
    event_emitter(
        "architect",
        "Designing folder structure...",
        Some(vec![ExpandableDetail {
            label: "Phase".to_string(),
            value: "Architect".to_string(),
        }]),
    );

    // 1. Generate stratified sample with file headers
    let (file_samples, folder_stats) = build_architect_context(target_folder, vfs)?;

    eprintln!(
        "[Architect] Built context with {} samples, {} total files",
        file_samples.len(),
        folder_stats.total_files
    );

    // 2. Build prompt and call Sonnet
    let blueprint = call_architect_llm(user_instruction, &file_samples, &folder_stats).await?;

    eprintln!(
        "[Architect] Blueprint created: {} folders, confidence {:.0}%",
        blueprint.structure.len(),
        blueprint.confidence * 100.0
    );

    // 3. Emit completion event with Blueprint details
    event_emitter(
        "architect",
        &format!("Blueprint: {}", blueprint.strategy_name),
        Some(vec![
            ExpandableDetail {
                label: "Strategy".to_string(),
                value: blueprint.strategy_name.clone(),
            },
            ExpandableDetail {
                label: "Folders".to_string(),
                value: blueprint.structure.len().to_string(),
            },
            ExpandableDetail {
                label: "Confidence".to_string(),
                value: format!("{:.0}%", blueprint.confidence * 100.0),
            },
        ]),
    );

    Ok(blueprint)
}

/// Build context for the Architect from VFS
fn build_architect_context(
    target_folder: &Path,
    vfs: &ShadowVFS,
) -> Result<(Vec<FileSample>, FolderStats), String> {
    let all_files = vfs.all_files_vec();

    // Use existing stratified sampling (max 60 files)
    let sample = sampling::generate_sample(&all_files, 0);

    // Enhance samples with file headers
    let file_samples: Vec<FileSample> = sample
        .samples
        .iter()
        .map(|s| {
            let header = read_file_header(&s.name, target_folder, s.ext.as_deref());
            FileSample {
                name: s.name.clone(),
                extension: s.ext.clone(),
                size: s.size,
                size_formatted: s.size_formatted.clone(),
                modified_at: s.modified_at.clone(),
                header_preview: header,
            }
        })
        .collect();

    // Build folder stats
    let extension_breakdown: Vec<(String, usize)> = sample
        .extensions
        .iter()
        .map(|(ext, stats)| (ext.clone(), stats.count))
        .collect();

    let folder_stats = FolderStats {
        total_files: sample.total_files,
        total_size_mb: sample.total_size_mb,
        extension_breakdown,
        date_range: sample.date_range,
    };

    Ok((file_samples, folder_stats))
}

/// Read first 1KB of a file for context (text files only)
fn read_file_header(filename: &str, root: &Path, ext: Option<&str>) -> Option<String> {
    // Only read text-like files
    if !is_text_extension(ext) {
        return None;
    }

    // Find file in folder (simple recursive search)
    let file_path = find_file_in_folder(root, filename)?;

    // Read first 1KB
    let mut file = File::open(&file_path).ok()?;
    let mut buffer = vec![0u8; MAX_HEADER_SIZE];
    let bytes_read = file.read(&mut buffer).ok()?;

    // Convert to string, handling invalid UTF-8
    let content = String::from_utf8_lossy(&buffer[..bytes_read]);

    // Clean up and truncate
    let cleaned: String = content
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(500) // Keep it reasonable
        .collect();

    if cleaned.trim().is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Check if extension is text-like
fn is_text_extension(ext: Option<&str>) -> bool {
    match ext {
        Some(e) => matches!(
            e.to_lowercase().as_str(),
            "txt" | "md" | "csv" | "json" | "xml" | "html" | "htm" | "yaml" | "yml" | "log"
                | "ini" | "cfg" | "conf" | "py" | "js" | "ts" | "rs" | "go" | "java" | "c"
                | "cpp" | "h" | "hpp" | "css" | "scss" | "less" | "sql" | "sh" | "bash"
                | "zsh" | "toml" | "env" | "gitignore" | "dockerfile"
        ),
        None => false,
    }
}

/// Find a file by name in folder (recursive)
fn find_file_in_folder(root: &Path, filename: &str) -> Option<PathBuf> {
    fn search_recursive(dir: &Path, target: &str) -> Option<PathBuf> {
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name() {
                    if name.to_string_lossy() == target {
                        return Some(path);
                    }
                }
            } else if path.is_dir() {
                // Skip hidden directories
                let name = path.file_name()?.to_string_lossy();
                if !name.starts_with('.') {
                    if let Some(found) = search_recursive(&path, target) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
    search_recursive(root, filename)
}

/// Call Sonnet to generate Blueprint
async fn call_architect_llm(
    user_instruction: &str,
    file_samples: &[FileSample],
    folder_stats: &FolderStats,
) -> Result<Blueprint, String> {
    // Get API key
    let api_key = CredentialManager::get_api_key("anthropic")?;

    let client = Client::new();
    let mut rate_limiter = RateLimitManager::new();

    // Build the prompt
    let prompt = build_architect_prompt(user_instruction, file_samples, folder_stats);

    eprintln!("[Architect] Prompt length: {} chars", prompt.len());

    let request = ArchitectApiRequest {
        model: ClaudeModel::Sonnet.as_str().to_string(),
        max_tokens: 4096,
        system: ARCHITECT_SYSTEM_PROMPT.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    // Send request with retries
    let mut retry_delay = Duration::from_secs(5);
    let mut last_error = String::new();
    let mut response_result = None;

    for retry in 0..=MAX_RETRIES {
        if retry > 0 {
            eprintln!(
                "[Architect] Rate limited, retrying in {:?} (attempt {}/{})",
                retry_delay, retry, MAX_RETRIES
            );
            tokio::time::sleep(retry_delay).await;
            retry_delay *= 2;
        }

        // Apply rate limit delay if needed
        let delay = rate_limiter.get_delay();
        if delay > Duration::ZERO {
            tokio::time::sleep(delay).await;
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
                if let Some(retry_after) = r.headers().get("retry-after") {
                    if let Ok(secs) = retry_after.to_str().unwrap_or("5").parse::<u64>() {
                        retry_delay = Duration::from_secs(secs);
                    }
                }
                last_error = "Rate limit exceeded".to_string();
                continue;
            }
            Ok(r) => {
                rate_limiter.update_from_response(&r);
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

    // Extract text content
    let text = api_response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
        })
        .collect::<Vec<_>>()
        .join("");

    // Parse JSON from response (handle markdown code blocks)
    let json_str = extract_json_from_response(&text)?;

    eprintln!("[Architect] Parsing blueprint JSON...");
    serde_json::from_str::<Blueprint>(&json_str)
        .map_err(|e| format!("Failed to parse blueprint JSON: {}. Response: {}", e, text))
}

/// Extract JSON from response (handles markdown code blocks)
fn extract_json_from_response(text: &str) -> Result<String, String> {
    // Try to find JSON in code blocks first
    if let Some(start) = text.find("```json") {
        let json_start = start + 7;
        if let Some(end) = text[json_start..].find("```") {
            return Ok(text[json_start..json_start + end].trim().to_string());
        }
    }

    // Try plain code blocks
    if let Some(start) = text.find("```") {
        let json_start = start + 3;
        // Skip language identifier if present
        let content_start = text[json_start..]
            .find('\n')
            .map(|i| json_start + i + 1)
            .unwrap_or(json_start);
        if let Some(end) = text[content_start..].find("```") {
            return Ok(text[content_start..content_start + end].trim().to_string());
        }
    }

    // Try to find raw JSON object
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return Ok(text[start..=end].to_string());
        }
    }

    Err("No JSON found in response".to_string())
}

/// Build the prompt context for the Architect LLM call
fn build_architect_prompt(
    user_instruction: &str,
    file_samples: &[FileSample],
    folder_stats: &FolderStats,
) -> String {
    let mut prompt = String::new();

    // User instruction
    prompt.push_str(&format!("## User Request\n{}\n\n", user_instruction));

    // Folder statistics
    prompt.push_str(&format!(
        "## Folder Statistics\n- Total files: {}\n- Total size: {:.1} MB\n",
        folder_stats.total_files, folder_stats.total_size_mb
    ));

    // Date range if available
    if let Some((oldest, newest)) = &folder_stats.date_range {
        prompt.push_str(&format!("- Date range: {} to {}\n", oldest, newest));
    }

    // Extension breakdown
    prompt.push_str("- Extensions: ");
    let ext_summary: Vec<String> = folder_stats
        .extension_breakdown
        .iter()
        .take(15)
        .map(|(ext, count)| format!(".{} ({})", ext, count))
        .collect();
    prompt.push_str(&ext_summary.join(", "));
    prompt.push_str("\n\n");

    // File samples with headers
    prompt.push_str("## Representative File Samples\n");
    for sample in file_samples {
        let ext = sample.extension.as_deref().unwrap_or("no_ext");
        let date = sample.modified_at.as_deref().unwrap_or("unknown");

        prompt.push_str(&format!(
            "- {} (.{}, {}, {})\n",
            sample.name, ext, sample.size_formatted, date
        ));

        // Include header preview if available
        if let Some(ref header) = sample.header_preview {
            let preview: String = header.chars().take(200).collect();
            prompt.push_str(&format!("  Content: \"{}...\"\n", preview));
        }
    }

    prompt.push_str("\n## Instructions\nBased on the user's request and the file samples above, output a Blueprint JSON for organizing these files. Follow the JSON schema exactly.");

    prompt
}

/// Embed Blueprint folder descriptions for vector matching.
/// This prepares the Blueprint for the Builder phase.
pub fn embed_blueprint(
    blueprint: &Blueprint,
    vfs: &ShadowVFS,
) -> Result<Blueprint, String> {
    let mut embedded = blueprint.clone();
    let index = vfs.vector_index();

    // Collect all semantic descriptions
    let descriptions: Vec<&str> = embedded
        .structure
        .iter()
        .map(|f| f.semantic_description.as_str())
        .collect();

    if descriptions.is_empty() {
        return Ok(embedded);
    }

    // Generate embeddings in batch
    let embeddings = index
        .embed_texts(&descriptions)
        .map_err(|e| format!("Failed to embed folder descriptions: {}", e))?;

    // Assign embeddings to folders
    for (folder, embedding) in embedded.structure.iter_mut().zip(embeddings) {
        folder.embedding = Some(embedding);
    }

    eprintln!(
        "[Architect] Embedded {} folder descriptions",
        embedded.structure.len()
    );

    Ok(embedded)
}

/// System prompt for the Architect
const ARCHITECT_SYSTEM_PROMPT: &str = r#"You are the Architect for Sentinel, a file organization AI. Your role is to design high-level organization strategies based on user instructions.

## YOUR TASK

Given:
1. A user's organization instruction (CRITICAL - this defines the strategy)
2. A representative sample of files (with names, sizes, dates, and sometimes content previews)
3. Folder statistics (extension breakdown, date range, etc.)

Output a Blueprint JSON with:
- strategy_name: Human-readable name (e.g., "Project-Based Organization", "Chronological Media Archive")
- structure: Array of target folders with semantic descriptions
- extraction_rules: DSL rules for matching files to folders
- confidence: Your confidence score (0.0-1.0)

## STRUCTURE FORMAT

Each folder entry should have:
- path: Relative folder path (e.g., "Documents/Invoices/2024")
- semanticDescription: Natural language description for vector matching (important for finding similar files)
- expectedExtensions: Likely file extensions

## EXAMPLE OUTPUT

```json
{
  "strategyName": "Financial Document Organization",
  "structure": [
    {
      "path": "Invoices/2024",
      "semanticDescription": "tax invoices billing statements receipts 2024 financial records",
      "expectedExtensions": ["pdf", "doc", "docx"]
    },
    {
      "path": "Contracts",
      "semanticDescription": "legal contracts agreements signed documents terms conditions",
      "expectedExtensions": ["pdf"]
    },
    {
      "path": "Misc",
      "semanticDescription": "miscellaneous files that don't fit other categories",
      "expectedExtensions": []
    }
  ],
  "extractionRules": "file.name MATCHES '(?i)invoice' => Invoices/{year}\nfile.name MATCHES '(?i)contract' => Contracts\n* => Misc",
  "confidence": 0.85
}
```

## GUIDELINES

1. **Follow the user's instruction precisely** - This is the most important input
2. **Be specific** - Create targeted folders based on the instruction, not generic ones
3. **Use rich semantic descriptions** - These drive vector matching (include synonyms, related terms)
4. **Consider the samples** - Base structure on actual file patterns you see
5. **Keep it flat** - Max 3 levels of nesting for usability
6. **Handle edge cases** - Always include a "Misc" or "Unsorted" folder for outliers
7. **Extraction rules** - Write simple DSL rules that match files to folders

## COMMON PATTERNS

- By date: "Photos/2024/January", "Documents/2023/Q4"
- By type: "Images", "Documents", "Code", "Media"
- By project: "Project-Alpha", "Client-Work/Acme"
- By vendor/source: "Invoices/Apple", "Receipts/Amazon"
- Hybrid: "Work/2024/Projects", "Personal/Photos/Vacation"

Output ONLY valid JSON, no markdown explanation outside the code block."#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_markdown() {
        let text = r#"Here's the blueprint:
```json
{"strategyName": "Test", "structure": [], "extractionRules": "", "confidence": 0.9}
```
That's the plan."#;

        let json = extract_json_from_response(text).unwrap();
        assert!(json.contains("strategyName"));
    }

    #[test]
    fn test_extract_raw_json() {
        let text = r#"{"strategyName": "Test", "structure": [], "extractionRules": "", "confidence": 0.9}"#;
        let json = extract_json_from_response(text).unwrap();
        assert!(json.contains("strategyName"));
    }

    #[test]
    fn test_is_text_extension() {
        assert!(is_text_extension(Some("txt")));
        assert!(is_text_extension(Some("md")));
        assert!(is_text_extension(Some("json")));
        assert!(!is_text_extension(Some("pdf")));
        assert!(!is_text_extension(Some("jpg")));
        assert!(!is_text_extension(None));
    }
}
