//! VFS Scanner
//!
//! Provides parallel directory scanning using jwalk for high performance.
//! Populates a ShadowVFS with nodes from the real filesystem.

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

use super::graph::ShadowVFS;
use super::node::{FileNode, VFSNodeType};

/// Configuration for the VFS scanner
#[derive(Debug, Clone)]
pub struct JWalkScanner {
    /// Maximum depth to scan (0 = unlimited)
    max_depth: usize,

    /// Maximum bytes to read for content preview
    max_preview_size: usize,

    /// Number of threads to use for scanning
    num_threads: usize,

    /// Whether to extract content previews
    extract_previews: bool,

    /// File extensions to extract content from
    previewable_extensions: Vec<String>,
}

impl Default for JWalkScanner {
    fn default() -> Self {
        Self {
            max_depth: 0, // Unlimited
            max_preview_size: 1024,
            num_threads: get_num_cpus().min(4),
            extract_previews: true,
            previewable_extensions: vec![
                "txt".to_string(),
                "md".to_string(),
                "json".to_string(),
                "yaml".to_string(),
                "yml".to_string(),
                "xml".to_string(),
                "html".to_string(),
                "css".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "rs".to_string(),
                "py".to_string(),
                "rb".to_string(),
                "go".to_string(),
                "java".to_string(),
                "c".to_string(),
                "cpp".to_string(),
                "h".to_string(),
                "sh".to_string(),
                "toml".to_string(),
                "ini".to_string(),
                "cfg".to_string(),
                "conf".to_string(),
                "log".to_string(),
                "csv".to_string(),
            ],
        }
    }
}

/// Statistics from a scan operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    /// Total number of files scanned
    pub total_files: usize,

    /// Total number of directories scanned
    pub total_dirs: usize,

    /// Total size of all files in bytes
    pub total_size_bytes: u64,

    /// Time taken to scan in milliseconds
    pub scan_duration_ms: u64,

    /// Number of content previews extracted
    pub content_previews_extracted: usize,

    /// Number of files skipped due to errors
    pub errors: usize,
}

impl JWalkScanner {
    /// Create a new scanner with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum scan depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set maximum preview size in bytes
    pub fn with_max_preview_size(mut self, size: usize) -> Self {
        self.max_preview_size = size;
        self
    }

    /// Set number of threads
    pub fn with_num_threads(mut self, threads: usize) -> Self {
        self.num_threads = threads;
        self
    }

    /// Enable or disable content preview extraction
    pub fn with_extract_previews(mut self, extract: bool) -> Self {
        self.extract_previews = extract;
        self
    }

    /// Scan a directory and populate the VFS
    ///
    /// Uses jwalk for parallel directory traversal, significantly
    /// improving performance on large directories.
    pub async fn scan(&self, root: &PathBuf, vfs: &mut ShadowVFS) -> Result<ScanStats, String> {
        let start = Instant::now();
        let mut stats = ScanStats {
            total_files: 0,
            total_dirs: 0,
            total_size_bytes: 0,
            scan_duration_ms: 0,
            content_previews_extracted: 0,
            errors: 0,
        };

        // Validate root exists
        if !root.exists() {
            return Err(format!("Path does not exist: {}", root.display()));
        }

        if !root.is_dir() {
            return Err(format!("Path is not a directory: {}", root.display()));
        }

        // Configure jwalk
        let mut walker = jwalk::WalkDir::new(root)
            .parallelism(jwalk::Parallelism::RayonNewPool(self.num_threads))
            .skip_hidden(false)
            .follow_links(false);

        if self.max_depth > 0 {
            walker = walker.max_depth(self.max_depth);
        }

        // Collect entries - jwalk handles parallelism internally
        let entries: Vec<_> = walker.into_iter().collect();

        for entry_result in entries {
            match entry_result {
                Ok(entry) => {
                    let path = entry.path();

                    // Skip the root itself (it's already in the VFS)
                    if path == *root {
                        continue;
                    }

                    match self.create_node_from_entry(&entry, &mut stats) {
                        Ok(node) => {
                            // Update parent's children list
                            if let Some(parent_path) = &node.parent {
                                if let Some(parent) = vfs.get_mut(parent_path) {
                                    parent.add_child(node.path.clone());
                                }
                            }

                            vfs.insert(node);
                        }
                        Err(e) => {
                            eprintln!("[VFS Scanner] Error processing {}: {}", path.display(), e);
                            stats.errors += 1;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[VFS Scanner] Walk error: {}", e);
                    stats.errors += 1;
                }
            }
        }

        // Update VFS scan time
        vfs.set_last_scan(Utc::now());

        stats.scan_duration_ms = start.elapsed().as_millis() as u64;

        eprintln!(
            "[VFS Scanner] Scanned {} files, {} dirs in {}ms",
            stats.total_files, stats.total_dirs, stats.scan_duration_ms
        );

        Ok(stats)
    }

    /// Create a FileNode from a jwalk entry
    fn create_node_from_entry(
        &self,
        entry: &jwalk::DirEntry<((), ())>,
        stats: &mut ScanStats,
    ) -> Result<FileNode, String> {
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|e| format!("Failed to get metadata: {}", e))?;

        // Determine node type
        let node_type = if metadata.is_symlink() {
            VFSNodeType::Symlink
        } else if metadata.is_dir() {
            stats.total_dirs += 1;
            VFSNodeType::Directory
        } else {
            stats.total_files += 1;
            VFSNodeType::File
        };

        let mut node = FileNode::new(path.clone(), node_type.clone());

        // Set parent
        if let Some(parent) = path.parent() {
            node.parent = Some(parent.to_path_buf());
        }

        // Set size for files
        if node_type == VFSNodeType::File {
            node.size = metadata.len();
            stats.total_size_bytes += node.size;
        }

        // Set timestamps
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                node.modified_at = Some(Utc.timestamp_opt(duration.as_secs() as i64, 0).unwrap());
            }
        }

        if let Ok(created) = metadata.created() {
            if let Ok(duration) = created.duration_since(std::time::UNIX_EPOCH) {
                node.created_at = Some(Utc.timestamp_opt(duration.as_secs() as i64, 0).unwrap());
            }
        }

        // Set MIME type
        if let Some(ext) = &node.extension {
            if let Some(mime) = mime_guess::from_ext(ext).first() {
                node.mime_type = Some(mime.to_string());
            }
        }

        // Extract content preview for text files
        if self.extract_previews && node_type == VFSNodeType::File {
            if let Some(preview) = self.get_content_preview(&path, self.max_preview_size) {
                node.content_preview = Some(preview);
                stats.content_previews_extracted += 1;
            }
        }

        Ok(node)
    }

    /// Get a content preview from a file
    ///
    /// Returns the first `max_bytes` of a text file, or None if:
    /// - The file is not a previewable type
    /// - The file cannot be read
    /// - The content is not valid UTF-8
    pub fn get_content_preview(&self, path: &PathBuf, max_bytes: usize) -> Option<String> {
        // Check extension
        let ext = path.extension()?.to_string_lossy().to_lowercase();

        if !self.previewable_extensions.contains(&ext) {
            return None;
        }

        // Try to read the file
        let mut file = fs::File::open(path).ok()?;
        let mut buffer = vec![0u8; max_bytes];

        let bytes_read = file.read(&mut buffer).ok()?;
        buffer.truncate(bytes_read);

        // Convert to string, handling partial UTF-8
        match String::from_utf8(buffer) {
            Ok(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
            Err(e) => {
                // Try lossy conversion for files with some non-UTF8 bytes
                let bytes = e.into_bytes();
                let s = String::from_utf8_lossy(&bytes[..bytes_read]).trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            }
        }
    }

    /// Check if an extension is previewable
    pub fn is_previewable(&self, extension: &str) -> bool {
        self.previewable_extensions
            .contains(&extension.to_lowercase())
    }
}

/// Helper function to get CPU count
fn get_num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();

        // Create some test files
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let mut file1 = File::create(dir.path().join("test.txt")).unwrap();
        file1.write_all(b"Hello, World!").unwrap();

        let mut file2 = File::create(dir.path().join("subdir/nested.txt")).unwrap();
        file2.write_all(b"Nested content").unwrap();

        File::create(dir.path().join("binary.bin")).unwrap();

        dir
    }

    #[tokio::test]
    async fn test_scan_basic() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path().to_path_buf();

        let scanner = JWalkScanner::new();
        let mut vfs = ShadowVFS::new(root.clone());

        let stats = scanner.scan(&root, &mut vfs).await.unwrap();

        assert!(stats.total_files >= 3);
        assert!(stats.total_dirs >= 1);
        // Note: scan_duration_ms can be 0 on fast systems when scan completes in < 1ms
    }

    #[tokio::test]
    async fn test_content_preview() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path().to_path_buf();

        let scanner = JWalkScanner::new();
        let mut vfs = ShadowVFS::new(root.clone());

        scanner.scan(&root, &mut vfs).await.unwrap();

        // Find the test.txt file
        let test_file = vfs.get(&root.join("test.txt"));
        assert!(test_file.is_some());

        let node = test_file.unwrap();
        assert_eq!(node.content_preview, Some("Hello, World!".to_string()));
    }

    #[tokio::test]
    async fn test_max_depth() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path().to_path_buf();

        let scanner = JWalkScanner::new().with_max_depth(1);
        let mut vfs = ShadowVFS::new(root.clone());

        let stats = scanner.scan(&root, &mut vfs).await.unwrap();

        // Should only see immediate children, not nested
        assert!(vfs.get(&root.join("test.txt")).is_some());
        assert!(vfs.get(&root.join("subdir")).is_some());
        // Nested file should not be scanned
        assert!(vfs.get(&root.join("subdir/nested.txt")).is_none());

        // Only 2 files at depth 1 (test.txt and binary.bin)
        assert_eq!(stats.total_files, 2);
    }

    #[test]
    fn test_is_previewable() {
        let scanner = JWalkScanner::new();

        assert!(scanner.is_previewable("txt"));
        assert!(scanner.is_previewable("TXT"));
        assert!(scanner.is_previewable("md"));
        assert!(scanner.is_previewable("rs"));
        assert!(!scanner.is_previewable("exe"));
        assert!(!scanner.is_previewable("bin"));
        assert!(!scanner.is_previewable("png"));
    }
}
