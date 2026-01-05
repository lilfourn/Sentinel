//! SHA-256 checksum utilities for file integrity verification.

use crate::history::entry::FileChecksum;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, Metadata};
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::SystemTime;

/// Buffer size for reading files (8KB)
const BUFFER_SIZE: usize = 8192;

/// Extract modification time from metadata as unix timestamp
fn get_mtime(metadata: &Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Compute SHA-256 checksum for a file or directory
pub fn compute_file_checksum(path: &Path) -> Result<FileChecksum, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("Failed to read metadata for {}: {}", path.display(), e))?;

    // Directories don't have content checksums
    if metadata.is_dir() {
        return Ok(FileChecksum {
            sha256: String::new(),
            size: 0,
            mtime: get_mtime(&metadata),
            is_directory: true,
        });
    }

    // Open file for reading
    let file = File::open(path)
        .map_err(|e| format!("Failed to open file {}: {}", path.display(), e))?;

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; BUFFER_SIZE];

    // Read and hash in chunks
    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| format!("Failed to read file {}: {}", path.display(), e))?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();

    Ok(FileChecksum {
        sha256: hex::encode(hash),
        size: metadata.len(),
        mtime: get_mtime(&metadata),
        is_directory: false,
    })
}

/// Verify a file matches its expected checksum
#[allow(dead_code)]
pub fn verify_checksum(path: &Path, expected: &FileChecksum) -> Result<bool, String> {
    // Check if file exists
    if !path.exists() {
        return Ok(false);
    }

    let current = compute_file_checksum(path)?;

    // For directories, just check it's still a directory
    if expected.is_directory {
        return Ok(current.is_directory);
    }

    // For files, compare the SHA-256 hash
    Ok(current.sha256 == expected.sha256)
}

/// Compute checksums for a list of paths
#[allow(dead_code)]
pub fn compute_checksums_batch(paths: &[&Path]) -> HashMap<String, FileChecksum> {
    let mut checksums = HashMap::new();

    for path in paths {
        match compute_file_checksum(path) {
            Ok(checksum) => {
                checksums.insert(path.to_string_lossy().to_string(), checksum);
            }
            Err(e) => {
                tracing::warn!("Failed to compute checksum for {}: {}", path.display(), e);
            }
        }
    }

    checksums
}

/// Verify multiple files against their expected checksums
/// Returns a list of paths that don't match
#[allow(dead_code)]
pub fn verify_checksums_batch(
    expected: &HashMap<String, FileChecksum>,
) -> Vec<(String, ChecksumMismatch)> {
    let mut mismatches = Vec::new();

    for (path_str, expected_checksum) in expected {
        let path = Path::new(path_str);

        if !path.exists() {
            mismatches.push((path_str.clone(), ChecksumMismatch::Missing));
            continue;
        }

        match compute_file_checksum(path) {
            Ok(current) => {
                if current.sha256 != expected_checksum.sha256 {
                    mismatches.push((
                        path_str.clone(),
                        ChecksumMismatch::Modified {
                            expected: expected_checksum.sha256.clone(),
                            actual: current.sha256,
                        },
                    ));
                }
            }
            Err(_) => {
                mismatches.push((path_str.clone(), ChecksumMismatch::Unreadable));
            }
        }
    }

    mismatches
}

/// Type of checksum mismatch
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ChecksumMismatch {
    /// File is missing
    Missing,
    /// File content has changed
    Modified { expected: String, actual: String },
    /// File could not be read
    Unreadable,
}

/// Hash a folder path to create a unique filename
pub fn hash_folder_path(folder_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(folder_path.as_bytes());
    let hash = hasher.finalize();
    // Use first 8 bytes (16 hex chars) for filename
    hex::encode(&hash[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_compute_file_checksum() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Write known content
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"Hello, World!").unwrap();

        let checksum = compute_file_checksum(&file_path).unwrap();

        assert!(!checksum.sha256.is_empty());
        assert_eq!(checksum.size, 13); // "Hello, World!" is 13 bytes
        assert!(!checksum.is_directory);
    }

    #[test]
    fn test_compute_directory_checksum() {
        let temp_dir = TempDir::new().unwrap();
        let checksum = compute_file_checksum(temp_dir.path()).unwrap();

        assert!(checksum.sha256.is_empty());
        assert!(checksum.is_directory);
    }

    #[test]
    fn test_verify_checksum() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Write content
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"Test content").unwrap();

        // Compute and verify
        let checksum = compute_file_checksum(&file_path).unwrap();
        assert!(verify_checksum(&file_path, &checksum).unwrap());

        // Modify file
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"Modified content").unwrap();

        // Verify should fail
        assert!(!verify_checksum(&file_path, &checksum).unwrap());
    }

    #[test]
    fn test_hash_folder_path() {
        let hash1 = hash_folder_path("/Users/test/Documents");
        let hash2 = hash_folder_path("/Users/test/Documents");
        let hash3 = hash_folder_path("/Users/test/Downloads");

        // Same path should produce same hash
        assert_eq!(hash1, hash2);

        // Different paths should produce different hashes
        assert_ne!(hash1, hash3);

        // Should be 16 characters (8 bytes in hex)
        assert_eq!(hash1.len(), 16);
    }
}
