use crate::models::{DirectoryContents, FileEntry, FileMetadata};
use crate::security::cycle_detection::{self, CycleError};
use crate::security::PathValidator;
use std::path::Path;

/// Structured error for directory operations
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryError {
    pub code: String,
    pub message: String,
    pub path: String,
    pub is_permission_error: bool,
}

impl From<DirectoryError> for String {
    fn from(err: DirectoryError) -> Self {
        err.message
    }
}

/// Structured error for drag-drop operations
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DragDropError {
    /// Cannot drop directory into itself
    CycleDetectedSelf { path: String },
    /// Cannot drop directory into its own descendant
    CycleDetectedDescendant { source: String, target: String },
    /// Cannot drop item into another selected item (multi-drag)
    TargetIsSelected { target: String },
    /// File/folder already exists at destination
    NameCollision { name: String, destination: String },
    /// Permission denied
    #[allow(dead_code)]
    PermissionDenied { path: String, message: String },
    /// Source does not exist
    SourceNotFound { path: String },
    /// Target is not a directory
    TargetNotDirectory { path: String },
    /// Protected path
    ProtectedPath { path: String },
    /// Generic IO error
    IoError { message: String },
}

/// Structured error for delete operations
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeleteError {
    /// File or directory does not exist
    NotFound { path: String },
    /// Path is protected and cannot be deleted
    ProtectedPath { path: String },
    /// iCloud file needs to be downloaded first (macOS error -8013)
    ICloudDownloadRequired { path: String, message: String },
    /// Generic IO error
    IoError { message: String },
}

impl From<CycleError> for DragDropError {
    fn from(err: CycleError) -> Self {
        match err {
            CycleError::SameDirectory(p) => DragDropError::CycleDetectedSelf {
                path: p.to_string_lossy().to_string(),
            },
            CycleError::TargetIsDescendant { source, target } => {
                DragDropError::CycleDetectedDescendant {
                    source: source.to_string_lossy().to_string(),
                    target: target.to_string_lossy().to_string(),
                }
            }
            CycleError::TargetIsSource(p) => DragDropError::TargetIsSelected {
                target: p.to_string_lossy().to_string(),
            },
            CycleError::SourceNotFound(p) => DragDropError::SourceNotFound {
                path: p.to_string_lossy().to_string(),
            },
            CycleError::TargetNotFound(p) => DragDropError::SourceNotFound {
                path: p.to_string_lossy().to_string(),
            },
        }
    }
}

/// Read directory contents
#[tauri::command]
pub async fn read_directory(
    path: String,
    show_hidden: Option<bool>,
) -> Result<DirectoryContents, DirectoryError> {
    let path_obj = Path::new(&path);
    let show_hidden = show_hidden.unwrap_or(false);

    if !path_obj.exists() {
        return Err(DirectoryError {
            code: "NOT_FOUND".to_string(),
            message: format!("Path does not exist: {:?}", path_obj),
            path: path.clone(),
            is_permission_error: false,
        });
    }

    if !path_obj.is_dir() {
        return Err(DirectoryError {
            code: "NOT_DIRECTORY".to_string(),
            message: format!("Path is not a directory: {:?}", path_obj),
            path: path.clone(),
            is_permission_error: false,
        });
    }

    let dir_name = path_obj
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path_obj.to_string_lossy().to_string());

    let parent_path = path_obj.parent().map(|p| p.to_string_lossy().to_string());

    let mut entries: Vec<FileEntry> = Vec::new();

    let read_dir = match std::fs::read_dir(path_obj) {
        Ok(rd) => rd,
        Err(e) => {
            let is_permission_error =
                e.raw_os_error() == Some(1) || e.kind() == std::io::ErrorKind::PermissionDenied;

            return Err(DirectoryError {
                code: if is_permission_error {
                    "PERMISSION_DENIED"
                } else {
                    "READ_ERROR"
                }
                .to_string(),
                message: if is_permission_error {
                    "Access denied. Sentinel needs Full Disk Access permission to read this folder. Open System Settings > Privacy & Security > Full Disk Access.".to_string()
                } else {
                    format!("Failed to read directory: {}", e)
                },
                path: path.clone(),
                is_permission_error,
            });
        }
    };

    for entry in read_dir {
        match entry {
            Ok(entry) => {
                match FileEntry::from_path(&entry.path()) {
                    Ok(file_entry) => {
                        // Filter hidden files if not requested
                        if !show_hidden && file_entry.is_hidden {
                            continue;
                        }
                        entries.push(file_entry);
                    }
                    Err(e) => {
                        // Skip files we can't read (permission denied, etc.)
                        eprintln!("Skipping {:?}: {}", entry.path(), e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading directory entry: {}", e);
            }
        }
    }

    // Sort: directories first, then files, alphabetically
    entries.sort_by(|a, b| {
        match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    let total_count = entries.len();

    Ok(DirectoryContents {
        path: path_obj.to_string_lossy().to_string(),
        name: dir_name,
        parent_path,
        entries,
        total_count,
    })
}

/// Get detailed file metadata
#[tauri::command]
pub async fn get_file_metadata(path: String) -> Result<FileMetadata, String> {
    let path = Path::new(&path);

    if !path.exists() {
        return Err(format!("Path does not exist: {:?}", path));
    }

    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let is_symlink = metadata.is_symlink();
    let is_directory = if is_symlink { path.is_dir() } else { metadata.is_dir() };
    let is_file = if is_symlink { path.is_file() } else { metadata.is_file() };

    let extension = if is_file {
        path.extension().map(|e| e.to_string_lossy().to_string())
    } else {
        None
    };

    let mime_type = extension.as_ref().and_then(|ext| {
        mime_guess::from_ext(ext).first().map(|m| m.to_string())
    });

    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);

    let created_at = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);

    let accessed_at = metadata
        .accessed()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);

    let is_readonly = metadata.permissions().readonly();

    Ok(FileMetadata {
        path: path.to_string_lossy().to_string(),
        name,
        size: metadata.len(),
        is_directory,
        is_file,
        is_symlink,
        is_readonly,
        modified_at,
        created_at,
        accessed_at,
        extension,
        mime_type,
    })
}

/// Rename a file or directory
#[tauri::command]
pub async fn rename_file(old_path: String, new_path: String) -> Result<(), String> {
    let old = Path::new(&old_path);
    let new = Path::new(&new_path);

    if !old.exists() {
        return Err(format!("Source path does not exist: {:?}", old));
    }

    if new.exists() {
        return Err(format!("Destination path already exists: {:?}", new));
    }

    if PathValidator::is_protected_path(old) {
        return Err(format!("Cannot rename protected path: {:?}", old));
    }

    std::fs::rename(old, new).map_err(|e| format!("Failed to rename: {}", e))?;

    Ok(())
}

/// Check if a path is an iCloud placeholder file (cloud-only, not downloaded)
#[cfg(target_os = "macos")]
fn is_icloud_placeholder(path: &Path) -> bool {
    // Check for .icloud placeholder files (e.g., ".Document.pdf.icloud")
    if let Some(name) = path.file_name() {
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') && name_str.ends_with(".icloud") {
            return true;
        }
    }
    false
}

#[cfg(not(target_os = "macos"))]
fn is_icloud_placeholder(_path: &Path) -> bool {
    false
}

/// Move a file or directory to trash (safe delete)
#[tauri::command]
pub async fn delete_to_trash(path: String) -> Result<(), DeleteError> {
    let path = Path::new(&path);

    if !path.exists() {
        return Err(DeleteError::NotFound {
            path: path.to_string_lossy().to_string(),
        });
    }

    PathValidator::validate_for_delete(path).map_err(|e| DeleteError::ProtectedPath { path: e })?;

    // Check for iCloud placeholder files on macOS
    if is_icloud_placeholder(path) {
        return Err(DeleteError::ICloudDownloadRequired {
            path: path.to_string_lossy().to_string(),
            message: "This file is stored in iCloud and needs to be downloaded before it can be moved to Trash. Right-click the file in Finder and select 'Download Now'.".to_string(),
        });
    }

    trash::delete(path).map_err(|e| {
        let err_str = e.to_string();
        // Detect iCloud error -8013 from the trash crate error message
        if err_str.contains("-8013") || err_str.contains("needs to be downloaded") {
            DeleteError::ICloudDownloadRequired {
                path: path.to_string_lossy().to_string(),
                message: "This file is stored in iCloud and needs to be downloaded before it can be moved to Trash. Right-click the file in Finder and select 'Download Now'.".to_string(),
            }
        } else {
            DeleteError::IoError { message: err_str }
        }
    })?;

    Ok(())
}

/// Create a new directory
#[tauri::command]
pub async fn create_directory(path: String) -> Result<(), String> {
    let path = Path::new(&path);

    if path.exists() {
        return Err(format!("Path already exists: {:?}", path));
    }

    std::fs::create_dir_all(path).map_err(|e| format!("Failed to create directory: {}", e))?;

    // Notify file system to trigger Finder/iCloud sync
    crate::file_coordination::notify_directory_created(path)?;

    Ok(())
}

/// Create a new empty file
#[tauri::command]
pub async fn create_file(path: String) -> Result<(), String> {
    let path = Path::new(&path);

    if path.exists() {
        return Err(format!("Path already exists: {:?}", path));
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            return Err(format!("Parent directory does not exist: {:?}", parent));
        }
    }

    std::fs::File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;

    // Notify file system to trigger Finder/iCloud sync
    crate::file_coordination::notify_file_created(path)?;

    Ok(())
}

/// Move a file or directory
#[tauri::command]
pub async fn move_file(source: String, destination: String) -> Result<(), String> {
    let src = Path::new(&source);
    let dst = Path::new(&destination);

    if !src.exists() {
        return Err(format!("Source does not exist: {:?}", src));
    }

    if dst.exists() {
        return Err(format!("Destination already exists: {:?}", dst));
    }

    if PathValidator::is_protected_path(src) {
        return Err(format!("Cannot move protected path: {:?}", src));
    }

    // Try rename first (same filesystem), fall back to copy+delete
    if std::fs::rename(src, dst).is_err() {
        if src.is_dir() {
            copy_dir_all(src, dst)?;
        } else {
            std::fs::copy(src, dst).map_err(|e| format!("Failed to copy: {}", e))?;
        }
        if src.is_dir() {
            std::fs::remove_dir_all(src).map_err(|e| format!("Failed to remove source: {}", e))?;
        } else {
            std::fs::remove_file(src).map_err(|e| format!("Failed to remove source: {}", e))?;
        }
    }

    Ok(())
}

/// Copy a file
#[tauri::command]
pub async fn copy_file(source: String, destination: String) -> Result<(), String> {
    let src = Path::new(&source);
    let dst = Path::new(&destination);

    if !src.exists() {
        return Err(format!("Source does not exist: {:?}", src));
    }

    if src.is_dir() {
        copy_dir_all(src, dst)?;
    } else {
        std::fs::copy(src, dst).map_err(|e| format!("Failed to copy: {}", e))?;
    }

    Ok(())
}

/// Get the user's home directory
#[tauri::command]
pub fn get_home_directory() -> Result<String, String> {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Could not determine home directory".to_string())
}

/// Get the user's downloads directory
#[tauri::command]
pub fn get_downloads_directory() -> Result<String, String> {
    dirs::download_dir()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Could not determine downloads directory".to_string())
}

/// Get common user directories
#[tauri::command]
pub fn get_user_directories() -> Result<Vec<(String, String)>, String> {
    let mut dirs_list = Vec::new();

    if let Some(home) = dirs::home_dir() {
        dirs_list.push(("Home".to_string(), home.to_string_lossy().to_string()));
    }
    if let Some(desktop) = dirs::desktop_dir() {
        dirs_list.push(("Desktop".to_string(), desktop.to_string_lossy().to_string()));
    }
    if let Some(documents) = dirs::document_dir() {
        dirs_list.push(("Documents".to_string(), documents.to_string_lossy().to_string()));
    }
    if let Some(downloads) = dirs::download_dir() {
        dirs_list.push(("Downloads".to_string(), downloads.to_string_lossy().to_string()));
    }
    if let Some(pictures) = dirs::picture_dir() {
        dirs_list.push(("Pictures".to_string(), pictures.to_string_lossy().to_string()));
    }
    if let Some(music) = dirs::audio_dir() {
        dirs_list.push(("Music".to_string(), music.to_string_lossy().to_string()));
    }
    if let Some(videos) = dirs::video_dir() {
        dirs_list.push(("Videos".to_string(), videos.to_string_lossy().to_string()));
    }

    Ok(dirs_list)
}

/// Open a file with the system's default application
#[tauri::command]
pub async fn open_file(path: String) -> Result<(), String> {
    let path = Path::new(&path);

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    if path.is_dir() {
        return Err("Cannot open directories with this command".to_string());
    }

    tauri_plugin_opener::open_path(path.to_str().unwrap_or_default(), None::<&str>)
        .map_err(|e| format!("Failed to open file: {}", e))
}

/// Validate a drag-drop operation without executing it.
/// Returns Ok(()) if valid, or specific DragDropError if invalid.
#[tauri::command]
pub async fn validate_drag_drop(
    sources: Vec<String>,
    target: String,
) -> Result<(), DragDropError> {
    let target_path = Path::new(&target);

    // Target must exist
    if !target_path.exists() {
        return Err(DragDropError::SourceNotFound {
            path: target.clone(),
        });
    }

    // Target must be a directory
    if !target_path.is_dir() {
        return Err(DragDropError::TargetNotDirectory {
            path: target.clone(),
        });
    }

    // Convert to Path references
    let source_paths: Vec<std::path::PathBuf> =
        sources.iter().map(|s| std::path::PathBuf::from(s)).collect();
    let source_refs: Vec<&Path> = source_paths.iter().map(|p| p.as_path()).collect();

    // Check for cycles
    cycle_detection::validate_multi_drop(&source_refs, target_path)?;

    // Check each source
    for source_path in &source_paths {
        // Source must exist
        if !source_path.exists() {
            return Err(DragDropError::SourceNotFound {
                path: source_path.to_string_lossy().to_string(),
            });
        }

        // Source cannot be a protected path
        if PathValidator::is_protected_path(source_path) {
            return Err(DragDropError::ProtectedPath {
                path: source_path.to_string_lossy().to_string(),
            });
        }

        // Check for name collision at destination
        if let Some(name) = source_path.file_name() {
            let destination = target_path.join(name);
            if destination.exists() {
                return Err(DragDropError::NameCollision {
                    name: name.to_string_lossy().to_string(),
                    destination: target.clone(),
                });
            }
        }
    }

    Ok(())
}

/// Execute multiple move operations (batch move for drag-drop).
/// Returns the new paths of the moved items.
#[tauri::command]
pub async fn move_files_batch(
    sources: Vec<String>,
    target_directory: String,
) -> Result<Vec<String>, DragDropError> {
    // Validate first
    validate_drag_drop(sources.clone(), target_directory.clone()).await?;

    let target_path = Path::new(&target_directory);
    let mut new_paths = Vec::new();

    for source in &sources {
        let src_path = Path::new(source);
        let file_name = src_path.file_name().ok_or_else(|| DragDropError::IoError {
            message: format!("Invalid source path: {}", source),
        })?;

        let dst_path = target_path.join(file_name);

        // Try rename first (same filesystem), fall back to copy+delete
        if std::fs::rename(src_path, &dst_path).is_err() {
            if src_path.is_dir() {
                copy_dir_all(src_path, &dst_path).map_err(|e| DragDropError::IoError {
                    message: e,
                })?;
                std::fs::remove_dir_all(src_path).map_err(|e| DragDropError::IoError {
                    message: format!("Failed to remove source directory: {}", e),
                })?;
            } else {
                std::fs::copy(src_path, &dst_path).map_err(|e| DragDropError::IoError {
                    message: format!("Failed to copy file: {}", e),
                })?;
                std::fs::remove_file(src_path).map_err(|e| DragDropError::IoError {
                    message: format!("Failed to remove source file: {}", e),
                })?;
            }
        }

        new_paths.push(dst_path.to_string_lossy().to_string());
    }

    Ok(new_paths)
}

/// Execute multiple copy operations (batch copy for drag-drop with Option key).
/// Returns the new paths of the copied items.
#[tauri::command]
pub async fn copy_files_batch(
    sources: Vec<String>,
    target_directory: String,
) -> Result<Vec<String>, DragDropError> {
    // Validate first (same validation as move)
    validate_drag_drop(sources.clone(), target_directory.clone()).await?;

    let target_path = Path::new(&target_directory);
    let mut new_paths = Vec::new();

    for source in &sources {
        let src_path = Path::new(source);
        let file_name = src_path.file_name().ok_or_else(|| DragDropError::IoError {
            message: format!("Invalid source path: {}", source),
        })?;

        let dst_path = target_path.join(file_name);

        if src_path.is_dir() {
            copy_dir_all(src_path, &dst_path).map_err(|e| DragDropError::IoError {
                message: e,
            })?;
        } else {
            std::fs::copy(src_path, &dst_path).map_err(|e| DragDropError::IoError {
                message: format!("Failed to copy file: {}", e),
            })?;
        }

        new_paths.push(dst_path.to_string_lossy().to_string());
    }

    Ok(new_paths)
}

/// Helper function to copy a directory recursively
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("Failed to create directory: {}", e))?;

    for entry in std::fs::read_dir(src).map_err(|e| format!("Failed to read directory: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let ty = entry.file_type().map_err(|e| format!("Failed to get file type: {}", e))?;

        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Failed to copy file: {}", e))?;
        }
    }

    Ok(())
}
