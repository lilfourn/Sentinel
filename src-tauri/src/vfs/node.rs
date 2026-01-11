//! VFS Node definitions
//!
//! Represents individual nodes in the virtual filesystem tree.
//! Each node can be a file, directory, or symlink with associated metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Type of node in the virtual filesystem
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VFSNodeType {
    /// Regular file
    #[default]
    File,
    /// Directory containing other nodes
    Directory,
    /// Symbolic link to another path
    Symlink,
}

/// Represents a single node in the virtual filesystem.
///
/// FileNode captures both filesystem metadata and application-specific
/// information like content previews and semantic tags for AI processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    /// Absolute path to the file/directory
    pub path: PathBuf,

    /// Name of the file/directory (basename)
    pub name: String,

    /// Type of this node (file, directory, or symlink)
    pub node_type: VFSNodeType,

    /// Size in bytes (0 for directories)
    pub size: u64,

    /// Last modification timestamp
    pub modified_at: Option<DateTime<Utc>>,

    /// Creation timestamp
    pub created_at: Option<DateTime<Utc>>,

    /// File extension without the leading dot (None for directories)
    pub extension: Option<String>,

    /// MIME type guessed from extension
    pub mime_type: Option<String>,

    /// Content preview (first ~1KB for text files)
    /// Used by AI for semantic understanding
    pub content_preview: Option<String>,

    /// Semantic tags derived from content analysis
    /// Examples: ["invoice", "2024", "acme-corp"]
    pub vector_tags: Vec<String>,

    /// Parent directory path (None for root)
    pub parent: Option<PathBuf>,

    /// Child paths (only populated for directories)
    pub children: Vec<PathBuf>,

    /// Whether this node has staged (uncommitted) changes
    pub is_staged: bool,

    /// Original path before any staged move operation
    /// Used to track source for move operations
    pub original_path: Option<PathBuf>,

    /// Whether this file/directory is hidden
    pub is_hidden: bool,
}

impl FileNode {
    /// Create a new file node with minimal required fields
    pub fn new(path: PathBuf, node_type: VFSNodeType) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        let extension = if node_type == VFSNodeType::File {
            path.extension().map(|e| e.to_string_lossy().to_string())
        } else {
            None
        };

        let is_hidden = name.starts_with('.');

        Self {
            path,
            name,
            node_type,
            size: 0,
            modified_at: None,
            created_at: None,
            extension,
            mime_type: None,
            content_preview: None,
            vector_tags: Vec::new(),
            parent: None,
            children: Vec::new(),
            is_staged: false,
            original_path: None,
            is_hidden,
        }
    }

    /// Create a directory node
    pub fn directory(path: PathBuf) -> Self {
        Self::new(path, VFSNodeType::Directory)
    }

    /// Create a file node
    pub fn file(path: PathBuf) -> Self {
        Self::new(path, VFSNodeType::File)
    }

    /// Create a symlink node
    pub fn symlink(path: PathBuf) -> Self {
        Self::new(path, VFSNodeType::Symlink)
    }

    /// Check if this node is a directory
    pub fn is_directory(&self) -> bool {
        self.node_type == VFSNodeType::Directory
    }

    /// Check if this node is a file
    pub fn is_file(&self) -> bool {
        self.node_type == VFSNodeType::File
    }

    /// Check if this node is a symlink
    pub fn is_symlink(&self) -> bool {
        self.node_type == VFSNodeType::Symlink
    }

    /// Set the parent path
    pub fn with_parent(mut self, parent: PathBuf) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Set size in bytes
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = size;
        self
    }

    /// Set modification timestamp
    pub fn with_modified_at(mut self, modified_at: DateTime<Utc>) -> Self {
        self.modified_at = Some(modified_at);
        self
    }

    /// Set creation timestamp
    pub fn with_created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

    /// Set content preview
    pub fn with_content_preview(mut self, preview: String) -> Self {
        self.content_preview = Some(preview);
        self
    }

    /// Set MIME type
    pub fn with_mime_type(mut self, mime_type: String) -> Self {
        self.mime_type = Some(mime_type);
        self
    }

    /// Add vector tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.vector_tags = tags;
        self
    }

    /// Mark as staged with original path
    pub fn mark_staged(&mut self, original: PathBuf) {
        self.is_staged = true;
        self.original_path = Some(original);
    }

    /// Clear staged status
    pub fn clear_staged(&mut self) {
        self.is_staged = false;
        self.original_path = None;
    }

    /// Add a child path (for directories)
    pub fn add_child(&mut self, child_path: PathBuf) {
        if !self.children.contains(&child_path) {
            self.children.push(child_path);
        }
    }

    /// Remove a child path (for directories)
    pub fn remove_child(&mut self, child_path: &PathBuf) {
        self.children.retain(|p| p != child_path);
    }

    /// Check if content preview contains a search query (case-insensitive)
    pub fn content_contains(&self, query: &str) -> bool {
        self.content_preview
            .as_ref()
            .map(|c| c.to_lowercase().contains(&query.to_lowercase()))
            .unwrap_or(false)
    }

    /// Check if name contains a search query (case-insensitive)
    pub fn name_contains(&self, query: &str) -> bool {
        self.name.to_lowercase().contains(&query.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_node_creation() {
        let node = FileNode::file(PathBuf::from("/home/user/document.txt"));
        assert_eq!(node.name, "document.txt");
        assert_eq!(node.extension, Some("txt".to_string()));
        assert!(node.is_file());
        assert!(!node.is_directory());
    }

    #[test]
    fn test_directory_node_creation() {
        let node = FileNode::directory(PathBuf::from("/home/user/Documents"));
        assert_eq!(node.name, "Documents");
        assert!(node.extension.is_none());
        assert!(node.is_directory());
        assert!(!node.is_file());
    }

    #[test]
    fn test_hidden_file_detection() {
        let hidden = FileNode::file(PathBuf::from("/home/user/.config"));
        assert!(hidden.is_hidden);

        let visible = FileNode::file(PathBuf::from("/home/user/config"));
        assert!(!visible.is_hidden);
    }

    #[test]
    fn test_content_search() {
        let node = FileNode::file(PathBuf::from("/test.txt"))
            .with_content_preview("Hello World".to_string());

        assert!(node.content_contains("hello"));
        assert!(node.content_contains("WORLD"));
        assert!(!node.content_contains("goodbye"));
    }

    #[test]
    fn test_staged_operations() {
        let mut node = FileNode::file(PathBuf::from("/new/path.txt"));
        let original = PathBuf::from("/old/path.txt");

        node.mark_staged(original.clone());
        assert!(node.is_staged);
        assert_eq!(node.original_path, Some(original));

        node.clear_staged();
        assert!(!node.is_staged);
        assert!(node.original_path.is_none());
    }
}
