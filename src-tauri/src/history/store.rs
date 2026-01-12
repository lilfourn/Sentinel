//! Persistence manager for organization history files.

use crate::history::checksum::hash_folder_path;
use crate::history::entry::{
    FolderHistory, FolderIndexEntry, HistoryIndex, HistorySession, HistorySummary,
    SessionSummary,
};
use chrono::Utc;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;

/// History file extension
const HISTORY_EXTENSION: &str = "history.json";

/// Index filename
const INDEX_FILENAME: &str = "index.json";

/// Protected system paths that should never have history operations
const PROTECTED_PATHS: &[&str] = &[
    "/",
    "/bin",
    "/sbin",
    "/usr",
    "/etc",
    "/var",
    "/System",
    "/Library",
    "/Applications",
    "/private",
    "/tmp",
    "/dev",
    "/proc",
    "/sys",
];

/// Validate a folder path for history operations
fn validate_folder_path(folder_path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(folder_path);

    // Check for path traversal attempts
    if folder_path.contains("..") {
        return Err("Path traversal not allowed".to_string());
    }

    // Resolve to canonical path (follows symlinks, resolves . and ..)
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Invalid path '{}': {}", folder_path, e))?;

    // Check against protected paths
    let canonical_str = canonical.to_string_lossy();
    for protected in PROTECTED_PATHS {
        if canonical_str == *protected || canonical_str.starts_with(&format!("{}/", protected)) {
            // Allow user home directories and temp directories
            let allowed_prefixes = [
                "/Users/",              // macOS home dirs
                "/home/",               // Linux home dirs
                "/var/folders/",        // macOS temp dirs (non-canonical)
                "/private/var/folders/",// macOS temp dirs (canonical - /var symlinks to /private/var)
                "/tmp/",                // Linux temp dirs
                "/private/tmp/",        // macOS temp dirs (canonical - /tmp symlinks to /private/tmp)
            ];
            let is_allowed = allowed_prefixes
                .iter()
                .any(|prefix| canonical_str.starts_with(prefix));

            if !is_allowed {
                return Err(format!(
                    "Cannot operate on protected system path: {}",
                    folder_path
                ));
            }
        }
    }

    // Must be a directory
    if !canonical.is_dir() {
        return Err(format!("Path is not a directory: {}", folder_path));
    }

    Ok(canonical)
}

/// History store for managing organization history files.
///
/// Files are stored in `~/.config/sentinel/history/`:
/// - `index.json` - Global index of all organized folders
/// - `{folder_hash}.history.json` - Per-folder session history
pub struct HistoryStore {
    history_dir: PathBuf,
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryStore {
    /// Create a new history store, ensuring the directory exists
    pub fn new() -> Self {
        let history_dir = dirs::config_dir()
            .expect("Failed to get config directory")
            .join("sentinel")
            .join("history");

        // Ensure directory exists
        if let Err(e) = fs::create_dir_all(&history_dir) {
            tracing::warn!("Failed to create history directory: {}", e);
        }

        Self { history_dir }
    }

    /// Get the path for a folder's history file
    fn history_file_path(&self, folder_hash: &str) -> PathBuf {
        self.history_dir
            .join(format!("{}.{}", folder_hash, HISTORY_EXTENSION))
    }

    /// Get the path for the global index
    fn index_file_path(&self) -> PathBuf {
        self.history_dir.join(INDEX_FILENAME)
    }

    /// Hash a folder path to create a unique identifier
    pub fn folder_hash(folder_path: &str) -> String {
        hash_folder_path(folder_path)
    }

    /// Atomically write JSON to a file
    fn atomic_write<T: serde::Serialize>(&self, path: &PathBuf, data: &T) -> Result<(), String> {
        // Write to temporary file first
        let temp_path = path.with_extension("tmp");

        let file = File::create(&temp_path)
            .map_err(|e| format!("Failed to create temp file: {}", e))?;

        let mut writer = BufWriter::new(file);

        serde_json::to_writer_pretty(&mut writer, data)
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        writer
            .flush()
            .map_err(|e| format!("Failed to flush: {}", e))?;

        // Sync to disk
        writer
            .get_ref()
            .sync_all()
            .map_err(|e| format!("Failed to sync: {}", e))?;

        // Atomic rename
        fs::rename(&temp_path, path).map_err(|e| format!("Failed to rename: {}", e))?;

        Ok(())
    }

    /// Save a session to history
    pub fn save_session(&self, folder_path: &str, session: HistorySession) -> Result<(), String> {
        // Validate the folder path before processing
        let canonical = validate_folder_path(folder_path)?;
        let folder_path = canonical.to_string_lossy().to_string();

        let folder_hash = Self::folder_hash(&folder_path);
        let history_path = self.history_file_path(&folder_hash);

        // Load existing history or create new
        let mut history = self.load_history_internal(&folder_path)?.unwrap_or_else(|| {
            FolderHistory::new(folder_path.clone(), folder_hash.clone())
        });

        // Add the session (enforces retention limit)
        history.add_session(session);

        // Save history file
        self.atomic_write(&history_path, &history)?;

        // Update global index
        self.update_index_entry(&folder_hash, &folder_path, history.sessions.len())?;

        tracing::info!(
            "Saved history session for {} ({} sessions)",
            folder_path,
            history.sessions.len()
        );

        Ok(())
    }

    /// Load history for a folder (with path validation)
    pub fn load_history(&self, folder_path: &str) -> Result<Option<FolderHistory>, String> {
        // Validate the folder path
        let canonical = validate_folder_path(folder_path)?;
        self.load_history_internal(&canonical.to_string_lossy())
    }

    /// Load history for a folder (internal, no validation - for use after validation)
    fn load_history_internal(&self, folder_path: &str) -> Result<Option<FolderHistory>, String> {
        let folder_hash = Self::folder_hash(folder_path);
        let history_path = self.history_file_path(&folder_hash);

        if !history_path.exists() {
            return Ok(None);
        }

        let file = File::open(&history_path)
            .map_err(|e| format!("Failed to open history file: {}", e))?;

        let reader = BufReader::new(file);

        let history: FolderHistory = serde_json::from_reader(reader)
            .map_err(|e| format!("Failed to parse history file: {}", e))?;

        Ok(Some(history))
    }

    /// Check if a folder has history
    pub fn has_history(&self, folder_path: &str) -> bool {
        // Validate path first - if invalid, return false
        let canonical = match validate_folder_path(folder_path) {
            Ok(p) => p,
            Err(_) => return false,
        };
        let folder_hash = Self::folder_hash(&canonical.to_string_lossy());
        self.history_file_path(&folder_hash).exists()
    }

    /// Get session summaries for a folder
    pub fn get_session_summaries(&self, folder_path: &str) -> Result<Vec<SessionSummary>, String> {
        match self.load_history(folder_path)? {
            Some(history) => Ok(history.get_summaries()),
            None => Ok(vec![]),
        }
    }

    /// Get a summary of folder history
    pub fn get_summary(&self, folder_path: &str) -> Result<Option<HistorySummary>, String> {
        let history = match self.load_history(folder_path)? {
            Some(h) => h,
            None => return Ok(None),
        };

        let total_operations: usize = history
            .sessions
            .iter()
            .map(|s| s.operations.len())
            .sum();

        let last_organized = history
            .sessions
            .first()
            .map(|s| s.executed_at)
            .unwrap_or_else(Utc::now);

        Ok(Some(HistorySummary {
            folder_path: folder_path.to_string(),
            session_count: history.sessions.len(),
            total_operations,
            last_organized,
        }))
    }

    /// Get a specific session by ID
    pub fn get_session(&self, folder_path: &str, session_id: &str) -> Result<Option<HistorySession>, String> {
        let history = match self.load_history(folder_path)? {
            Some(h) => h,
            None => return Ok(None),
        };

        Ok(history.find_session(session_id).cloned())
    }

    /// Mark sessions as undone
    pub fn mark_sessions_undone(
        &self,
        folder_path: &str,
        up_to_session_id: &str,
    ) -> Result<(), String> {
        // Validate the folder path
        let canonical = validate_folder_path(folder_path)?;
        let folder_path = canonical.to_string_lossy().to_string();

        let folder_hash = Self::folder_hash(&folder_path);
        let history_path = self.history_file_path(&folder_hash);

        let mut history = self
            .load_history_internal(&folder_path)?
            .ok_or_else(|| format!("No history found for {}", folder_path))?;

        history.mark_sessions_undone(up_to_session_id);

        self.atomic_write(&history_path, &history)?;

        Ok(())
    }

    /// Delete history for a folder
    pub fn delete_history(&self, folder_path: &str) -> Result<(), String> {
        // Validate the folder path
        let canonical = validate_folder_path(folder_path)?;
        let folder_path = canonical.to_string_lossy().to_string();

        let folder_hash = Self::folder_hash(&folder_path);
        let history_path = self.history_file_path(&folder_hash);

        if history_path.exists() {
            fs::remove_file(&history_path)
                .map_err(|e| format!("Failed to delete history file: {}", e))?;
        }

        // Remove from index
        self.remove_index_entry(&folder_hash)?;

        tracing::info!("Deleted history for {}", folder_path);

        Ok(())
    }

    /// Load the global index
    pub fn load_index(&self) -> Result<HistoryIndex, String> {
        let index_path = self.index_file_path();

        if !index_path.exists() {
            return Ok(HistoryIndex::new());
        }

        let file = File::open(&index_path)
            .map_err(|e| format!("Failed to open index file: {}", e))?;

        let reader = BufReader::new(file);

        let index: HistoryIndex = serde_json::from_reader(reader)
            .map_err(|e| format!("Failed to parse index file: {}", e))?;

        Ok(index)
    }

    /// Update an entry in the global index
    fn update_index_entry(
        &self,
        folder_hash: &str,
        folder_path: &str,
        session_count: usize,
    ) -> Result<(), String> {
        let mut index = self.load_index()?;

        let entry = FolderIndexEntry {
            folder_path: folder_path.to_string(),
            folder_hash: folder_hash.to_string(),
            session_count,
            last_organized: Utc::now(),
        };

        index.update_folder(entry);

        let index_path = self.index_file_path();
        self.atomic_write(&index_path, &index)?;

        Ok(())
    }

    /// Remove an entry from the global index
    fn remove_index_entry(&self, folder_hash: &str) -> Result<(), String> {
        let mut index = self.load_index()?;

        index.remove_folder(folder_hash);

        let index_path = self.index_file_path();
        self.atomic_write(&index_path, &index)?;

        Ok(())
    }

    /// List all folders with history
    pub fn list_folders(&self) -> Result<Vec<FolderIndexEntry>, String> {
        let index = self.load_index()?;
        Ok(index.folders.into_values().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create test store with history dir and target folders
    fn create_test_store() -> (HistoryStore, TempDir, TempDir) {
        let history_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();
        let store = HistoryStore {
            history_dir: history_dir.path().to_path_buf(),
        };
        (store, history_dir, target_dir)
    }

    fn create_test_session(id: &str, target_folder: &str) -> HistorySession {
        HistorySession {
            session_id: id.to_string(),
            user_instruction: "Organize by type".to_string(),
            plan_description: "Created folders".to_string(),
            executed_at: Utc::now(),
            target_folder: target_folder.to_string(),
            operations: vec![],
            files_affected: 5,
            undone: false,
        }
    }

    #[test]
    fn test_save_and_load_session() {
        let (store, _history_dir, target_dir) = create_test_store();
        let folder_path = target_dir.path().to_string_lossy().to_string();
        let session = create_test_session("session-1", &folder_path);

        // Save
        store.save_session(&folder_path, session.clone()).unwrap();

        // Load
        let history = store.load_history(&folder_path).unwrap().unwrap();
        assert_eq!(history.sessions.len(), 1);
        assert_eq!(history.sessions[0].session_id, "session-1");
    }

    #[test]
    fn test_has_history() {
        let (store, _history_dir, target_dir) = create_test_store();
        let folder_path = target_dir.path().to_string_lossy().to_string();

        assert!(!store.has_history(&folder_path));

        store
            .save_session(&folder_path, create_test_session("session-1", &folder_path))
            .unwrap();

        assert!(store.has_history(&folder_path));
    }

    #[test]
    fn test_session_summaries() {
        let (store, _history_dir, target_dir) = create_test_store();
        let folder_path = target_dir.path().to_string_lossy().to_string();

        store
            .save_session(&folder_path, create_test_session("session-1", &folder_path))
            .unwrap();
        store
            .save_session(&folder_path, create_test_session("session-2", &folder_path))
            .unwrap();

        let summaries = store.get_session_summaries(&folder_path).unwrap();
        assert_eq!(summaries.len(), 2);
        // Most recent first
        assert_eq!(summaries[0].session_id, "session-2");
    }

    #[test]
    fn test_global_index() {
        let (store, _history_dir, _target_dir) = create_test_store();
        // Create two separate target directories
        let target_a = TempDir::new().unwrap();
        let target_b = TempDir::new().unwrap();
        let folder_a = target_a.path().to_string_lossy().to_string();
        let folder_b = target_b.path().to_string_lossy().to_string();

        store
            .save_session(&folder_a, create_test_session("session-1", &folder_a))
            .unwrap();
        store
            .save_session(&folder_b, create_test_session("session-2", &folder_b))
            .unwrap();

        let folders = store.list_folders().unwrap();
        assert_eq!(folders.len(), 2);
    }

    #[test]
    fn test_delete_history() {
        let (store, _history_dir, target_dir) = create_test_store();
        let folder_path = target_dir.path().to_string_lossy().to_string();

        store
            .save_session(&folder_path, create_test_session("session-1", &folder_path))
            .unwrap();

        assert!(store.has_history(&folder_path));

        store.delete_history(&folder_path).unwrap();

        assert!(!store.has_history(&folder_path));
        assert!(store.list_folders().unwrap().is_empty());
    }
}
