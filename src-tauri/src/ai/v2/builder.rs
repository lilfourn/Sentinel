//! Builder Module - Intelligent file slotting using Blueprint.
//!
//! The Builder uses the Architect's Blueprint to slot files into
//! the target structure using a tiered matching approach:
//!
//! - **Tier 1**: Vector similarity (>0.85 confidence) - immediate slot
//! - **Tier 2**: LLM read (Haiku) - for ambiguous files
//!
//! This approach minimizes expensive LLM calls by using fast vector
//! matching for the majority of files.

use crate::ai::client::ClaudeModel;
use crate::ai::credentials::CredentialManager;
use crate::ai::rules::VirtualFile;

use super::agent_loop::ExpandableDetail;
use super::architect::{Blueprint, BlueprintFolder};
use super::local_vector_index::LocalVectorIndex;
use super::vfs::ShadowVFS;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Confidence threshold for automatic Tier 1 slotting
const TIER1_THRESHOLD: f32 = 0.85;

/// Minimum confidence to consider a match at all
const MIN_MATCH_THRESHOLD: f32 = 0.4;

/// Maximum files per Haiku batch call
const HAIKU_BATCH_SIZE: usize = 10;

/// Maximum retries for rate limit errors
const MAX_RETRIES: u32 = 3;

/// Result of matching a file to the Blueprint
#[derive(Debug, Clone)]
pub enum MatchResult {
    /// Tier 1: High confidence vector match
    Tier1Match {
        file_path: String,
        destination_folder: String,
        confidence: f32,
    },
    /// Tier 2: Needs LLM disambiguation
    Tier2Ambiguous {
        file_path: String,
        file_name: String,
        candidates: Vec<(String, f32)>, // (folder_path, score)
    },
    /// No match found - goes to Misc/Unsorted
    NoMatch { file_path: String },
}

/// Result of batch matching all files
#[derive(Debug)]
pub struct BatchMatchResult {
    /// Tier 1: High confidence matches (file_path, destination, confidence)
    pub tier1_matches: Vec<(String, String, f32)>,
    /// Tier 2: Ambiguous files needing LLM (file_path, file_name, candidates)
    pub tier2_ambiguous: Vec<(String, String, Vec<(String, f32)>)>,
    /// Files with no match
    pub no_matches: Vec<String>,
}

/// Match a single file against the Blueprint structure
pub fn match_file_to_blueprint(
    file: &VirtualFile,
    blueprint: &Blueprint,
    index: &LocalVectorIndex,
) -> Result<MatchResult, String> {
    // Build searchable text from file
    let file_text = format!(
        "{} {}",
        file.name,
        file.ext.as_deref().unwrap_or("")
    );

    // Get file embedding
    let file_embeddings = index
        .embed_texts(&[file_text.as_str()])
        .map_err(|e| format!("Failed to embed file: {}", e))?;

    let file_embedding = file_embeddings
        .into_iter()
        .next()
        .ok_or("No embedding generated for file")?;

    // Score against all Blueprint folders
    let mut scores: Vec<(String, f32)> = blueprint
        .structure
        .iter()
        .filter_map(|folder| {
            folder.embedding.as_ref().map(|folder_emb| {
                let score = cosine_similarity(&file_embedding, folder_emb);
                (folder.path.clone(), score)
            })
        })
        .filter(|(_, score)| *score >= MIN_MATCH_THRESHOLD)
        .collect();

    // Sort by score descending
    scores.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if scores.is_empty() {
        return Ok(MatchResult::NoMatch {
            file_path: file.path.clone(),
        });
    }

    let (best_folder, best_score) = &scores[0];

    // Tier 1: High confidence match
    if *best_score >= TIER1_THRESHOLD {
        return Ok(MatchResult::Tier1Match {
            file_path: file.path.clone(),
            destination_folder: best_folder.clone(),
            confidence: *best_score,
        });
    }

    // Tier 2: Ambiguous - needs LLM
    let candidates: Vec<(String, f32)> = scores.into_iter().take(3).collect();

    Ok(MatchResult::Tier2Ambiguous {
        file_path: file.path.clone(),
        file_name: file.name.clone(),
        candidates,
    })
}

/// Batch match all files, returning Tier 1 matches and Tier 2 candidates
pub fn batch_match_files(
    files: &[VirtualFile],
    blueprint: &Blueprint,
    index: &LocalVectorIndex,
) -> Result<BatchMatchResult, String> {
    let mut tier1_matches = Vec::new();
    let mut tier2_ambiguous = Vec::new();
    let mut no_matches = Vec::new();

    for file in files {
        if file.is_directory {
            continue;
        }

        match match_file_to_blueprint(file, blueprint, index)? {
            MatchResult::Tier1Match {
                file_path,
                destination_folder,
                confidence,
            } => {
                tier1_matches.push((file_path, destination_folder, confidence));
            }
            MatchResult::Tier2Ambiguous {
                file_path,
                file_name,
                candidates,
            } => {
                tier2_ambiguous.push((file_path, file_name, candidates));
            }
            MatchResult::NoMatch { file_path } => {
                no_matches.push(file_path);
            }
        }
    }

    Ok(BatchMatchResult {
        tier1_matches,
        tier2_ambiguous,
        no_matches,
    })
}

/// Resolve Tier 2 ambiguous files using Haiku LLM
pub async fn resolve_tier2_with_llm<F>(
    ambiguous_files: &[(String, String, Vec<(String, f32)>)],
    blueprint: &Blueprint,
    event_emitter: F,
) -> Result<Vec<(String, String)>, String>
where
    F: Fn(&str, &str, Option<Vec<ExpandableDetail>>),
{
    if ambiguous_files.is_empty() {
        return Ok(Vec::new());
    }

    eprintln!(
        "[Builder] Resolving {} ambiguous files with Haiku",
        ambiguous_files.len()
    );

    event_emitter(
        "builder",
        &format!("Analyzing {} ambiguous files...", ambiguous_files.len()),
        Some(vec![ExpandableDetail {
            label: "Method".to_string(),
            value: "Haiku LLM".to_string(),
        }]),
    );

    // Batch files into groups
    let mut resolved = Vec::new();

    for chunk in ambiguous_files.chunks(HAIKU_BATCH_SIZE) {
        let resolutions = call_haiku_for_disambiguation(chunk, blueprint).await?;
        resolved.extend(resolutions);
    }

    Ok(resolved)
}

/// Call Haiku to resolve ambiguous file placements
async fn call_haiku_for_disambiguation(
    files: &[(String, String, Vec<(String, f32)>)],
    blueprint: &Blueprint,
) -> Result<Vec<(String, String)>, String> {
    // Get API key
    let api_key = CredentialManager::get_api_key("anthropic")?;

    let client = Client::new();

    // Build context with file details and candidate folders
    let prompt = build_disambiguation_prompt(files, blueprint);

    let request = HaikuApiRequest {
        model: ClaudeModel::Haiku.as_str().to_string(),
        max_tokens: 1024,
        system: HAIKU_DISAMBIGUATION_PROMPT.to_string(),
        messages: vec![HaikuMessage {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    // Send request with retries
    let mut retry_delay = Duration::from_secs(2);
    let mut last_error = String::new();
    let mut response_result = None;

    for retry in 0..=MAX_RETRIES {
        if retry > 0 {
            tokio::time::sleep(retry_delay).await;
            retry_delay *= 2;
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

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("API error: {}", error_text));
    }

    let api_response: HaikuApiResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Extract text content
    let text = api_response
        .content
        .iter()
        .filter_map(|block| match block {
            HaikuContentBlock::Text { text } => Some(text.as_str()),
        })
        .collect::<Vec<_>>()
        .join("");

    // Parse response
    parse_disambiguation_response(&text, files)
}

/// Build prompt for Haiku disambiguation
fn build_disambiguation_prompt(
    files: &[(String, String, Vec<(String, f32)>)],
    blueprint: &Blueprint,
) -> String {
    let mut prompt = String::new();

    prompt.push_str(&format!(
        "Strategy: {}\n\n",
        blueprint.strategy_name
    ));

    prompt.push_str("Available folders:\n");
    for folder in &blueprint.structure {
        prompt.push_str(&format!("- {} ({})\n", folder.path, folder.semantic_description));
    }
    prompt.push_str("\n");

    prompt.push_str("Files to categorize:\n");
    for (i, (file_path, file_name, candidates)) in files.iter().enumerate() {
        prompt.push_str(&format!("{}. {} (path: {})\n", i + 1, file_name, file_path));
        prompt.push_str("   Candidates: ");
        let cand_str: Vec<String> = candidates
            .iter()
            .map(|(folder, score)| format!("{} ({:.0}%)", folder, score * 100.0))
            .collect();
        prompt.push_str(&cand_str.join(", "));
        prompt.push_str("\n");
    }

    prompt.push_str("\nFor each file, output the best folder on a single line:\n");
    prompt.push_str("Format: <number>: <folder_path>\n");

    prompt
}

/// Parse disambiguation response from Haiku
fn parse_disambiguation_response(
    text: &str,
    files: &[(String, String, Vec<(String, f32)>)],
) -> Result<Vec<(String, String)>, String> {
    let mut resolved = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse format: "1: Documents/Invoices" or "1. Documents/Invoices"
        let parts: Vec<&str> = line.splitn(2, |c| c == ':' || c == '.').collect();
        if parts.len() != 2 {
            continue;
        }

        let index: usize = match parts[0].trim().parse() {
            Ok(i) => i,
            Err(_) => continue,
        };

        let folder = parts[1].trim().to_string();

        // Index is 1-based in the prompt
        if index > 0 && index <= files.len() {
            let file_path = files[index - 1].0.clone();
            resolved.push((file_path, folder));
        }
    }

    Ok(resolved)
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denominator = norm_a.sqrt() * norm_b.sqrt();
    if denominator == 0.0 {
        0.0
    } else {
        dot / denominator
    }
}

/// Haiku API request
#[derive(Serialize)]
struct HaikuApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<HaikuMessage>,
}

#[derive(Serialize)]
struct HaikuMessage {
    role: String,
    content: String,
}

/// Haiku API response
#[derive(Deserialize)]
struct HaikuApiResponse {
    content: Vec<HaikuContentBlock>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum HaikuContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

/// System prompt for Haiku disambiguation
const HAIKU_DISAMBIGUATION_PROMPT: &str = r#"You are a file categorization assistant. Given a list of files and their candidate folders, choose the single best folder for each file.

Output format - one line per file:
<number>: <folder_path>

Example:
1: Documents/Invoices
2: Media/Photos
3: Misc

Be decisive. Choose exactly one folder per file. Use the exact folder paths provided."#;

/// Generate file move operations from match results
pub fn generate_operations_from_matches(
    tier1_matches: &[(String, String, f32)],
    tier2_resolved: &[(String, String)],
    no_matches: &[String],
    misc_folder: &str,
) -> Vec<(String, String)> {
    let mut operations = Vec::new();

    // Tier 1 matches
    for (file_path, folder, _confidence) in tier1_matches {
        operations.push((file_path.clone(), folder.clone()));
    }

    // Tier 2 resolved
    for (file_path, folder) in tier2_resolved {
        operations.push((file_path.clone(), folder.clone()));
    }

    // No matches go to Misc folder
    for file_path in no_matches {
        operations.push((file_path.clone(), misc_folder.to_string()));
    }

    operations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 0.001);

        let d = vec![1.0, 1.0, 0.0];
        let expected = 1.0 / 2.0_f32.sqrt();
        assert!((cosine_similarity(&a, &d) - expected).abs() < 0.001);
    }

    #[test]
    fn test_parse_disambiguation_response() {
        let text = "1: Documents/Invoices\n2: Media/Photos\n3: Misc";
        let files = vec![
            ("file1.pdf".to_string(), "invoice.pdf".to_string(), vec![]),
            ("file2.jpg".to_string(), "photo.jpg".to_string(), vec![]),
            ("file3.txt".to_string(), "notes.txt".to_string(), vec![]),
        ];

        let result = parse_disambiguation_response(text, &files).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], ("file1.pdf".to_string(), "Documents/Invoices".to_string()));
        assert_eq!(result[1], ("file2.jpg".to_string(), "Media/Photos".to_string()));
        assert_eq!(result[2], ("file3.txt".to_string(), "Misc".to_string()));
    }
}
