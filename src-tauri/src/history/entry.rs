//! Data structures for organization history and undo operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Schema version for forward compatibility
pub const HISTORY_SCHEMA_VERSION: u32 = 1;

/// Maximum number of sessions to retain per folder
pub const MAX_SESSIONS_PER_FOLDER: usize = 10;

/// Operation record (mirrors WALOperationType for serialization)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationRecord {
    CreateFolder {
        path: String,
    },
    Move {
        source: String,
        destination: String,
    },
    Rename {
        path: String,
        #[serde(rename = "newName")]
        new_name: String,
    },
    Quarantine {
        path: String,
        #[serde(rename = "quarantinePath")]
        quarantine_path: String,
    },
    Copy {
        source: String,
        destination: String,
    },
    DeleteFolder {
        path: String,
    },
}

impl OperationRecord {
    /// Compute the inverse operation for undo
    pub fn inverse(&self) -> Self {
        match self {
            OperationRecord::CreateFolder { path } => OperationRecord::DeleteFolder {
                path: path.clone(),
            },
            OperationRecord::Move {
                source,
                destination,
            } => OperationRecord::Move {
                source: destination.clone(),
                destination: source.clone(),
            },
            OperationRecord::Rename { path, new_name } => {
                // Extract the original name from the path
                let path_buf = std::path::PathBuf::from(path);
                let parent = path_buf
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let original_name = path_buf
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                // New path after rename
                let new_path = if parent.is_empty() {
                    new_name.clone()
                } else {
                    format!("{}/{}", parent, new_name)
                };

                OperationRecord::Rename {
                    path: new_path,
                    new_name: original_name,
                }
            }
            OperationRecord::Quarantine {
                path,
                quarantine_path,
            } => OperationRecord::Move {
                source: quarantine_path.clone(),
                destination: path.clone(),
            },
            OperationRecord::Copy {
                source: _,
                destination,
            } => {
                // Undo copy by deleting the destination
                OperationRecord::DeleteFolder {
                    path: destination.clone(),
                }
            }
            OperationRecord::DeleteFolder { path } => {
                // Best-effort: recreate the folder (won't restore contents)
                OperationRecord::CreateFolder { path: path.clone() }
            }
        }
    }

    /// Get a human-readable description of the operation
    pub fn description(&self) -> String {
        match self {
            OperationRecord::CreateFolder { path } => {
                format!("Create folder: {}", path)
            }
            OperationRecord::Move {
                source,
                destination,
            } => {
                format!("Move: {} → {}", source, destination)
            }
            OperationRecord::Rename { path, new_name } => {
                format!("Rename: {} → {}", path, new_name)
            }
            OperationRecord::Quarantine {
                path,
                quarantine_path: _,
            } => {
                format!("Quarantine: {}", path)
            }
            OperationRecord::Copy {
                source,
                destination,
            } => {
                format!("Copy: {} → {}", source, destination)
            }
            OperationRecord::DeleteFolder { path } => {
                format!("Delete folder: {}", path)
            }
        }
    }
}

/// File checksum for integrity verification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileChecksum {
    /// SHA-256 hash of file content (hex-encoded, empty for directories)
    pub sha256: String,
    /// File size in bytes (0 for directories)
    pub size: u64,
    /// Modification time (unix timestamp)
    pub mtime: u64,
    /// Whether this is a directory
    pub is_directory: bool,
}

/// Single operation with checksums for integrity verification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryOperation {
    /// Unique operation ID
    pub id: String,
    /// Sequence number (execution order)
    pub sequence: u32,
    /// The operation that was performed
    pub operation: OperationRecord,
    /// The inverse operation for undo
    pub undo_operation: OperationRecord,
    /// Checksums of source files BEFORE operation
    pub source_checksums: HashMap<String, FileChecksum>,
    /// Checksums of result files AFTER operation
    pub result_checksums: HashMap<String, FileChecksum>,
}

/// Organization session representing one complete organization run
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySession {
    /// Unique session ID (from job_id or plan_id)
    pub session_id: String,
    /// User-provided organization instruction
    pub user_instruction: String,
    /// AI-generated plan description
    pub plan_description: String,
    /// When the session was executed
    pub executed_at: DateTime<Utc>,
    /// Target folder path (canonical)
    pub target_folder: String,
    /// All operations in execution order
    pub operations: Vec<HistoryOperation>,
    /// Total files affected
    pub files_affected: usize,
    /// Whether this session has been undone
    pub undone: bool,
}

impl HistorySession {
    /// Get total operation count
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }

    /// Convert to a lightweight summary
    pub fn to_summary(&self) -> SessionSummary {
        SessionSummary {
            session_id: self.session_id.clone(),
            user_instruction: self.user_instruction.clone(),
            plan_description: self.plan_description.clone(),
            executed_at: self.executed_at,
            files_affected: self.files_affected,
            operation_count: self.operations.len(),
            undone: self.undone,
        }
    }
}

/// Lightweight session summary for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: String,
    pub user_instruction: String,
    pub plan_description: String,
    pub executed_at: DateTime<Utc>,
    pub files_affected: usize,
    pub operation_count: usize,
    pub undone: bool,
}

/// Per-folder history file containing all sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderHistory {
    /// Schema version for migrations
    pub version: u32,
    /// Canonical folder path
    pub folder_path: String,
    /// Hash of folder path (used for filename)
    pub folder_hash: String,
    /// All sessions for this folder (most recent first)
    pub sessions: Vec<HistorySession>,
    /// When this history was last updated
    pub last_updated: DateTime<Utc>,
}

impl FolderHistory {
    /// Create a new empty history for a folder
    pub fn new(folder_path: String, folder_hash: String) -> Self {
        Self {
            version: HISTORY_SCHEMA_VERSION,
            folder_path,
            folder_hash,
            sessions: Vec::new(),
            last_updated: Utc::now(),
        }
    }

    /// Add a session, enforcing the retention limit
    pub fn add_session(&mut self, session: HistorySession) {
        // Insert at the beginning (most recent first)
        self.sessions.insert(0, session);

        // Enforce retention limit
        if self.sessions.len() > MAX_SESSIONS_PER_FOLDER {
            self.sessions.pop();
        }

        self.last_updated = Utc::now();
    }

    /// Get session summaries
    pub fn get_summaries(&self) -> Vec<SessionSummary> {
        self.sessions.iter().map(|s| s.to_summary()).collect()
    }

    /// Find a session by ID
    pub fn find_session(&self, session_id: &str) -> Option<&HistorySession> {
        self.sessions.iter().find(|s| s.session_id == session_id)
    }

    /// Mark sessions as undone up to and including the target session
    pub fn mark_sessions_undone(&mut self, up_to_session_id: &str) {
        let mut found = false;
        for session in &mut self.sessions {
            if !found {
                session.undone = true;
            }
            if session.session_id == up_to_session_id {
                found = true;
            }
        }
        self.last_updated = Utc::now();
    }
}

/// Global index entry for a folder
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderIndexEntry {
    /// Canonical folder path
    pub folder_path: String,
    /// Hash of folder path (used for filename)
    pub folder_hash: String,
    /// Number of sessions in history
    pub session_count: usize,
    /// When folder was last organized
    pub last_organized: DateTime<Utc>,
}

/// Global index of all organized folders
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryIndex {
    /// Schema version
    pub version: u32,
    /// Map of folder_hash -> FolderIndexEntry
    pub folders: HashMap<String, FolderIndexEntry>,
    /// When index was last updated
    pub last_updated: DateTime<Utc>,
}

impl Default for HistoryIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryIndex {
    /// Create a new empty index
    pub fn new() -> Self {
        Self {
            version: HISTORY_SCHEMA_VERSION,
            folders: HashMap::new(),
            last_updated: Utc::now(),
        }
    }

    /// Update or add an entry for a folder
    pub fn update_folder(&mut self, entry: FolderIndexEntry) {
        self.folders.insert(entry.folder_hash.clone(), entry);
        self.last_updated = Utc::now();
    }

    /// Remove a folder from the index
    pub fn remove_folder(&mut self, folder_hash: &str) {
        self.folders.remove(folder_hash);
        self.last_updated = Utc::now();
    }

    /// List all folders
    pub fn list_folders(&self) -> Vec<&FolderIndexEntry> {
        self.folders.values().collect()
    }
}

/// Summary of folder history for frontend display
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummary {
    pub folder_path: String,
    pub session_count: usize,
    pub total_operations: usize,
    pub last_organized: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_inverse_move() {
        let op = OperationRecord::Move {
            source: "/a/file.txt".to_string(),
            destination: "/b/file.txt".to_string(),
        };
        let inverse = op.inverse();
        assert_eq!(
            inverse,
            OperationRecord::Move {
                source: "/b/file.txt".to_string(),
                destination: "/a/file.txt".to_string(),
            }
        );
    }

    #[test]
    fn test_operation_inverse_create_folder() {
        let op = OperationRecord::CreateFolder {
            path: "/new/folder".to_string(),
        };
        let inverse = op.inverse();
        assert_eq!(
            inverse,
            OperationRecord::DeleteFolder {
                path: "/new/folder".to_string(),
            }
        );
    }

    #[test]
    fn test_folder_history_retention() {
        let mut history = FolderHistory::new("test".to_string(), "abc123".to_string());

        // Add more than MAX_SESSIONS_PER_FOLDER sessions
        for i in 0..15 {
            let session = HistorySession {
                session_id: format!("session-{}", i),
                user_instruction: "test".to_string(),
                plan_description: "test".to_string(),
                executed_at: Utc::now(),
                target_folder: "test".to_string(),
                operations: vec![],
                files_affected: 0,
                undone: false,
            };
            history.add_session(session);
        }

        // Should only keep MAX_SESSIONS_PER_FOLDER
        assert_eq!(history.sessions.len(), MAX_SESSIONS_PER_FOLDER);

        // Most recent should be first
        assert_eq!(history.sessions[0].session_id, "session-14");
    }
}
