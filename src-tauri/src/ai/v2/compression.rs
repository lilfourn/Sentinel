//! V5 Adaptive Pattern Folding (APF) Compression module.
//!
//! This module compresses file lists by detecting sequential naming patterns
//! and "folding" them into single-line representations.
//!
//! ## Example
//!
//! **Before**: 5000 individual files
//! ```text
//! IMG_0001.jpg, IMG_0002.jpg, ... IMG_5000.jpg
//! ```
//!
//! **After APF**: ~100 tokens
//! ```text
//! PATTERNS:
//! - IMG_[0001..5000].jpg (5000 files, avg 2.2MB)
//! ```
//!
//! ## Algorithm
//!
//! 1. **Skeleton Extraction**: `IMG_0001.jpg` → skeleton `IMG_{NUM}.jpg`
//! 2. **Clustering**: Group files by identical skeleton
//! 3. **Range Detection**: Sort by number, extract min/max
//! 4. **Threshold**: Groups with < 3 files → outliers (not a pattern)

use crate::ai::rules::VirtualFile;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;

/// Minimum files in a group to be considered a pattern (not outlier)
const MIN_PATTERN_SIZE: usize = 3;

/// Compressed representation of a folder for AI context
#[derive(Serialize, Clone, Debug)]
pub struct FolderHologram {
    /// Detected sequential patterns: "IMG_[0001..5000].jpg (5000 files)"
    pub patterns: Vec<PatternGroup>,
    /// Files that don't fit any pattern
    pub outliers: Vec<OutlierFile>,
    /// Summary statistics
    pub stats: HologramStats,
}

/// A detected sequential file pattern
#[derive(Serialize, Clone, Debug)]
pub struct PatternGroup {
    /// Template with range: "IMG_[0001..5000].jpg"
    pub template: String,
    /// Number of files in this pattern
    pub count: usize,
    /// First number in the sequence
    pub range_start: String,
    /// Last number in the sequence
    pub range_end: String,
    /// Average file size formatted
    pub avg_size: String,
    /// Total size in MB
    pub total_size_mb: f64,
    /// File extension
    pub extension: String,
    /// The regex pattern that matches these files
    pub regex_pattern: String,
}

/// A file that doesn't fit any pattern
#[derive(Serialize, Clone, Debug)]
pub struct OutlierFile {
    /// File name
    pub name: String,
    /// Formatted file size
    pub size: String,
    /// File extension
    pub extension: Option<String>,
}

/// Summary statistics for the hologram
#[derive(Serialize, Clone, Debug)]
pub struct HologramStats {
    /// Total number of files analyzed
    pub total_files: usize,
    /// Percentage of files covered by patterns (0.0 - 1.0)
    pub pattern_coverage: f64,
    /// Number of detected patterns
    pub pattern_count: usize,
    /// Number of outlier files
    pub outlier_count: usize,
    /// Total size in MB
    pub total_size_mb: f64,
}

/// Internal structure for tracking file clusters
#[derive(Debug)]
struct FileCluster {
    /// The skeleton pattern (e.g., "IMG_{NUM}.jpg")
    skeleton: String,
    /// Files in this cluster with their extracted numbers
    files: Vec<(VirtualFile, String)>, // (file, extracted_number)
    /// Total size of all files in cluster
    total_size: u64,
    /// The extension for this cluster
    extension: String,
}

/// Generate a compressed "hologram" from a file list
///
/// This function analyzes the file list and generates a compressed
/// representation that captures sequential patterns.
///
/// # Arguments
/// * `files` - Slice of VirtualFile to analyze
///
/// # Returns
/// A FolderHologram with patterns and outliers
pub fn generate_hologram(files: &[VirtualFile]) -> FolderHologram {
    // Skip directories, only process files
    let file_only: Vec<&VirtualFile> = files.iter().filter(|f| !f.is_directory).collect();

    if file_only.is_empty() {
        return FolderHologram {
            patterns: Vec::new(),
            outliers: Vec::new(),
            stats: HologramStats {
                total_files: 0,
                pattern_coverage: 1.0,
                pattern_count: 0,
                outlier_count: 0,
                total_size_mb: 0.0,
            },
        };
    }

    // Regex to find numbers in filenames
    let number_regex = Regex::new(r"\d+").expect("Invalid regex");

    // Step 1: Cluster files by skeleton
    let mut clusters: HashMap<String, FileCluster> = HashMap::new();

    for file in &file_only {
        let (skeleton, number) = extract_skeleton(&file.name, &number_regex);
        let ext = file.ext.clone().unwrap_or_default();
        let cluster_key = format!("{}:{}", skeleton, ext);

        let cluster = clusters.entry(cluster_key.clone()).or_insert_with(|| FileCluster {
            skeleton: skeleton.clone(),
            files: Vec::new(),
            total_size: 0,
            extension: ext.clone(),
        });

        cluster.files.push(((*file).clone(), number));
        cluster.total_size += file.size;
    }

    // Step 2: Separate patterns from outliers
    let mut patterns: Vec<PatternGroup> = Vec::new();
    let mut outliers: Vec<OutlierFile> = Vec::new();
    let mut pattern_file_count = 0;
    let mut total_size: u64 = 0;

    for (_key, cluster) in clusters {
        total_size += cluster.total_size;

        if cluster.files.len() >= MIN_PATTERN_SIZE && has_numeric_pattern(&cluster) {
            // This is a pattern - fold it
            let pattern = fold_cluster(cluster);
            pattern_file_count += pattern.count;
            patterns.push(pattern);
        } else {
            // These are outliers
            for (file, _) in cluster.files {
                outliers.push(OutlierFile {
                    name: file.name.clone(),
                    size: format_size(file.size),
                    extension: file.ext.clone(),
                });
            }
        }
    }

    // Sort patterns by count (most files first)
    patterns.sort_by(|a, b| b.count.cmp(&a.count));

    // Limit outliers shown (keep first 50)
    let outlier_count = outliers.len();
    outliers.truncate(50);

    let total_files = file_only.len();
    let pattern_coverage = if total_files > 0 {
        pattern_file_count as f64 / total_files as f64
    } else {
        1.0
    };

    FolderHologram {
        patterns: patterns.clone(),
        outliers,
        stats: HologramStats {
            total_files,
            pattern_coverage,
            pattern_count: patterns.len(),
            outlier_count,
            total_size_mb: total_size as f64 / 1_048_576.0,
        },
    }
}

/// Extract skeleton and number from a filename
///
/// Example: "IMG_0001.jpg" → ("IMG_{NUM}", "0001")
fn extract_skeleton(name: &str, regex: &Regex) -> (String, String) {
    let mut skeleton = name.to_string();
    let mut last_number = String::new();

    // Find all numbers and replace with {NUM}
    // Keep track of the last number found (usually the sequence number)
    for mat in regex.find_iter(name) {
        last_number = mat.as_str().to_string();
    }

    // Replace the last number with {NUM} placeholder
    if !last_number.is_empty() {
        // Find the last occurrence and replace only that
        if let Some(pos) = skeleton.rfind(&last_number) {
            skeleton = format!(
                "{}{{NUM}}{}",
                &skeleton[..pos],
                &skeleton[pos + last_number.len()..]
            );
        }
    }

    (skeleton, last_number)
}

/// Check if a cluster has a valid numeric pattern
fn has_numeric_pattern(cluster: &FileCluster) -> bool {
    // Must have at least some files with numeric components
    let files_with_numbers = cluster.files.iter().filter(|(_, num)| !num.is_empty()).count();
    files_with_numbers >= MIN_PATTERN_SIZE
}

/// Fold a cluster into a pattern group
fn fold_cluster(cluster: FileCluster) -> PatternGroup {
    let mut files_with_nums: Vec<(VirtualFile, u64, String)> = cluster
        .files
        .into_iter()
        .filter_map(|(file, num_str)| {
            if num_str.is_empty() {
                None
            } else {
                num_str.parse::<u64>().ok().map(|n| (file, n, num_str))
            }
        })
        .collect();

    // Sort by the numeric value
    files_with_nums.sort_by_key(|(_, num, _)| *num);

    let count = files_with_nums.len();
    let (range_start, range_end) = if let (Some(first), Some(last)) =
        (files_with_nums.first(), files_with_nums.last())
    {
        (first.2.clone(), last.2.clone())
    } else {
        ("0".to_string(), "0".to_string())
    };

    // Calculate average size
    let total_size: u64 = files_with_nums.iter().map(|(f, _, _)| f.size).sum();
    let avg_size = if count > 0 {
        total_size / count as u64
    } else {
        0
    };

    // Build template string
    let template = cluster
        .skeleton
        .replace("{NUM}", &format!("[{}..{}]", range_start, range_end));

    // Build regex pattern for this skeleton
    let regex_pattern = cluster.skeleton.replace("{NUM}", r"\d+");

    PatternGroup {
        template,
        count,
        range_start,
        range_end,
        avg_size: format_size(avg_size),
        total_size_mb: total_size as f64 / 1_048_576.0,
        extension: cluster.extension,
        regex_pattern,
    }
}

/// Verify that a regex pattern matches files in the VFS
///
/// # Arguments
/// * `pattern` - Regex pattern string
/// * `files` - Files to test against
///
/// # Returns
/// List of matching file paths
pub fn verify_pattern_matches(pattern: &str, files: &[VirtualFile]) -> Vec<String> {
    match Regex::new(pattern) {
        Ok(regex) => files
            .iter()
            .filter(|f| !f.is_directory && regex.is_match(&f.name))
            .map(|f| f.path.clone())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Check if hologram compression should be used
///
/// Returns true if:
/// 1. More than threshold files
/// 2. Pattern coverage would be > 50%
pub fn should_use_hologram(files: &[VirtualFile], threshold: usize) -> bool {
    if files.len() <= threshold {
        return false;
    }

    // Quick check: generate hologram and check coverage
    let hologram = generate_hologram(files);
    hologram.stats.pattern_coverage > 0.5
}

impl FolderHologram {
    /// Generate prompt-friendly text representation
    pub fn to_prompt_text(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!(
            "## Folder Hologram\nTotal: {} files | Patterns: {} | Outliers: {} | Coverage: {:.1}% | Size: {:.1}MB",
            self.stats.total_files,
            self.stats.pattern_count,
            self.stats.outlier_count,
            self.stats.pattern_coverage * 100.0,
            self.stats.total_size_mb
        ));

        // Patterns section
        if !self.patterns.is_empty() {
            lines.push(format!("\n### Detected Patterns ({})", self.patterns.len()));
            for pattern in &self.patterns {
                lines.push(format!(
                    "- {} ({} files, avg {}, total {:.1}MB)",
                    pattern.template, pattern.count, pattern.avg_size, pattern.total_size_mb
                ));
            }
        }

        // Outliers section
        if !self.outliers.is_empty() {
            let shown = self.outliers.len();
            let total = self.stats.outlier_count;
            let header = if shown < total {
                format!("\n### Outliers (showing {} of {})", shown, total)
            } else {
                format!("\n### Outliers ({})", total)
            };
            lines.push(header);

            for outlier in &self.outliers {
                let ext = outlier.extension.as_deref().unwrap_or("no_ext");
                lines.push(format!("- {} (.{}, {})", outlier.name, ext, outlier.size));
            }

            if shown < total {
                lines.push(format!("... +{} more outliers", total - shown));
            }
        }

        lines.join("\n")
    }

    /// Get a sample of files from a specific pattern for inspection
    pub fn sample_pattern_files<'a>(&self, pattern_index: usize, files: &'a [VirtualFile]) -> Vec<&'a VirtualFile> {
        if pattern_index >= self.patterns.len() {
            return Vec::new();
        }

        let pattern = &self.patterns[pattern_index];

        // Match files using the pattern's regex
        let regex = match Regex::new(&pattern.regex_pattern) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let matching: Vec<&'a VirtualFile> = files
            .iter()
            .filter(|f| !f.is_directory && regex.is_match(&f.name))
            .collect();

        // Return first, middle, last as samples
        if matching.is_empty() {
            return Vec::new();
        }

        let len = matching.len();
        let mut samples = Vec::new();

        samples.push(matching[0]); // First
        if len > 2 {
            samples.push(matching[len / 2]); // Middle
        }
        if len > 1 {
            samples.push(matching[len - 1]); // Last
        }

        samples
    }
}

/// Format file size for display
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_file(name: &str, ext: &str, size: u64) -> VirtualFile {
        VirtualFile {
            path: format!("/test/{}.{}", name, ext),
            name: format!("{}.{}", name, ext),
            ext: Some(ext.to_string()),
            size,
            is_directory: false,
            is_hidden: false,
            modified_at: Some(0),
            created_at: Some(0),
            mime_type: None,
        }
    }

    #[test]
    fn test_extract_skeleton() {
        let regex = Regex::new(r"\d+").unwrap();

        let (skeleton, num) = extract_skeleton("IMG_0001.jpg", &regex);
        assert_eq!(skeleton, "IMG_{NUM}.jpg");
        assert_eq!(num, "0001");

        let (skeleton, num) = extract_skeleton("document_2024_final.pdf", &regex);
        assert_eq!(skeleton, "document_2024_{NUM}.pdf");
        assert_eq!(num, "final"); // "final" has no numbers, so empty
        // Actually "2024" and "final" - final has no nums

        let (skeleton, num) = extract_skeleton("screenshot_2024-12-30_001.png", &regex);
        assert_eq!(skeleton, "screenshot_2024-12-30_{NUM}.png");
        assert_eq!(num, "001");
    }

    #[test]
    fn test_generate_hologram_sequential() {
        // Create 100 sequential images
        let mut files: Vec<VirtualFile> = Vec::new();
        for i in 1..=100 {
            files.push(create_test_file(&format!("IMG_{:04}", i), "jpg", 1024 * 1024));
        }

        let hologram = generate_hologram(&files);

        assert_eq!(hologram.stats.total_files, 100);
        assert_eq!(hologram.patterns.len(), 1);
        assert_eq!(hologram.patterns[0].count, 100);
        assert_eq!(hologram.patterns[0].range_start, "0001");
        assert_eq!(hologram.patterns[0].range_end, "0100");
        assert!(hologram.stats.pattern_coverage >= 0.99);
    }

    #[test]
    fn test_generate_hologram_mixed() {
        let mut files: Vec<VirtualFile> = Vec::new();

        // Add 50 sequential images
        for i in 1..=50 {
            files.push(create_test_file(&format!("IMG_{:04}", i), "jpg", 1024 * 1024));
        }

        // Add 2 outliers (not enough for pattern)
        files.push(create_test_file("random_doc", "pdf", 500));
        files.push(create_test_file("notes", "txt", 200));

        let hologram = generate_hologram(&files);

        assert_eq!(hologram.stats.total_files, 52);
        assert_eq!(hologram.patterns.len(), 1);
        assert_eq!(hologram.stats.outlier_count, 2);
    }

    #[test]
    fn test_verify_pattern_matches() {
        let files = vec![
            create_test_file("IMG_0001", "jpg", 100),
            create_test_file("IMG_0002", "jpg", 100),
            create_test_file("document", "pdf", 100),
        ];

        let matches = verify_pattern_matches(r"IMG_\d+\.jpg", &files);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_to_prompt_text() {
        let mut files: Vec<VirtualFile> = Vec::new();
        for i in 1..=10 {
            files.push(create_test_file(&format!("IMG_{:04}", i), "jpg", 1024 * 1024));
        }

        let hologram = generate_hologram(&files);
        let text = hologram.to_prompt_text();

        assert!(text.contains("Folder Hologram"));
        assert!(text.contains("Detected Patterns"));
        assert!(text.contains("IMG_"));
        assert!(text.contains("10 files"));
    }

    #[test]
    fn test_should_use_hologram() {
        // Small file count - should not use hologram
        let small_files: Vec<VirtualFile> = (1..=50)
            .map(|i| create_test_file(&format!("IMG_{:04}", i), "jpg", 1024))
            .collect();
        assert!(!should_use_hologram(&small_files, 300));

        // Large file count with patterns - should use hologram
        let large_files: Vec<VirtualFile> = (1..=500)
            .map(|i| create_test_file(&format!("IMG_{:04}", i), "jpg", 1024))
            .collect();
        assert!(should_use_hologram(&large_files, 300));
    }
}
