//! V4 Sampling module for Map-Reduce file organization.
//!
//! This module provides stratified sampling to handle large folders (5,000+ files)
//! without context saturation. Instead of sending the full file tree to Claude,
//! we generate a statistical digest with representative samples.
//!
//! ## Algorithm
//!
//! 1. **Group by Extension**: Files are bucketed by their extension
//! 2. **Stratified Sampling**: From each bucket, select:
//!    - Head (oldest file)
//!    - Tail (newest file)
//!    - Median (middle by date)
//!    - 2 Random samples
//! 3. **Statistics**: Extension counts, size totals, date ranges
//!
//! This reduces O(N) complexity to O(1) constant context size.

use crate::ai::rules::VirtualFile;
use serde::Serialize;
use std::collections::HashMap;

/// Threshold for switching between full tree and sampling mode
pub const SAMPLING_THRESHOLD: usize = 300;

/// Target coverage percentage before stopping iteration
pub const TARGET_COVERAGE: f64 = 0.95;

/// Maximum samples per extension group
const MAX_SAMPLES_PER_GROUP: usize = 5;

/// Total maximum samples to include in digest
const MAX_TOTAL_SAMPLES: usize = 60;

/// Statistical digest of a folder for AI context
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSample {
    /// Total number of files in the folder
    pub total_files: usize,
    /// Number of files already organized (covered by rules)
    pub organized_files: usize,
    /// Number of remaining unorganized files
    pub unorganized_files: usize,
    /// Extension statistics
    pub extensions: HashMap<String, ExtensionStats>,
    /// Representative file samples (stratified)
    pub samples: Vec<SampleFile>,
    /// Date range of files (oldest, newest) as ISO strings
    pub date_range: Option<(String, String)>,
    /// Total size in MB
    pub total_size_mb: f64,
}

/// Statistics for a file extension group
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStats {
    /// Number of files with this extension
    pub count: usize,
    /// Total size in MB
    pub size_mb: f64,
    /// Percentage of total files
    pub percentage: f64,
}

/// A sampled file for AI context (simplified from VirtualFile)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleFile {
    /// File name (without path)
    pub name: String,
    /// File extension
    pub ext: Option<String>,
    /// File size in bytes
    pub size: u64,
    /// Formatted size string
    pub size_formatted: String,
    /// Modified date as ISO string
    pub modified_at: Option<String>,
    /// Why this file was selected (head, tail, median, random)
    pub sample_reason: String,
}

impl FolderSample {
    /// Generate a prompt-friendly text representation
    pub fn to_prompt_text(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!(
            "## Folder Statistics\n- Total files: {}\n- Organized: {} ({:.1}%)\n- Remaining: {}\n- Total size: {:.1} MB",
            self.total_files,
            self.organized_files,
            if self.total_files > 0 { (self.organized_files as f64 / self.total_files as f64) * 100.0 } else { 0.0 },
            self.unorganized_files,
            self.total_size_mb
        ));

        if let Some((oldest, newest)) = &self.date_range {
            lines.push(format!("- Date range: {} to {}", oldest, newest));
        }

        lines.push("\n## Extension Breakdown".to_string());

        // Sort extensions by count descending
        let mut ext_list: Vec<_> = self.extensions.iter().collect();
        ext_list.sort_by(|a, b| b.1.count.cmp(&a.1.count));

        for (ext, stats) in ext_list.iter().take(15) {
            lines.push(format!(
                "- .{}: {} files ({:.1}%, {:.1} MB)",
                ext, stats.count, stats.percentage, stats.size_mb
            ));
        }

        if self.extensions.len() > 15 {
            lines.push(format!("- ... and {} more extensions", self.extensions.len() - 15));
        }

        lines.push("\n## Sample Files (Representative)".to_string());

        for sample in &self.samples {
            let ext_str = sample.ext.as_deref().unwrap_or("no_ext");
            let date_str = sample.modified_at.as_deref().unwrap_or("unknown");
            lines.push(format!(
                "- {} (.{}, {}, {}) [{}]",
                sample.name, ext_str, sample.size_formatted, date_str, sample.sample_reason
            ));
        }

        lines.join("\n")
    }
}

/// Generate a statistical digest with stratified sampling
///
/// # Arguments
/// * `files` - All files to sample from
/// * `organized_count` - Number of files already organized
///
/// # Returns
/// A FolderSample with statistics and representative files
pub fn generate_sample(files: &[VirtualFile], organized_count: usize) -> FolderSample {
    let total_files = files.len();
    let unorganized_files = total_files.saturating_sub(organized_count);

    // Group files by extension
    let mut by_ext: HashMap<String, Vec<&VirtualFile>> = HashMap::new();
    let mut ext_stats: HashMap<String, ExtensionStats> = HashMap::new();
    let mut total_size: u64 = 0;
    let mut oldest_date: Option<i64> = None;
    let mut newest_date: Option<i64> = None;

    for file in files {
        let ext = file.ext.clone().unwrap_or_else(|| "no_ext".to_string());
        by_ext.entry(ext.clone()).or_default().push(file);

        total_size += file.size;

        // Track date range
        if let Some(modified) = file.modified_at {
            oldest_date = Some(oldest_date.map_or(modified, |o| o.min(modified)));
            newest_date = Some(newest_date.map_or(modified, |n| n.max(modified)));
        }

        // Update stats
        let stat = ext_stats.entry(ext).or_insert(ExtensionStats {
            count: 0,
            size_mb: 0.0,
            percentage: 0.0,
        });
        stat.count += 1;
        stat.size_mb += file.size as f64 / 1_048_576.0;
    }

    // Calculate percentages
    for stat in ext_stats.values_mut() {
        stat.percentage = if total_files > 0 {
            (stat.count as f64 / total_files as f64) * 100.0
        } else {
            0.0
        };
    }

    // Stratified sampling
    let mut samples = Vec::new();
    let mut samples_per_ext = MAX_TOTAL_SAMPLES / by_ext.len().max(1);
    samples_per_ext = samples_per_ext.min(MAX_SAMPLES_PER_GROUP).max(1);

    for (_ext, mut group) in by_ext {
        if group.is_empty() {
            continue;
        }

        // Sort by modification date
        group.sort_by_key(|f| f.modified_at.unwrap_or(0));

        let group_len = group.len();

        if group_len <= samples_per_ext {
            // If few files, take all
            for file in group {
                samples.push(file_to_sample(file, "all"));
            }
        } else {
            // Stratified sampling: Head, Tail, Median, Random

            // Head (oldest)
            samples.push(file_to_sample(group[0], "oldest"));

            // Tail (newest)
            samples.push(file_to_sample(group[group_len - 1], "newest"));

            // Median
            let median_idx = group_len / 2;
            if median_idx != 0 && median_idx != group_len - 1 {
                samples.push(file_to_sample(group[median_idx], "median"));
            }

            // Additional samples if budget allows
            if samples_per_ext > 3 {
                // Quarter points
                let q1 = group_len / 4;
                let q3 = (group_len * 3) / 4;

                if q1 > 0 && q1 != median_idx {
                    samples.push(file_to_sample(group[q1], "q1"));
                }
                if q3 < group_len - 1 && q3 != median_idx {
                    samples.push(file_to_sample(group[q3], "q3"));
                }
            }
        }

        // Limit total samples
        if samples.len() >= MAX_TOTAL_SAMPLES {
            break;
        }
    }

    // Truncate if over limit
    samples.truncate(MAX_TOTAL_SAMPLES);

    // Format date range
    let date_range = match (oldest_date, newest_date) {
        (Some(oldest), Some(newest)) => {
            let oldest_str = chrono::DateTime::from_timestamp_millis(oldest)
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let newest_str = chrono::DateTime::from_timestamp_millis(newest)
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            Some((oldest_str, newest_str))
        }
        _ => None,
    };

    FolderSample {
        total_files,
        organized_files: organized_count,
        unorganized_files,
        extensions: ext_stats,
        samples,
        date_range,
        total_size_mb: total_size as f64 / 1_048_576.0,
    }
}

/// Generate a sample from only unmatched (unorganized) files
///
/// This is used in the "janitor pass" to handle files that didn't match
/// any rules from previous iterations.
pub fn generate_unmatched_sample(
    all_files: &[VirtualFile],
    matched_paths: &std::collections::HashSet<String>,
) -> FolderSample {
    let unmatched: Vec<VirtualFile> = all_files
        .iter()
        .filter(|f| !matched_paths.contains(&f.path))
        .cloned()
        .collect();

    let organized_count = matched_paths.len();
    generate_sample(&unmatched, organized_count)
}

/// Convert a VirtualFile to a SampleFile
fn file_to_sample(file: &VirtualFile, reason: &str) -> SampleFile {
    let size_formatted = format_size(file.size);
    let modified_at = file.modified_at.and_then(|ts| {
        chrono::DateTime::from_timestamp_millis(ts)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
    });

    SampleFile {
        name: file.name.clone(),
        ext: file.ext.clone(),
        size: file.size,
        size_formatted,
        modified_at,
        sample_reason: reason.to_string(),
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

/// Check if sampling mode should be used
pub fn should_use_sampling(file_count: usize) -> bool {
    file_count > SAMPLING_THRESHOLD
}

/// Calculate coverage percentage
pub fn calculate_coverage(total_files: usize, matched_files: usize) -> f64 {
    if total_files == 0 {
        return 1.0;
    }
    matched_files as f64 / total_files as f64
}

/// Check if coverage target is reached
pub fn coverage_target_reached(total_files: usize, matched_files: usize) -> bool {
    calculate_coverage(total_files, matched_files) >= TARGET_COVERAGE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_file(name: &str, ext: &str, size: u64, modified: i64) -> VirtualFile {
        VirtualFile {
            path: format!("/test/{}.{}", name, ext),
            name: format!("{}.{}", name, ext),
            ext: Some(ext.to_string()),
            size,
            is_directory: false,
            is_hidden: false,
            modified_at: Some(modified),
            created_at: Some(modified),
            mime_type: None,
        }
    }

    #[test]
    fn test_should_use_sampling() {
        assert!(!should_use_sampling(100));
        assert!(!should_use_sampling(300));
        assert!(should_use_sampling(301));
        assert!(should_use_sampling(5000));
    }

    #[test]
    fn test_coverage_calculation() {
        assert_eq!(calculate_coverage(100, 95), 0.95);
        assert_eq!(calculate_coverage(100, 100), 1.0);
        assert_eq!(calculate_coverage(0, 0), 1.0);
    }

    #[test]
    fn test_coverage_target() {
        assert!(coverage_target_reached(100, 95));
        assert!(coverage_target_reached(100, 100));
        assert!(!coverage_target_reached(100, 94));
    }

    #[test]
    fn test_generate_sample_small() {
        let files = vec![
            create_test_file("doc1", "pdf", 1000, 1000),
            create_test_file("doc2", "pdf", 2000, 2000),
            create_test_file("img1", "jpg", 3000, 3000),
        ];

        let sample = generate_sample(&files, 0);

        assert_eq!(sample.total_files, 3);
        assert_eq!(sample.organized_files, 0);
        assert_eq!(sample.unorganized_files, 3);
        assert_eq!(sample.extensions.len(), 2);
        assert_eq!(sample.extensions.get("pdf").unwrap().count, 2);
    }

    #[test]
    fn test_generate_sample_large() {
        // Create 100 files across multiple extensions
        let mut files = Vec::new();
        for i in 0..40 {
            files.push(create_test_file(&format!("doc{}", i), "pdf", 1000, i as i64 * 1000));
        }
        for i in 0..30 {
            files.push(create_test_file(&format!("img{}", i), "jpg", 2000, i as i64 * 1000));
        }
        for i in 0..30 {
            files.push(create_test_file(&format!("data{}", i), "csv", 500, i as i64 * 1000));
        }

        let sample = generate_sample(&files, 10);

        assert_eq!(sample.total_files, 100);
        assert_eq!(sample.organized_files, 10);
        assert_eq!(sample.unorganized_files, 90);

        // Should have samples from each extension
        assert!(sample.samples.len() <= MAX_TOTAL_SAMPLES);
        assert!(sample.samples.iter().any(|s| s.ext.as_deref() == Some("pdf")));
        assert!(sample.samples.iter().any(|s| s.ext.as_deref() == Some("jpg")));
        assert!(sample.samples.iter().any(|s| s.ext.as_deref() == Some("csv")));
    }

    #[test]
    fn test_to_prompt_text() {
        let files = vec![
            create_test_file("doc1", "pdf", 1024 * 1024, 1609459200000), // 1MB, 2021-01-01
            create_test_file("doc2", "pdf", 2 * 1024 * 1024, 1640995200000), // 2MB, 2022-01-01
        ];

        let sample = generate_sample(&files, 0);
        let text = sample.to_prompt_text();

        assert!(text.contains("Total files: 2"));
        assert!(text.contains(".pdf: 2 files"));
        assert!(text.contains("Sample Files"));
    }
}
