//! Quarantine Module
//!
//! Provides safe deletion functionality by moving files to a quarantine
//! directory instead of permanently deleting them. Supports restoration
//! and automatic cleanup of old entries.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// A quarantined file or directory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantinedItem {
    /// Current path in quarantine
    pub path: PathBuf,

    /// Original file/directory name
    pub name: String,

    /// Original path before quarantine
    pub original_path: PathBuf,

    /// When the item was quarantined
    pub quarantine_date: DateTime<Utc>,

    /// Size in bytes
    pub size: u64,

    /// Whether this is a directory
    pub is_directory: bool,
}

/// Statistics from a cleanup operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupStats {
    /// Number of items removed
    pub items_removed: usize,

    /// Total bytes freed
    pub bytes_freed: u64,

    /// Number of items that failed to delete
    pub errors: usize,
}

/// Manages the quarantine directory for safe deletion
///
/// Files are moved to a quarantine directory instead of being permanently
/// deleted. They can be restored or are automatically cleaned up after
/// the retention period expires.
#[derive(Debug, Clone)]
pub struct QuarantineManager {
    /// Base path for quarantine storage
    base_path: PathBuf,

    /// Number of days to retain quarantined items
    retention_days: u32,
}

impl QuarantineManager {
    /// Create a new QuarantineManager with default settings
    ///
    /// Uses ~/.sentinel/quarantine as the base path and 30 days retention
    pub fn new() -> Result<Self, String> {
        let base_path = dirs::home_dir()
            .ok_or("Could not determine home directory")?
            .join(".sentinel")
            .join("quarantine");

        Ok(Self {
            base_path,
            retention_days: 30,
        })
    }

    /// Create a QuarantineManager with custom settings
    pub fn with_config(base_path: PathBuf, retention_days: u32) -> Self {
        Self {
            base_path,
            retention_days,
        }
    }

    /// Get the base quarantine path
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }

    /// Get the retention period in days
    pub fn retention_days(&self) -> u32 {
        self.retention_days
    }

    /// Ensure the quarantine directory exists
    fn ensure_quarantine_dir(&self) -> Result<(), String> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path)
                .map_err(|e| format!("Failed to create quarantine directory: {}", e))?;
        }
        Ok(())
    }

    /// Generate a unique quarantine path for an item
    fn generate_quarantine_path(&self, original_path: &Path) -> PathBuf {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%3f").to_string();
        let name = original_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        self.base_path.join(format!("{}_{}", timestamp, name))
    }

    /// Move a file or directory to quarantine
    ///
    /// # Arguments
    /// * `path` - The path to quarantine
    ///
    /// # Returns
    /// * `Ok(PathBuf)` - The path where the item was quarantined
    /// * `Err(String)` - Error message if quarantine failed
    pub fn quarantine(&self, path: &PathBuf) -> Result<PathBuf, String> {
        self.ensure_quarantine_dir()?;

        // Validate path exists
        if !path.exists() {
            return Err(format!("Path does not exist: {}", path.display()));
        }

        // Get metadata before move
        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to get metadata: {}", e))?;

        let quarantine_path = self.generate_quarantine_path(path);

        // Move the file/directory
        fs::rename(path, &quarantine_path)
            .map_err(|e| format!("Failed to move to quarantine: {}", e))?;

        // Save metadata for restoration
        let item = QuarantinedItem {
            path: quarantine_path.clone(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            original_path: path.clone(),
            quarantine_date: Utc::now(),
            size: if metadata.is_dir() {
                self.calculate_dir_size(&quarantine_path)
            } else {
                metadata.len()
            },
            is_directory: metadata.is_dir(),
        };

        // Save item metadata
        self.save_item_metadata(&quarantine_path, &item)?;

        eprintln!(
            "[Quarantine] Moved {} to {}",
            path.display(),
            quarantine_path.display()
        );

        Ok(quarantine_path)
    }

    /// Save metadata for a quarantined item
    fn save_item_metadata(&self, quarantine_path: &Path, item: &QuarantinedItem) -> Result<(), String> {
        let metadata_path = quarantine_path.with_extension("quarantine.json");
        let json = serde_json::to_string_pretty(item)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        fs::write(&metadata_path, json)
            .map_err(|e| format!("Failed to write metadata: {}", e))?;
        Ok(())
    }

    /// Load metadata for a quarantined item
    fn load_item_metadata(&self, quarantine_path: &Path) -> Option<QuarantinedItem> {
        let metadata_path = quarantine_path.with_extension("quarantine.json");
        if metadata_path.exists() {
            fs::read_to_string(&metadata_path)
                .ok()
                .and_then(|json| serde_json::from_str(&json).ok())
        } else {
            None
        }
    }

    /// Restore a quarantined item to its original location
    ///
    /// # Arguments
    /// * `quarantine_path` - Path in quarantine
    /// * `original_path` - Where to restore the item (optional, uses stored original if None)
    ///
    /// # Returns
    /// * `Ok(())` - Item was restored
    /// * `Err(String)` - Error message if restoration failed
    pub fn restore(
        &self,
        quarantine_path: &PathBuf,
        original_path: Option<PathBuf>,
    ) -> Result<(), String> {
        if !quarantine_path.exists() {
            return Err(format!(
                "Quarantine path does not exist: {}",
                quarantine_path.display()
            ));
        }

        // Determine restoration path
        let restore_path = if let Some(path) = original_path {
            path
        } else {
            // Try to load from metadata
            self.load_item_metadata(quarantine_path)
                .map(|item| item.original_path)
                .ok_or_else(|| "No original path found and none provided".to_string())?
        };

        // Check if original location is available
        if restore_path.exists() {
            return Err(format!(
                "Cannot restore: path already exists: {}",
                restore_path.display()
            ));
        }

        // Ensure parent directory exists
        if let Some(parent) = restore_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent directory: {}", e))?;
            }
        }

        // Move back to original location
        fs::rename(quarantine_path, &restore_path)
            .map_err(|e| format!("Failed to restore from quarantine: {}", e))?;

        // Clean up metadata file
        let metadata_path = quarantine_path.with_extension("quarantine.json");
        if metadata_path.exists() {
            let _ = fs::remove_file(&metadata_path);
        }

        eprintln!(
            "[Quarantine] Restored {} to {}",
            quarantine_path.display(),
            restore_path.display()
        );

        Ok(())
    }

    /// Clean up old quarantined items that have exceeded the retention period
    ///
    /// # Returns
    /// * `Ok(CleanupStats)` - Statistics about the cleanup operation
    /// * `Err(String)` - Error message if cleanup failed
    pub fn cleanup(&self) -> Result<CleanupStats, String> {
        let mut stats = CleanupStats {
            items_removed: 0,
            bytes_freed: 0,
            errors: 0,
        };

        if !self.base_path.exists() {
            return Ok(stats);
        }

        let cutoff_date = Utc::now() - Duration::days(self.retention_days as i64);

        let entries = fs::read_dir(&self.base_path)
            .map_err(|e| format!("Failed to read quarantine directory: {}", e))?;

        for entry_result in entries {
            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => {
                    stats.errors += 1;
                    continue;
                }
            };

            let path = entry.path();

            // Skip metadata files
            if path
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
            {
                continue;
            }

            // Check if item should be cleaned up
            let should_cleanup = if let Some(item) = self.load_item_metadata(&path) {
                item.quarantine_date < cutoff_date
            } else {
                // No metadata - check file modification time
                entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| {
                        let file_time = chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                            .unwrap_or_else(Utc::now);
                        file_time < cutoff_date
                    })
                    .unwrap_or(false)
            };

            if should_cleanup {
                let size = if path.is_dir() {
                    self.calculate_dir_size(&path)
                } else {
                    fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
                };

                let result = if path.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };

                match result {
                    Ok(_) => {
                        stats.items_removed += 1;
                        stats.bytes_freed += size;

                        // Remove metadata file
                        let metadata_path = path.with_extension("quarantine.json");
                        let _ = fs::remove_file(&metadata_path);

                        eprintln!("[Quarantine] Cleaned up: {}", path.display());
                    }
                    Err(e) => {
                        eprintln!("[Quarantine] Failed to clean up {}: {}", path.display(), e);
                        stats.errors += 1;
                    }
                }
            }
        }

        eprintln!(
            "[Quarantine] Cleanup complete: {} items removed, {} bytes freed",
            stats.items_removed, stats.bytes_freed
        );

        Ok(stats)
    }

    /// List all quarantined items
    ///
    /// # Returns
    /// * `Ok(Vec<QuarantinedItem>)` - List of quarantined items
    /// * `Err(String)` - Error message if listing failed
    pub fn list(&self) -> Result<Vec<QuarantinedItem>, String> {
        let mut items = Vec::new();

        if !self.base_path.exists() {
            return Ok(items);
        }

        let entries = fs::read_dir(&self.base_path)
            .map_err(|e| format!("Failed to read quarantine directory: {}", e))?;

        for entry_result in entries {
            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();

            // Skip metadata files
            if path
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
            {
                continue;
            }

            // Try to load metadata
            if let Some(item) = self.load_item_metadata(&path) {
                items.push(item);
            } else {
                // Create item from filesystem metadata
                if let Ok(metadata) = fs::metadata(&path) {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    // Try to extract original name from quarantine name (timestamp_name)
                    let original_name = name
                        .split('_')
                        .skip(3) // Skip timestamp parts
                        .collect::<Vec<_>>()
                        .join("_");

                    let quarantine_date = metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
                        .unwrap_or_else(Utc::now);

                    items.push(QuarantinedItem {
                        path: path.clone(),
                        name: if original_name.is_empty() {
                            name
                        } else {
                            original_name
                        },
                        original_path: PathBuf::new(), // Unknown
                        quarantine_date,
                        size: if metadata.is_dir() {
                            self.calculate_dir_size(&path)
                        } else {
                            metadata.len()
                        },
                        is_directory: metadata.is_dir(),
                    });
                }
            }
        }

        // Sort by quarantine date, newest first
        items.sort_by(|a, b| b.quarantine_date.cmp(&a.quarantine_date));

        Ok(items)
    }

    /// Calculate the total size of a directory
    fn calculate_dir_size(&self, path: &PathBuf) -> u64 {
        walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum()
    }

    /// Check if an item is in quarantine
    pub fn is_quarantined(&self, original_path: &PathBuf) -> bool {
        self.list()
            .map(|items| {
                items
                    .iter()
                    .any(|item| item.original_path == *original_path)
            })
            .unwrap_or(false)
    }

    /// Get a quarantined item by its original path
    pub fn get_by_original_path(&self, original_path: &PathBuf) -> Option<QuarantinedItem> {
        self.list()
            .ok()?
            .into_iter()
            .find(|item| item.original_path == *original_path)
    }

    /// Permanently delete a quarantined item (bypassing retention)
    pub fn permanent_delete(&self, quarantine_path: &PathBuf) -> Result<(), String> {
        if !quarantine_path.exists() {
            return Err(format!(
                "Quarantine path does not exist: {}",
                quarantine_path.display()
            ));
        }

        if !quarantine_path.starts_with(&self.base_path) {
            return Err("Path is not in quarantine directory".to_string());
        }

        let result = if quarantine_path.is_dir() {
            fs::remove_dir_all(quarantine_path)
        } else {
            fs::remove_file(quarantine_path)
        };

        result.map_err(|e| format!("Failed to permanently delete: {}", e))?;

        // Remove metadata file
        let metadata_path = quarantine_path.with_extension("quarantine.json");
        let _ = fs::remove_file(&metadata_path);

        eprintln!(
            "[Quarantine] Permanently deleted: {}",
            quarantine_path.display()
        );

        Ok(())
    }
}

impl Default for QuarantineManager {
    fn default() -> Self {
        Self::new().expect("Failed to create default QuarantineManager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_manager() -> (QuarantineManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let quarantine_path = temp_dir.path().join("quarantine");
        let manager = QuarantineManager::with_config(quarantine_path, 30);
        (manager, temp_dir)
    }

    #[test]
    fn test_quarantine_file() {
        let (manager, temp_dir) = create_test_manager();

        // Create a test file
        let test_file = temp_dir.path().join("test.txt");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"Hello, World!").unwrap();

        // Quarantine it
        let quarantine_path = manager.quarantine(&test_file).unwrap();

        // Original should not exist
        assert!(!test_file.exists());

        // Quarantine path should exist
        assert!(quarantine_path.exists());
    }

    #[test]
    fn test_restore_file() {
        let (manager, temp_dir) = create_test_manager();

        // Create and quarantine a file
        let test_file = temp_dir.path().join("restore_test.txt");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"Restore me!").unwrap();

        let quarantine_path = manager.quarantine(&test_file).unwrap();

        // Restore it
        manager.restore(&quarantine_path, None).unwrap();

        // Original should exist again
        assert!(test_file.exists());
        assert!(!quarantine_path.exists());
    }

    #[test]
    fn test_list_quarantine() {
        let (manager, temp_dir) = create_test_manager();

        // Create and quarantine multiple files
        for i in 0..3 {
            let test_file = temp_dir.path().join(format!("list_test_{}.txt", i));
            File::create(&test_file).unwrap();
            manager.quarantine(&test_file).unwrap();
        }

        let items = manager.list().unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_permanent_delete() {
        let (manager, temp_dir) = create_test_manager();

        let test_file = temp_dir.path().join("delete_test.txt");
        File::create(&test_file).unwrap();

        let quarantine_path = manager.quarantine(&test_file).unwrap();
        manager.permanent_delete(&quarantine_path).unwrap();

        assert!(!quarantine_path.exists());
    }
}
