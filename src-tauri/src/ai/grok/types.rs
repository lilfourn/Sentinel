//! Shared types for Grok multi-agent system

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Result of analyzing a single document
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentAnalysis {
    /// Original file path
    pub file_path: String,

    /// Original filename
    pub file_name: String,

    /// Concise summary of document content (1-2 sentences)
    pub content_summary: String,

    /// Document type classification
    pub document_type: DocumentType,

    /// Key entities extracted (people, companies, dates, amounts)
    pub key_entities: Vec<String>,

    /// AI-suggested descriptive filename (without extension)
    pub suggested_name: Option<String>,

    /// Confidence score (0.0-1.0)
    pub confidence: f32,

    /// Analysis method used
    pub method: AnalysisMethod,
}

/// Document type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Invoice,
    Contract,
    Report,
    Letter,
    Form,
    Receipt,
    Statement,
    Proposal,
    Presentation,
    Spreadsheet,
    Manual,
    Certificate,
    License,
    Permit,
    Application,
    Resume,
    Photo,
    Diagram,
    Drawing,
    Unknown,
}

impl DocumentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Invoice => "invoice",
            Self::Contract => "contract",
            Self::Report => "report",
            Self::Letter => "letter",
            Self::Form => "form",
            Self::Receipt => "receipt",
            Self::Statement => "statement",
            Self::Proposal => "proposal",
            Self::Presentation => "presentation",
            Self::Spreadsheet => "spreadsheet",
            Self::Manual => "manual",
            Self::Certificate => "certificate",
            Self::License => "license",
            Self::Permit => "permit",
            Self::Application => "application",
            Self::Resume => "resume",
            Self::Photo => "photo",
            Self::Diagram => "diagram",
            Self::Drawing => "drawing",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "invoice" => Self::Invoice,
            "contract" => Self::Contract,
            "report" => Self::Report,
            "letter" => Self::Letter,
            "form" => Self::Form,
            "receipt" => Self::Receipt,
            "statement" => Self::Statement,
            "proposal" => Self::Proposal,
            "presentation" => Self::Presentation,
            "spreadsheet" => Self::Spreadsheet,
            "manual" => Self::Manual,
            "certificate" => Self::Certificate,
            "license" => Self::License,
            "permit" => Self::Permit,
            "application" => Self::Application,
            "resume" | "cv" => Self::Resume,
            "photo" | "image" => Self::Photo,
            "diagram" => Self::Diagram,
            "drawing" => Self::Drawing,
            _ => Self::Unknown,
        }
    }
}

/// How the document was analyzed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMethod {
    /// Retrieved from persistent cache
    Cached,
    /// Text extracted directly from file
    TextExtraction,
    /// Analyzed via Grok Vision API
    GrokVision,
    /// OCR fallback
    Ocr,
    /// Metadata only (filename, extension, size)
    MetadataOnly,
}

/// Batch of files for an explore agent
#[derive(Debug, Clone)]
pub struct ExploreBatch {
    pub batch_id: usize,
    pub files: Vec<PathBuf>,
}

/// Result from an explore agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExploreResult {
    pub batch_id: usize,
    pub analyses: Vec<DocumentAnalysis>,
    pub failed_files: Vec<(String, String)>, // (path, error)
    pub total_tokens_used: u32,
    pub duration_ms: u64,
}

/// Folder assignment from orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderAssignment {
    pub file_path: String,
    pub original_name: String,
    pub destination_folder: String,
    pub new_name: Option<String>,
    pub confidence: f32,
}

impl FolderAssignment {
    /// Get the sanitized new filename, falling back to original if not set
    #[allow(dead_code)]
    pub fn get_sanitized_new_name(&self) -> String {
        match &self.new_name {
            Some(name) if !name.is_empty() => sanitize_filename(name, &self.original_name),
            _ => self.original_name.clone(),
        }
    }
}

/// Sanitize a filename to follow naming conventions:
/// - Replace spaces with hyphens
/// - Remove special characters
/// - Ensure extension is preserved
/// - Limit length to 80 characters
pub fn sanitize_filename(name: &str, original_name: &str) -> String {
    // Extract original extension
    let original_ext = std::path::Path::new(original_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Remove any extension from the new name (we'll add the original back)
    let name_without_ext = if let Some(dot_pos) = name.rfind('.') {
        let potential_ext = &name[dot_pos + 1..];
        // Only strip if it looks like an extension (short, alphanumeric)
        if potential_ext.len() <= 5 && potential_ext.chars().all(|c| c.is_alphanumeric()) {
            &name[..dot_pos]
        } else {
            name
        }
    } else {
        name
    };

    // Sanitize the name
    let sanitized: String = name_without_ext
        .chars()
        .map(|c| match c {
            ' ' | '_' => '-',  // Replace spaces and underscores with hyphens
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-', // Replace invalid chars
            c if c.is_alphanumeric() || c == '-' || c == '.' => c,
            _ => '-',
        })
        .collect();

    // Remove consecutive hyphens
    let mut result = String::new();
    let mut last_was_hyphen = false;
    for c in sanitized.chars() {
        if c == '-' {
            if !last_was_hyphen {
                result.push(c);
                last_was_hyphen = true;
            }
        } else {
            result.push(c);
            last_was_hyphen = false;
        }
    }

    // Trim hyphens from start and end
    let result = result.trim_matches('-');

    // Limit length (leaving room for extension)
    let max_name_len = 75 - original_ext.len();
    let result = if result.len() > max_name_len {
        &result[..max_name_len].trim_end_matches('-')
    } else {
        result
    };

    // Add extension back
    if original_ext.is_empty() {
        result.to_string()
    } else {
        format!("{}.{}", result, original_ext.to_lowercase())
    }
}

/// Sanitize a folder path
pub fn sanitize_folder_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            segment
                .chars()
                .map(|c| match c {
                    ' ' | '_' => '-',
                    c if c.is_alphanumeric() || c == '-' || c == '.' => c,
                    _ => '-',
                })
                .collect::<String>()
                .trim_matches('-')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Complete organization plan from orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationPlan {
    /// Detected file domain (e.g., "Software developer workspace", "Business records")
    #[serde(default)]
    pub detected_domain: Option<String>,
    /// Key entities extracted from content (company names, project names, etc.)
    #[serde(default)]
    pub key_entities_found: Vec<String>,
    pub strategy_name: String,
    pub description: String,
    pub folder_structure: Vec<PlannedFolder>,
    pub assignments: Vec<FolderAssignment>,
    pub unassigned_files: Vec<String>,
}

/// A folder in the planned structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedFolder {
    pub path: String,
    pub description: String,
    pub expected_file_count: usize,
}

/// Configuration for the multi-agent system
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GrokConfig {
    /// API key for xAI
    pub api_key: String,

    /// Base URL for API (default: https://api.x.ai)
    pub base_url: String,

    /// Model to use (default: grok-4-1-fast)
    pub model: String,

    /// Maximum concurrent explore agents
    pub max_parallel_agents: usize,

    /// Files per explore agent batch
    pub batch_size: usize,

    /// Maximum cost in cents per job
    pub budget_cents: u32,

    /// Rate limit: requests per second
    pub requests_per_second: f32,

    /// Rate limit: max concurrent requests
    pub max_concurrent_requests: usize,
}

impl Default for GrokConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.x.ai".to_string(),
            model: "grok-4-1-fast".to_string(),
            max_parallel_agents: 4,
            batch_size: 50,
            budget_cents: 100, // $1 default budget
            requests_per_second: 5.0,
            max_concurrent_requests: 10,
        }
    }
}

/// Progress event for UI updates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProgress {
    pub phase: AnalysisPhase,
    pub current: usize,
    pub total: usize,
    pub current_file: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisPhase {
    Scanning,
    CheckingCache,
    RenderingPdf,
    AnalyzingContent,
    Aggregating,
    Planning,
    Complete,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename_spaces() {
        let result = sanitize_filename("Acme Corp Invoice 2024", "document.pdf");
        assert_eq!(result, "Acme-Corp-Invoice-2024.pdf");
    }

    #[test]
    fn test_sanitize_filename_preserves_extension() {
        let result = sanitize_filename("Report Final", "spreadsheet.xlsx");
        assert_eq!(result, "Report-Final.xlsx");
    }

    #[test]
    fn test_sanitize_filename_removes_special_chars() {
        let result = sanitize_filename("Invoice #123: Test?", "doc.pdf");
        assert_eq!(result, "Invoice-123-Test.pdf");
    }

    #[test]
    fn test_sanitize_filename_handles_underscores() {
        let result = sanitize_filename("acme_corp_invoice_2024", "file.pdf");
        assert_eq!(result, "acme-corp-invoice-2024.pdf");
    }

    #[test]
    fn test_sanitize_filename_removes_consecutive_hyphens() {
        let result = sanitize_filename("Test---Multiple---Hyphens", "file.pdf");
        assert_eq!(result, "Test-Multiple-Hyphens.pdf");
    }

    #[test]
    fn test_sanitize_filename_handles_extension_in_name() {
        let result = sanitize_filename("Report-2024.pdf", "original.pdf");
        assert_eq!(result, "Report-2024.pdf");
    }

    #[test]
    fn test_sanitize_folder_path() {
        let result = sanitize_folder_path("Clients/Acme Corp/2024 Q1/Invoices");
        assert_eq!(result, "Clients/Acme-Corp/2024-Q1/Invoices");
    }

    #[test]
    fn test_sanitize_folder_path_removes_empty() {
        let result = sanitize_folder_path("Clients//Empty//Test");
        assert_eq!(result, "Clients/Empty/Test");
    }
}
