//! FolderDigest Analytics Module
//!
//! Pre-computes rich folder analytics for one-shot AI planning:
//! - Extension counts and MIME breakdown
//! - Date range analysis
//! - Common filename prefixes
//! - Content previews from key files
//! - Semantic tags from embeddings
//!
//! The digest is injected into the AI prompt to enable immediate
//! organization planning without exploration iterations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::local_vector_index::LocalVectorIndex;
use crate::utils::format_size;

/// Maximum content preview length per file (characters)
const MAX_PREVIEW_LENGTH: usize = 200;

/// Maximum files to sample for content previews
const MAX_PREVIEW_FILES: usize = 10;

/// Maximum recursion depth for folder scanning
const MAX_SCAN_DEPTH: usize = 20;

/// Comprehensive folder analysis for AI context
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDigest {
    /// Root folder path
    pub root_path: String,

    /// Total file count
    pub file_count: usize,

    /// Total directory count
    pub dir_count: usize,

    /// Total size in bytes
    pub total_size: u64,

    /// File count by extension (e.g., {"pdf": 45, "jpg": 120})
    pub ext_counts: HashMap<String, usize>,

    /// File count by MIME type category (e.g., {"image": 150, "document": 45})
    pub mime_breakdown: HashMap<String, usize>,

    /// Date range (min_timestamp, max_timestamp) in milliseconds
    pub date_range: (i64, i64),

    /// Common filename prefixes (e.g., ["IMG_", "Screenshot", "Invoice-"])
    pub common_prefixes: Vec<String>,

    /// Sampled content previews from representative files
    pub content_previews: Vec<ContentPreview>,

    /// Semantic tags derived from embeddings (top categories)
    pub semantic_tags: Vec<SemanticTag>,

    /// Maximum folder depth encountered
    pub max_depth: usize,

    /// Hidden files count (files starting with '.')
    pub hidden_count: usize,
}

/// Content preview from a sampled file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPreview {
    /// Relative path from root
    pub path: String,
    /// File extension
    pub extension: Option<String>,
    /// MIME type
    pub mime_type: Option<String>,
    /// First N characters of content (for text files)
    pub preview: String,
    /// File size in bytes
    pub size: u64,
}

/// Semantic tag with confidence/coverage score
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTag {
    /// Tag name (e.g., "invoice", "photo", "code")
    pub tag: String,
    /// Percentage of files matching this tag (0.0 - 1.0)
    pub coverage: f32,
    /// Number of files with this tag
    pub count: usize,
}

/// Folder digest generator
///
/// Scans a directory and computes comprehensive analytics
/// for injection into AI prompts.
pub struct DigestGenerator {
    /// Minimum files for prefix detection
    prefix_threshold: usize,
    /// Minimum prefix occurrences to be considered "common"
    min_prefix_count: usize,
}

impl DigestGenerator {
    /// Create a new DigestGenerator with default settings
    pub fn new() -> Self {
        Self {
            prefix_threshold: 10,
            min_prefix_count: 5,
        }
    }

    /// Create with custom thresholds
    pub fn with_thresholds(prefix_threshold: usize, min_prefix_count: usize) -> Self {
        Self {
            prefix_threshold,
            min_prefix_count,
        }
    }

    /// Generate a complete FolderDigest for the given path
    ///
    /// # Arguments
    /// * `root` - Path to the folder to analyze
    /// * `vector_index` - Optional LocalVectorIndex for semantic tags
    ///
    /// # Returns
    /// A FolderDigest containing comprehensive analytics
    pub fn generate(
        &self,
        root: &Path,
        vector_index: Option<&LocalVectorIndex>,
    ) -> Result<FolderDigest, String> {
        if !root.exists() || !root.is_dir() {
            return Err(format!("Invalid directory: {:?}", root));
        }

        eprintln!("[DigestGenerator] Analyzing folder: {:?}", root);

        let mut file_count = 0;
        let mut dir_count = 0;
        let mut total_size: u64 = 0;
        let mut ext_counts: HashMap<String, usize> = HashMap::new();
        let mut mime_breakdown: HashMap<String, usize> = HashMap::new();
        let mut min_timestamp: i64 = i64::MAX;
        let mut max_timestamp: i64 = i64::MIN;
        let mut filename_prefixes: HashMap<String, usize> = HashMap::new();
        let mut max_depth: usize = 0;
        let mut hidden_count = 0;

        // Files suitable for content preview
        let mut preview_candidates: Vec<(PathBuf, u64, Option<String>)> = Vec::new();

        // Recursive scan
        self.scan_directory(
            root,
            root,
            0,
            &mut file_count,
            &mut dir_count,
            &mut total_size,
            &mut ext_counts,
            &mut mime_breakdown,
            &mut min_timestamp,
            &mut max_timestamp,
            &mut filename_prefixes,
            &mut max_depth,
            &mut hidden_count,
            &mut preview_candidates,
        )?;

        eprintln!(
            "[DigestGenerator] Scanned {} files in {} directories",
            file_count, dir_count
        );

        // Compute common prefixes
        let common_prefixes = self.compute_common_prefixes(&filename_prefixes, file_count);

        // Generate content previews
        let content_previews = self.generate_previews(&preview_candidates, root);

        // Compute semantic tags from vector index
        let semantic_tags = if let Some(index) = vector_index {
            self.compute_semantic_tags(index, file_count)
        } else {
            Vec::new()
        };

        Ok(FolderDigest {
            root_path: root.to_string_lossy().to_string(),
            file_count,
            dir_count,
            total_size,
            ext_counts,
            mime_breakdown,
            date_range: (
                if min_timestamp == i64::MAX {
                    0
                } else {
                    min_timestamp
                },
                if max_timestamp == i64::MIN {
                    0
                } else {
                    max_timestamp
                },
            ),
            common_prefixes,
            content_previews,
            semantic_tags,
            max_depth,
            hidden_count,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_directory(
        &self,
        path: &Path,
        root: &Path,
        depth: usize,
        file_count: &mut usize,
        dir_count: &mut usize,
        total_size: &mut u64,
        ext_counts: &mut HashMap<String, usize>,
        mime_breakdown: &mut HashMap<String, usize>,
        min_timestamp: &mut i64,
        max_timestamp: &mut i64,
        filename_prefixes: &mut HashMap<String, usize>,
        max_depth: &mut usize,
        hidden_count: &mut usize,
        preview_candidates: &mut Vec<(PathBuf, u64, Option<String>)>,
    ) -> Result<(), String> {
        *max_depth = (*max_depth).max(depth);

        let entries = fs::read_dir(path).map_err(|e| format!("Failed to read {:?}: {}", path, e))?;

        for entry in entries.filter_map(|e| e.ok()) {
            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Track hidden files but skip processing them
            if name.starts_with('.') {
                *hidden_count += 1;
                continue;
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            if metadata.is_dir() {
                *dir_count += 1;
                // Recurse with depth limit
                if depth < MAX_SCAN_DEPTH {
                    let _ = self.scan_directory(
                        &entry_path,
                        root,
                        depth + 1,
                        file_count,
                        dir_count,
                        total_size,
                        ext_counts,
                        mime_breakdown,
                        min_timestamp,
                        max_timestamp,
                        filename_prefixes,
                        max_depth,
                        hidden_count,
                        preview_candidates,
                    );
                }
            } else if metadata.is_file() {
                *file_count += 1;
                *total_size += metadata.len();

                // Extension counting
                let ext = entry_path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_else(|| "none".to_string());
                *ext_counts.entry(ext.clone()).or_insert(0) += 1;

                // MIME breakdown
                let mime_category = get_mime_category(&ext);
                *mime_breakdown.entry(mime_category).or_insert(0) += 1;

                // Timestamp tracking
                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        let ts = duration.as_millis() as i64;
                        *min_timestamp = (*min_timestamp).min(ts);
                        *max_timestamp = (*max_timestamp).max(ts);
                    }
                }

                // Filename prefix extraction
                if let Some(prefix) = extract_prefix(&name) {
                    *filename_prefixes.entry(prefix).or_insert(0) += 1;
                }

                // Content preview candidates (text-like files, reasonable size)
                if is_previewable(&ext) && metadata.len() < 1_000_000 {
                    let mime = mime_guess::from_ext(&ext).first().map(|m| m.to_string());
                    preview_candidates.push((entry_path, metadata.len(), mime));
                }
            }
        }

        Ok(())
    }

    fn compute_common_prefixes(
        &self,
        prefixes: &HashMap<String, usize>,
        total_files: usize,
    ) -> Vec<String> {
        if total_files < self.prefix_threshold {
            return Vec::new();
        }

        let mut common: Vec<(String, usize)> = prefixes
            .iter()
            .filter(|(_, count)| **count >= self.min_prefix_count)
            .map(|(p, c)| (p.clone(), *c))
            .collect();

        common.sort_by(|a, b| b.1.cmp(&a.1));
        common.into_iter().take(10).map(|(p, _)| p).collect()
    }

    fn generate_previews(
        &self,
        candidates: &[(PathBuf, u64, Option<String>)],
        root: &Path,
    ) -> Vec<ContentPreview> {
        let mut previews = Vec::new();

        // Sample diverse files (different extensions)
        let mut seen_exts: HashMap<String, usize> = HashMap::new();

        for (path, size, mime) in candidates.iter().take(MAX_PREVIEW_FILES * 3) {
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();

            // Limit 2 previews per extension for diversity
            let count = seen_exts.entry(ext.clone()).or_insert(0);
            if *count >= 2 {
                continue;
            }
            *count += 1;

            // Read content preview
            let preview = match fs::File::open(path) {
                Ok(mut file) => {
                    let mut buffer = vec![0u8; MAX_PREVIEW_LENGTH];
                    match file.read(&mut buffer) {
                        Ok(n) => String::from_utf8_lossy(&buffer[..n])
                            .chars()
                            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                            .take(MAX_PREVIEW_LENGTH)
                            .collect(),
                        Err(_) => continue,
                    }
                }
                Err(_) => continue,
            };

            let rel_path = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string());

            previews.push(ContentPreview {
                path: rel_path,
                extension: if ext.is_empty() { None } else { Some(ext) },
                mime_type: mime.clone(),
                preview,
                size: *size,
            });

            if previews.len() >= MAX_PREVIEW_FILES {
                break;
            }
        }

        previews
    }

    fn compute_semantic_tags(
        &self,
        index: &LocalVectorIndex,
        total_files: usize,
    ) -> Vec<SemanticTag> {
        if index.is_empty() || total_files == 0 {
            return Vec::new();
        }

        // Categories to search for
        let categories = vec![
            ("invoice", "invoice billing receipt payment"),
            ("receipt", "receipt purchase transaction"),
            ("document", "document report letter memo"),
            ("photo", "photo picture image photograph"),
            ("screenshot", "screenshot screen capture"),
            ("code", "code programming source script"),
            ("config", "config configuration settings"),
            ("archive", "archive backup compressed zip"),
            ("video", "video movie recording clip"),
            ("audio", "audio music sound recording"),
        ];

        let mut tags = Vec::new();

        for (tag_name, query) in categories {
            // Search index for this category
            match index.search(query) {
                Ok(results) => {
                    // Count files above a reasonable threshold (0.4)
                    let count = results.iter().filter(|(_, score)| *score > 0.4).count();
                    if count > 0 {
                        let coverage = count as f32 / total_files as f32;
                        tags.push(SemanticTag {
                            tag: tag_name.to_string(),
                            coverage,
                            count,
                        });
                    }
                }
                Err(_) => continue,
            }
        }

        // Sort by coverage descending
        tags.sort_by(|a, b| b.coverage.partial_cmp(&a.coverage).unwrap_or(std::cmp::Ordering::Equal));

        // Return top tags with meaningful coverage (>5%)
        tags.into_iter().filter(|t| t.coverage > 0.05).take(5).collect()
    }
}

impl Default for DigestGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl FolderDigest {
    /// Format as concise text for AI prompt injection
    ///
    /// Returns a human-readable summary suitable for the AI context
    pub fn to_prompt_text(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!("## Folder Analysis: {}", self.root_path));
        lines.push(format!(
            "- {} files, {} directories",
            self.file_count, self.dir_count
        ));
        lines.push(format!("- Total size: {}", format_size(self.total_size)));

        // Top extensions
        let mut exts: Vec<_> = self.ext_counts.iter().collect();
        exts.sort_by(|a, b| b.1.cmp(a.1));
        let top_exts: Vec<String> = exts
            .iter()
            .take(5)
            .map(|(ext, count)| format!("{} ({})", ext, count))
            .collect();
        if !top_exts.is_empty() {
            lines.push(format!("- Top extensions: {}", top_exts.join(", ")));
        }

        // MIME breakdown
        let mut mimes: Vec<_> = self.mime_breakdown.iter().collect();
        mimes.sort_by(|a, b| b.1.cmp(a.1));
        let mime_summary: Vec<String> = mimes
            .iter()
            .take(4)
            .map(|(cat, count)| format!("{}: {}", cat, count))
            .collect();
        if !mime_summary.is_empty() {
            lines.push(format!("- Content types: {}", mime_summary.join(", ")));
        }

        // Date range
        if self.date_range.0 > 0 && self.date_range.1 > 0 {
            let min_date = chrono::DateTime::from_timestamp_millis(self.date_range.0)
                .map(|d| d.format("%Y-%m").to_string())
                .unwrap_or_default();
            let max_date = chrono::DateTime::from_timestamp_millis(self.date_range.1)
                .map(|d| d.format("%Y-%m").to_string())
                .unwrap_or_default();
            if !min_date.is_empty() && !max_date.is_empty() {
                lines.push(format!("- Date range: {} to {}", min_date, max_date));
            }
        }

        // Common prefixes
        if !self.common_prefixes.is_empty() {
            lines.push(format!(
                "- Common prefixes: {}",
                self.common_prefixes.join(", ")
            ));
        }

        // Semantic tags
        if !self.semantic_tags.is_empty() {
            let tag_summary: Vec<String> = self
                .semantic_tags
                .iter()
                .map(|t| format!("{} ({:.0}%)", t.tag, t.coverage * 100.0))
                .collect();
            lines.push(format!("- Semantic categories: {}", tag_summary.join(", ")));
        }

        // Content previews (summarized)
        if !self.content_previews.is_empty() {
            lines.push("- Sample files:".to_string());
            for preview in self.content_previews.iter().take(3) {
                let preview_short: String = preview
                    .preview
                    .chars()
                    .take(50)
                    .collect::<String>()
                    .replace('\n', " ");
                lines.push(format!("  - {}: \"{}...\"", preview.path, preview_short));
            }
        }

        lines.join("\n")
    }
}

/// Get MIME category from file extension
fn get_mime_category(ext: &str) -> String {
    match ext {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" | "bmp" | "tiff" | "svg" | "ico" => {
            "image"
        }
        "pdf" | "doc" | "docx" | "txt" | "md" | "rtf" | "odt" | "pages" => "document",
        "xls" | "xlsx" | "csv" | "numbers" | "ods" => "spreadsheet",
        "mp4" | "mov" | "avi" | "mkv" | "wmv" | "webm" | "m4v" | "flv" => "video",
        "mp3" | "wav" | "aac" | "flac" | "ogg" | "m4a" | "wma" => "audio",
        "zip" | "tar" | "gz" | "rar" | "7z" | "bz2" | "xz" => "archive",
        "js" | "ts" | "py" | "rs" | "go" | "java" | "c" | "cpp" | "swift" | "rb" | "php" => "code",
        "json" | "yaml" | "yml" | "toml" | "xml" | "ini" | "conf" | "env" => "config",
        "dmg" | "pkg" | "exe" | "msi" | "deb" | "rpm" | "app" => "installer",
        "html" | "css" | "htm" | "scss" | "less" => "web",
        "ppt" | "pptx" | "key" | "odp" => "presentation",
        _ => "other",
    }
    .to_string()
}

/// Extract prefix from filename (before first digit, underscore, dash, or dot)
fn extract_prefix(filename: &str) -> Option<String> {
    let chars: Vec<char> = filename.chars().collect();
    let mut prefix_end = 0;

    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_digit() || *c == '_' || *c == '-' || *c == '.' {
            prefix_end = i;
            break;
        }
        prefix_end = i + 1;
    }

    // Only return prefixes between 3-20 characters
    if prefix_end >= 3 && prefix_end <= 20 {
        Some(chars[..prefix_end].iter().collect())
    } else {
        None
    }
}

/// Check if file extension is previewable (text-based)
fn is_previewable(ext: &str) -> bool {
    matches!(
        ext,
        "txt" | "md"
            | "json"
            | "yaml"
            | "yml"
            | "toml"
            | "js"
            | "ts"
            | "py"
            | "rs"
            | "go"
            | "java"
            | "c"
            | "cpp"
            | "html"
            | "css"
            | "xml"
            | "csv"
            | "log"
            | "ini"
            | "conf"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "swift"
            | "rb"
            | "php"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_mime_category() {
        assert_eq!(get_mime_category("jpg"), "image");
        assert_eq!(get_mime_category("pdf"), "document");
        assert_eq!(get_mime_category("rs"), "code");
        assert_eq!(get_mime_category("unknown"), "other");
    }

    #[test]
    fn test_extract_prefix() {
        assert_eq!(extract_prefix("IMG_1234.jpg"), Some("IMG".to_string()));
        assert_eq!(
            extract_prefix("Screenshot_2024.png"),
            Some("Screenshot".to_string())
        );
        assert_eq!(extract_prefix("a.txt"), None); // Too short
        assert_eq!(extract_prefix("12345.txt"), None); // Starts with digit
    }

    #[test]
    fn test_is_previewable() {
        assert!(is_previewable("txt"));
        assert!(is_previewable("rs"));
        assert!(is_previewable("json"));
        assert!(!is_previewable("jpg"));
        assert!(!is_previewable("pdf"));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500B");
        assert_eq!(format_size(1024), "1KB");
        assert_eq!(format_size(1024 * 1024), "1.0MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0GB");
    }

    #[test]
    fn test_digest_generator_default() {
        let gen = DigestGenerator::new();
        assert_eq!(gen.prefix_threshold, 10);
        assert_eq!(gen.min_prefix_count, 5);
    }
}
