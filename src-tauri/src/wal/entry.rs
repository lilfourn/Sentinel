//! WAL entry types and structures
//!
//! Defines the core types for WAL entries including operation types,
//! status tracking, and the journal structure for organizing entries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Status of a WAL entry
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WALStatus {
    /// Operation has been logged but not yet started
    #[default]
    Pending,
    /// Operation is currently executing
    InProgress,
    /// Operation completed successfully
    Complete,
    /// Operation failed with an error
    Failed,
    /// Operation was rolled back (undone)
    RolledBack,
}

/// Type of filesystem operation logged in the WAL
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WALOperationType {
    /// Create a new folder
    CreateFolder { path: PathBuf },
    /// Move a file or folder from source to destination
    Move {
        source: PathBuf,
        destination: PathBuf,
    },
    /// Rename a file or folder
    Rename { path: PathBuf, new_name: String },
    /// Move a file to quarantine (temporary holding area)
    Quarantine {
        path: PathBuf,
        quarantine_path: PathBuf,
    },
    /// Copy a file or folder from source to destination
    Copy {
        source: PathBuf,
        destination: PathBuf,
    },
    /// Delete a folder (only empty folders, used for cleanup)
    DeleteFolder { path: PathBuf },
}

impl WALOperationType {
    /// Generate the inverse (undo) operation for this operation type.
    ///
    /// Returns an operation that, when executed, will reverse the effect
    /// of the original operation. This is used for rollback scenarios.
    ///
    /// # Returns
    /// * `Ok(WALOperationType)` - The inverse operation
    /// * `Err(String)` - Error if inverse cannot be computed (e.g., invalid paths)
    pub fn inverse(&self) -> Result<WALOperationType, String> {
        match self {
            WALOperationType::CreateFolder { path } => {
                // Inverse of create is delete
                Ok(WALOperationType::DeleteFolder { path: path.clone() })
            }
            WALOperationType::Move {
                source,
                destination,
            } => {
                // Inverse of move is move back
                Ok(WALOperationType::Move {
                    source: destination.clone(),
                    destination: source.clone(),
                })
            }
            WALOperationType::Rename { path, new_name } => {
                // Inverse of rename requires the old name
                // We derive the old name from the current path
                let old_name = path
                    .file_name()
                    .ok_or_else(|| format!("Cannot compute inverse: path has no filename: {}", path.display()))?
                    .to_string_lossy()
                    .to_string();
                let parent = path
                    .parent()
                    .ok_or_else(|| format!("Cannot compute inverse: path has no parent: {}", path.display()))?;
                let new_path = parent.join(new_name);
                Ok(WALOperationType::Rename {
                    path: new_path,
                    new_name: old_name,
                })
            }
            WALOperationType::Quarantine {
                path,
                quarantine_path,
            } => {
                // Inverse of quarantine is move back from quarantine
                Ok(WALOperationType::Move {
                    source: quarantine_path.clone(),
                    destination: path.clone(),
                })
            }
            WALOperationType::Copy {
                source: _,
                destination,
            } => {
                // Inverse of copy is delete the destination
                // Note: We use DeleteFolder for directories, for files
                // the executor should handle appropriately
                Ok(WALOperationType::DeleteFolder {
                    path: destination.clone(),
                })
            }
            WALOperationType::DeleteFolder { path } => {
                // Cannot truly undo a delete without backup
                // Return a no-op equivalent (create same folder)
                Ok(WALOperationType::CreateFolder { path: path.clone() })
            }
        }
    }

    /// Generate inverse operation without error checking (panics on invalid paths)
    ///
    /// For use in contexts where path validity has already been verified.
    /// Prefer `inverse()` for new code.
    #[allow(dead_code)]
    pub fn inverse_unchecked(&self) -> WALOperationType {
        self.inverse().expect("inverse_unchecked called on invalid operation")
    }

    /// Get a human-readable description of this operation
    pub fn description(&self) -> String {
        match self {
            WALOperationType::CreateFolder { path } => {
                format!("Create folder: {}", path.display())
            }
            WALOperationType::Move {
                source,
                destination,
            } => {
                format!("Move {} -> {}", source.display(), destination.display())
            }
            WALOperationType::Rename { path, new_name } => {
                format!("Rename {} to {}", path.display(), new_name)
            }
            WALOperationType::Quarantine {
                path,
                quarantine_path,
            } => {
                format!(
                    "Quarantine {} -> {}",
                    path.display(),
                    quarantine_path.display()
                )
            }
            WALOperationType::Copy {
                source,
                destination,
            } => {
                format!("Copy {} -> {}", source.display(), destination.display())
            }
            WALOperationType::DeleteFolder { path } => {
                format!("Delete folder: {}", path.display())
            }
        }
    }
}

/// A single entry in the Write-Ahead Log
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WALEntry {
    /// Unique identifier for this entry
    pub id: Uuid,
    /// Sequence number within the journal (execution order hint)
    pub sequence: u32,
    /// The operation to perform
    pub operation: WALOperationType,
    /// The inverse operation for rollback
    pub undo_operation: WALOperationType,
    /// Current status of this entry
    pub status: WALStatus,
    /// When this entry was created
    pub created_at: DateTime<Utc>,
    /// Last time this entry was updated
    pub updated_at: DateTime<Utc>,
    /// Error message if the operation failed
    pub error: Option<String>,
    /// IDs of entries this entry depends on (must complete first)
    pub depends_on: Vec<Uuid>,
}

impl WALEntry {
    /// Create a new WAL entry with the given operation
    ///
    /// # Returns
    /// * `Ok(WALEntry)` - The created entry
    /// * `Err(String)` - Error if inverse operation cannot be computed
    pub fn new(operation: WALOperationType, sequence: u32) -> Result<Self, String> {
        let undo_operation = operation.inverse()?;
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            sequence,
            operation,
            undo_operation,
            status: WALStatus::Pending,
            created_at: now,
            updated_at: now,
            error: None,
            depends_on: Vec::new(),
        })
    }

    /// Create a new WAL entry with dependencies
    ///
    /// # Returns
    /// * `Ok(WALEntry)` - The created entry
    /// * `Err(String)` - Error if inverse operation cannot be computed
    pub fn new_with_deps(
        operation: WALOperationType,
        sequence: u32,
        depends_on: Vec<Uuid>,
    ) -> Result<Self, String> {
        let mut entry = Self::new(operation, sequence)?;
        entry.depends_on = depends_on;
        Ok(entry)
    }

    /// Mark this entry as in progress
    pub fn mark_in_progress(&mut self) {
        self.status = WALStatus::InProgress;
        self.updated_at = Utc::now();
    }

    /// Mark this entry as complete
    pub fn mark_complete(&mut self) {
        self.status = WALStatus::Complete;
        self.updated_at = Utc::now();
    }

    /// Mark this entry as failed with an error message
    pub fn mark_failed(&mut self, error: String) {
        self.status = WALStatus::Failed;
        self.error = Some(error);
        self.updated_at = Utc::now();
    }

    /// Mark this entry as rolled back
    pub fn mark_rolled_back(&mut self) {
        self.status = WALStatus::RolledBack;
        self.updated_at = Utc::now();
    }

    /// Check if this entry has completed (either success or failure)
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            WALStatus::Complete | WALStatus::Failed | WALStatus::RolledBack
        )
    }

    /// Check if this entry is pending execution
    pub fn is_pending(&self) -> bool {
        matches!(self.status, WALStatus::Pending | WALStatus::InProgress)
    }
}

/// A collection of WAL entries for a single job/session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WALJournal {
    /// Unique identifier for this job
    pub job_id: String,
    /// The target folder being organized
    pub target_folder: PathBuf,
    /// When this journal was started
    pub started_at: DateTime<Utc>,
    /// All entries in this journal
    pub entries: Vec<WALEntry>,
    /// Schema version for forward compatibility
    pub version: u32,
}

impl WALJournal {
    /// Current schema version
    pub const CURRENT_VERSION: u32 = 1;

    /// Create a new empty journal
    pub fn new(job_id: String, target_folder: PathBuf) -> Self {
        Self {
            job_id,
            target_folder,
            started_at: Utc::now(),
            entries: Vec::new(),
            version: Self::CURRENT_VERSION,
        }
    }

    /// Add an entry to this journal
    pub fn add_entry(&mut self, entry: WALEntry) {
        self.entries.push(entry);
    }

    /// Add a new operation and return its ID
    ///
    /// # Returns
    /// * `Ok(Uuid)` - The ID of the created entry
    /// * `Err(String)` - Error if inverse operation cannot be computed
    pub fn add_operation(&mut self, operation: WALOperationType) -> Result<Uuid, String> {
        let sequence = self.entries.len() as u32;
        let entry = WALEntry::new(operation, sequence)?;
        let id = entry.id;
        self.entries.push(entry);
        Ok(id)
    }

    /// Add a new operation with dependencies
    ///
    /// # Returns
    /// * `Ok(Uuid)` - The ID of the created entry
    /// * `Err(String)` - Error if inverse operation cannot be computed
    pub fn add_operation_with_deps(
        &mut self,
        operation: WALOperationType,
        depends_on: Vec<Uuid>,
    ) -> Result<Uuid, String> {
        let sequence = self.entries.len() as u32;
        let entry = WALEntry::new_with_deps(operation, sequence, depends_on)?;
        let id = entry.id;
        self.entries.push(entry);
        Ok(id)
    }

    /// Find an entry by its ID
    pub fn get_entry(&self, id: Uuid) -> Option<&WALEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Find an entry by its ID (mutable)
    pub fn get_entry_mut(&mut self, id: Uuid) -> Option<&mut WALEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// Get all pending entries (not yet started or in progress)
    pub fn pending_entries(&self) -> Vec<&WALEntry> {
        self.entries.iter().filter(|e| e.is_pending()).collect()
    }

    /// Get all completed entries
    pub fn completed_entries(&self) -> Vec<&WALEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == WALStatus::Complete)
            .collect()
    }

    /// Get all failed entries
    #[allow(dead_code)]
    pub fn failed_entries(&self) -> Vec<&WALEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == WALStatus::Failed)
            .collect()
    }

    /// Check if all entries have completed successfully
    pub fn is_complete(&self) -> bool {
        self.entries.iter().all(|e| e.status == WALStatus::Complete)
    }

    /// Check if any entry has failed
    #[allow(dead_code)]
    pub fn has_failures(&self) -> bool {
        self.entries.iter().any(|e| e.status == WALStatus::Failed)
    }

    /// Count of entries by status
    pub fn status_counts(&self) -> (usize, usize, usize, usize) {
        let pending = self
            .entries
            .iter()
            .filter(|e| e.status == WALStatus::Pending)
            .count();
        let in_progress = self
            .entries
            .iter()
            .filter(|e| e.status == WALStatus::InProgress)
            .count();
        let complete = self
            .entries
            .iter()
            .filter(|e| e.status == WALStatus::Complete)
            .count();
        let failed = self
            .entries
            .iter()
            .filter(|e| e.status == WALStatus::Failed)
            .count();
        (pending, in_progress, complete, failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_folder_inverse() {
        let op = WALOperationType::CreateFolder {
            path: PathBuf::from("/test/folder"),
        };
        let inverse = op.inverse().unwrap();
        assert!(matches!(inverse, WALOperationType::DeleteFolder { .. }));
    }

    #[test]
    fn test_move_inverse() {
        let op = WALOperationType::Move {
            source: PathBuf::from("/src"),
            destination: PathBuf::from("/dst"),
        };
        let inverse = op.inverse().unwrap();
        if let WALOperationType::Move {
            source,
            destination,
        } = inverse
        {
            assert_eq!(source, PathBuf::from("/dst"));
            assert_eq!(destination, PathBuf::from("/src"));
        } else {
            panic!("Expected Move inverse");
        }
    }

    #[test]
    fn test_rename_inverse_error_no_filename() {
        // Root path has no filename
        let op = WALOperationType::Rename {
            path: PathBuf::from("/"),
            new_name: "new".to_string(),
        };
        let result = op.inverse();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no filename"));
    }

    #[test]
    fn test_journal_add_operation() {
        let mut journal = WALJournal::new("test-job".to_string(), PathBuf::from("/test"));
        let id = journal.add_operation(WALOperationType::CreateFolder {
            path: PathBuf::from("/test/new"),
        }).unwrap();
        assert_eq!(journal.entries.len(), 1);
        assert!(journal.get_entry(id).is_some());
    }

    #[test]
    fn test_entry_status_transitions() {
        let mut entry = WALEntry::new(
            WALOperationType::CreateFolder {
                path: PathBuf::from("/test"),
            },
            0,
        ).unwrap();
        assert_eq!(entry.status, WALStatus::Pending);

        entry.mark_in_progress();
        assert_eq!(entry.status, WALStatus::InProgress);

        entry.mark_complete();
        assert_eq!(entry.status, WALStatus::Complete);
        assert!(entry.is_terminal());
    }
}
