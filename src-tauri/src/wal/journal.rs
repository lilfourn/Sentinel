//! WAL Journal Manager
//!
//! Handles persistence of WAL journals to disk, enabling crash recovery.
//! Journals are stored as JSON files in ~/.config/sentinel/wal/
//!
//! ## Concurrency Safety
//! Uses file locking via fs2 to prevent race conditions when multiple
//! parallel operations update the same journal simultaneously.
//!
//! ## Durability
//! Uses atomic writes with fsync to ensure data integrity even on crash.

use super::entry::{WALJournal, WALStatus};
use super::io::atomic_write;
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use uuid::Uuid;

/// Maximum number of entries allowed in a single journal
pub const MAX_JOURNAL_ENTRIES: usize = 10_000;

/// Maximum serialized size of a journal (10 MB)
pub const MAX_JOURNAL_SIZE: usize = 10 * 1024 * 1024;

/// Error type for WAL operations
#[derive(Debug, Clone)]
pub struct WALError {
    pub message: String,
    pub kind: WALErrorKind,
}

#[derive(Debug, Clone)]
pub enum WALErrorKind {
    IoError,
    SerializationError,
    NotFound,
    InvalidState,
    LimitExceeded,
}

impl std::fmt::Display for WALError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for WALError {}

impl From<WALError> for String {
    fn from(err: WALError) -> Self {
        err.message
    }
}

/// Manager for WAL journal persistence
pub struct WALManager {
    /// Base directory for WAL storage
    wal_dir: PathBuf,
}

impl WALManager {
    /// Create a new WAL manager with default directory
    pub fn new() -> Self {
        Self {
            wal_dir: Self::default_wal_dir(),
        }
    }

    /// Create a WAL manager with a custom directory (for testing)
    #[allow(dead_code)]
    pub fn with_dir(wal_dir: PathBuf) -> Self {
        Self { wal_dir }
    }

    /// Get the default WAL directory (~/.config/sentinel/wal/)
    fn default_wal_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("sentinel")
            .join("wal")
    }

    /// Get the WAL directory path
    pub fn get_wal_dir(&self) -> PathBuf {
        self.wal_dir.clone()
    }

    /// Ensure the WAL directory exists
    fn ensure_dir(&self) -> Result<(), WALError> {
        fs::create_dir_all(&self.wal_dir).map_err(|e| WALError {
            message: format!("Failed to create WAL directory: {}", e),
            kind: WALErrorKind::IoError,
        })
    }

    /// Get the file path for a journal
    fn journal_path(&self, job_id: &str) -> PathBuf {
        self.wal_dir.join(format!("{}.wal.json", job_id))
    }

    /// Get the lock file path for a journal
    fn lock_path(&self, job_id: &str) -> PathBuf {
        self.wal_dir.join(format!("{}.wal.lock", job_id))
    }

    /// Acquire an exclusive lock for a journal.
    /// Returns a File handle that must be kept alive while holding the lock.
    fn acquire_lock(&self, job_id: &str) -> Result<File, WALError> {
        self.ensure_dir()?;

        let lock_path = self.lock_path(job_id);
        let lock_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| WALError {
                message: format!("Failed to open lock file: {}", e),
                kind: WALErrorKind::IoError,
            })?;

        // Block until we get exclusive access
        lock_file.lock_exclusive().map_err(|e| WALError {
            message: format!("Failed to acquire lock: {}", e),
            kind: WALErrorKind::IoError,
        })?;

        Ok(lock_file)
    }

    /// Save a journal to disk (acquires lock)
    ///
    /// Writes atomically with fsync to ensure durability on crash.
    /// This method acquires an exclusive lock before writing.
    ///
    /// For use within methods that already hold the lock, use `save_journal_internal`.
    pub fn save_journal(&self, journal: &WALJournal) -> Result<(), WALError> {
        let _lock = self.acquire_lock(&journal.job_id)?;
        self.save_journal_internal(journal)
    }

    /// Save a journal to disk (internal - lock must be held by caller)
    ///
    /// This internal method assumes the caller already holds the lock.
    /// Use `save_journal` for external calls.
    fn save_journal_internal(&self, journal: &WALJournal) -> Result<(), WALError> {
        self.ensure_dir()?;

        // Check entry count limit
        if journal.entries.len() > MAX_JOURNAL_ENTRIES {
            return Err(WALError {
                message: format!(
                    "Journal exceeds maximum entry count: {} > {}",
                    journal.entries.len(),
                    MAX_JOURNAL_ENTRIES
                ),
                kind: WALErrorKind::LimitExceeded,
            });
        }

        let path = self.journal_path(&journal.job_id);

        // Serialize to JSON
        let json = serde_json::to_string_pretty(journal).map_err(|e| WALError {
            message: format!("Failed to serialize journal: {}", e),
            kind: WALErrorKind::SerializationError,
        })?;

        // Check size limit
        if json.len() > MAX_JOURNAL_SIZE {
            return Err(WALError {
                message: format!(
                    "Journal exceeds maximum size: {} bytes > {} bytes",
                    json.len(),
                    MAX_JOURNAL_SIZE
                ),
                kind: WALErrorKind::LimitExceeded,
            });
        }

        // Use atomic write with fsync for durability
        atomic_write(&path, json.as_bytes()).map_err(|e| WALError {
            message: format!("Failed to write journal: {}", e),
            kind: WALErrorKind::IoError,
        })?;

        tracing::debug!(
            job_id = %journal.job_id,
            entries = journal.entries.len(),
            size_bytes = json.len(),
            "Saved WAL journal"
        );

        Ok(())
    }

    /// Load a journal from disk by job ID
    pub fn load_journal(&self, job_id: &str) -> Result<Option<WALJournal>, WALError> {
        let path = self.journal_path(job_id);

        if !path.exists() {
            return Ok(None);
        }

        let json = fs::read_to_string(&path).map_err(|e| WALError {
            message: format!("Failed to read journal file: {}", e),
            kind: WALErrorKind::IoError,
        })?;

        let journal: WALJournal = serde_json::from_str(&json).map_err(|e| WALError {
            message: format!("Failed to parse journal: {}", e),
            kind: WALErrorKind::SerializationError,
        })?;

        tracing::debug!(
            job_id = %journal.job_id,
            entries = journal.entries.len(),
            "Loaded WAL journal"
        );

        Ok(Some(journal))
    }

    /// Find any incomplete journal (for recovery on startup)
    ///
    /// Scans the WAL directory for journals that have pending or in-progress entries.
    /// Returns the first incomplete journal found, or None if all are complete.
    pub fn find_incomplete_journal(&self) -> Result<Option<WALJournal>, WALError> {
        if !self.wal_dir.exists() {
            return Ok(None);
        }

        let entries = fs::read_dir(&self.wal_dir).map_err(|e| WALError {
            message: format!("Failed to read WAL directory: {}", e),
            kind: WALErrorKind::IoError,
        })?;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();

            // Only process .wal.json files
            if !path
                .file_name()
                .map(|n| n.to_string_lossy().ends_with(".wal.json"))
                .unwrap_or(false)
            {
                continue;
            }

            // Try to load and check if incomplete
            let json = match fs::read_to_string(&path) {
                Ok(j) => j,
                Err(_) => continue,
            };

            let journal: WALJournal = match serde_json::from_str(&json) {
                Ok(j) => j,
                Err(_) => continue,
            };

            // Check if journal has any pending or in-progress entries
            let has_incomplete = journal.entries.iter().any(|e| {
                matches!(e.status, WALStatus::Pending | WALStatus::InProgress)
            });

            if has_incomplete {
                tracing::info!(
                    job_id = %journal.job_id,
                    pending = journal.pending_entries().len(),
                    "Found incomplete WAL journal for recovery"
                );
                return Ok(Some(journal));
            }
        }

        Ok(None)
    }

    /// Mark a specific entry as complete
    ///
    /// Uses file locking to prevent race conditions with parallel operations.
    pub fn mark_entry_complete(&self, job_id: &str, entry_id: Uuid) -> Result<(), WALError> {
        // Acquire exclusive lock before read-modify-write
        let _lock = self.acquire_lock(job_id)?;

        let mut journal = self.load_journal(job_id)?.ok_or_else(|| WALError {
            message: format!("Journal not found: {}", job_id),
            kind: WALErrorKind::NotFound,
        })?;

        let entry = journal.get_entry_mut(entry_id).ok_or_else(|| WALError {
            message: format!("Entry not found: {}", entry_id),
            kind: WALErrorKind::NotFound,
        })?;

        entry.mark_complete();
        // Use internal save since we already hold the lock
        self.save_journal_internal(&journal)?;

        // Lock is automatically released when _lock is dropped
        tracing::debug!(entry_id = %entry_id, "Marked WAL entry complete");
        Ok(())
    }

    /// Mark a specific entry as failed
    ///
    /// Uses file locking to prevent race conditions with parallel operations.
    pub fn mark_entry_failed(
        &self,
        job_id: &str,
        entry_id: Uuid,
        error: String,
    ) -> Result<(), WALError> {
        // Acquire exclusive lock before read-modify-write
        let _lock = self.acquire_lock(job_id)?;

        let mut journal = self.load_journal(job_id)?.ok_or_else(|| WALError {
            message: format!("Journal not found: {}", job_id),
            kind: WALErrorKind::NotFound,
        })?;

        let entry = journal.get_entry_mut(entry_id).ok_or_else(|| WALError {
            message: format!("Entry not found: {}", entry_id),
            kind: WALErrorKind::NotFound,
        })?;

        entry.mark_failed(error.clone());
        // Use internal save since we already hold the lock
        self.save_journal_internal(&journal)?;

        // Lock is automatically released when _lock is dropped
        tracing::debug!(entry_id = %entry_id, error = %error, "Marked WAL entry failed");
        Ok(())
    }

    /// Mark a specific entry as in progress
    ///
    /// Uses file locking to prevent race conditions with parallel operations.
    pub fn mark_entry_in_progress(&self, job_id: &str, entry_id: Uuid) -> Result<(), WALError> {
        // Acquire exclusive lock before read-modify-write
        let _lock = self.acquire_lock(job_id)?;

        let mut journal = self.load_journal(job_id)?.ok_or_else(|| WALError {
            message: format!("Journal not found: {}", job_id),
            kind: WALErrorKind::NotFound,
        })?;

        let entry = journal.get_entry_mut(entry_id).ok_or_else(|| WALError {
            message: format!("Entry not found: {}", entry_id),
            kind: WALErrorKind::NotFound,
        })?;

        entry.mark_in_progress();
        // Use internal save since we already hold the lock
        self.save_journal_internal(&journal)?;

        // Lock is automatically released when _lock is dropped
        tracing::debug!(entry_id = %entry_id, "Marked WAL entry in progress");
        Ok(())
    }

    /// Mark a specific entry as rolled back
    ///
    /// Uses file locking to prevent race conditions with parallel operations.
    pub fn mark_entry_rolled_back(&self, job_id: &str, entry_id: Uuid) -> Result<(), WALError> {
        // Acquire exclusive lock before read-modify-write
        let _lock = self.acquire_lock(job_id)?;

        let mut journal = self.load_journal(job_id)?.ok_or_else(|| WALError {
            message: format!("Journal not found: {}", job_id),
            kind: WALErrorKind::NotFound,
        })?;

        let entry = journal.get_entry_mut(entry_id).ok_or_else(|| WALError {
            message: format!("Entry not found: {}", entry_id),
            kind: WALErrorKind::NotFound,
        })?;

        entry.mark_rolled_back();
        // Use internal save since we already hold the lock
        self.save_journal_internal(&journal)?;

        // Lock is automatically released when _lock is dropped
        tracing::debug!(entry_id = %entry_id, "Marked WAL entry rolled back");
        Ok(())
    }

    /// Discard (delete) a journal after successful completion
    ///
    /// Acquires lock before deletion to prevent race conditions.
    pub fn discard_journal(&self, job_id: &str) -> Result<(), WALError> {
        // Acquire lock before deletion to prevent races
        let _lock = self.acquire_lock(job_id)?;

        let path = self.journal_path(job_id);
        let lock_path = self.lock_path(job_id);

        if path.exists() {
            fs::remove_file(&path).map_err(|e| WALError {
                message: format!("Failed to delete journal: {}", e),
                kind: WALErrorKind::IoError,
            })?;
            tracing::info!(job_id = %job_id, "Discarded WAL journal");
        }

        // Lock is released when _lock is dropped, then we can remove the lock file
        drop(_lock);

        // Clean up lock file
        if lock_path.exists() {
            let _ = fs::remove_file(&lock_path);
        }

        Ok(())
    }

    /// List all journal IDs in the WAL directory
    pub fn list_journals(&self) -> Result<Vec<String>, WALError> {
        if !self.wal_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&self.wal_dir).map_err(|e| WALError {
            message: format!("Failed to read WAL directory: {}", e),
            kind: WALErrorKind::IoError,
        })?;

        let mut job_ids = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let name = entry.file_name().to_string_lossy().to_string();
            // Only match .wal.json files, not .wal.lock files
            if name.ends_with(".wal.json") {
                let job_id = name.trim_end_matches(".wal.json").to_string();
                job_ids.push(job_id);
            }
        }

        Ok(job_ids)
    }

    /// Clean up stale lock files (e.g., from crashed processes)
    ///
    /// Only removes lock files that can be successfully locked (not held by other processes).
    pub fn cleanup_stale_locks(&self) -> Result<(), WALError> {
        if !self.wal_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.wal_dir).map_err(|e| WALError {
            message: format!("Failed to read WAL directory: {}", e),
            kind: WALErrorKind::IoError,
        })?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".wal.lock") {
                let lock_path = entry.path();

                // Try to acquire lock before deleting
                // If we can't lock it, another process is using it
                if let Ok(lock_file) = OpenOptions::new()
                    .write(true)
                    .open(&lock_path)
                {
                    // Try non-blocking lock
                    if lock_file.try_lock_exclusive().is_ok() {
                        // We got the lock, so no one is using it - safe to delete
                        // IMPORTANT: Delete while still holding the lock to prevent TOCTOU race
                        // The lock_file will be dropped after remove_file, releasing the lock
                        if fs::remove_file(&lock_path).is_ok() {
                            tracing::debug!(path = %lock_path.display(), "Cleaned up stale lock file");
                        }
                        // lock_file is dropped here, releasing the lock after deletion
                    }
                    // If try_lock fails, another process has the lock - leave it alone
                }
            }
        }

        Ok(())
    }
}

impl Default for WALManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::entry::WALOperationType;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn create_test_manager() -> (WALManager, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let manager = WALManager::with_dir(dir.path().to_path_buf());
        (manager, dir)
    }

    #[test]
    fn test_save_and_load_journal() {
        let (manager, _dir) = create_test_manager();

        let mut journal = WALJournal::new("test-job".to_string(), PathBuf::from("/test"));
        journal.add_operation(WALOperationType::CreateFolder {
            path: PathBuf::from("/test/new"),
        }).unwrap();

        manager.save_journal(&journal).unwrap();

        let loaded = manager.load_journal("test-job").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.job_id, "test-job");
        assert_eq!(loaded.entries.len(), 1);
    }

    #[test]
    fn test_find_incomplete_journal() {
        let (manager, _dir) = create_test_manager();

        // Create a complete journal
        let mut complete_journal =
            WALJournal::new("complete-job".to_string(), PathBuf::from("/test1"));
        let id = complete_journal.add_operation(WALOperationType::CreateFolder {
            path: PathBuf::from("/test1/new"),
        }).unwrap();
        complete_journal.get_entry_mut(id).unwrap().mark_complete();
        manager.save_journal(&complete_journal).unwrap();

        // Create an incomplete journal
        let mut incomplete_journal =
            WALJournal::new("incomplete-job".to_string(), PathBuf::from("/test2"));
        incomplete_journal.add_operation(WALOperationType::CreateFolder {
            path: PathBuf::from("/test2/new"),
        }).unwrap();
        manager.save_journal(&incomplete_journal).unwrap();

        // Should find the incomplete one
        let found = manager.find_incomplete_journal().unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().job_id, "incomplete-job");
    }

    #[test]
    fn test_mark_entry_complete() {
        let (manager, _dir) = create_test_manager();

        let mut journal = WALJournal::new("test-job".to_string(), PathBuf::from("/test"));
        let entry_id = journal.add_operation(WALOperationType::CreateFolder {
            path: PathBuf::from("/test/new"),
        }).unwrap();
        manager.save_journal(&journal).unwrap();

        manager
            .mark_entry_complete("test-job", entry_id)
            .unwrap();

        let loaded = manager.load_journal("test-job").unwrap().unwrap();
        assert_eq!(
            loaded.get_entry(entry_id).unwrap().status,
            WALStatus::Complete
        );
    }

    #[test]
    fn test_discard_journal() {
        let (manager, _dir) = create_test_manager();

        let journal = WALJournal::new("test-job".to_string(), PathBuf::from("/test"));
        manager.save_journal(&journal).unwrap();

        assert!(manager.load_journal("test-job").unwrap().is_some());

        manager.discard_journal("test-job").unwrap();

        assert!(manager.load_journal("test-job").unwrap().is_none());
    }
}
