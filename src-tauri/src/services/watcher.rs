use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebouncedEvent, Debouncer, RecommendedCache};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Event payload sent to frontend
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangeEvent {
    pub id: String,
    pub event_type: String,
    pub path: String,
    pub file_name: String,
    pub extension: Option<String>,
    pub size: u64,
    pub content_preview: Option<String>,
    pub watched_folder: String,
}

/// Individual folder watcher
#[allow(dead_code)]
pub(crate) struct FolderWatcher {
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    path: PathBuf,
}

/// Watcher state - supports multiple folders
#[derive(Default)]
pub struct WatcherState {
    /// Map of folder path -> watcher
    pub(crate) watchers: HashMap<String, FolderWatcher>,
    /// Legacy single watcher path (for backwards compatibility)
    pub watching_path: Option<PathBuf>,
    pub enabled: bool,
}

/// Global watcher state
pub type WatcherHandle = Arc<Mutex<WatcherState>>;

/// Create a new watcher handle
pub fn create_watcher_handle() -> WatcherHandle {
    Arc::new(Mutex::new(WatcherState::default()))
}

/// Start watching a directory (legacy single-folder mode)
pub fn start_watcher(
    app: AppHandle,
    handle: WatcherHandle,
    path: PathBuf,
) -> Result<(), String> {
    let mut state = handle.lock().unwrap_or_else(|poisoned| {
        eprintln!("Watcher state mutex was poisoned, recovering...");
        poisoned.into_inner()
    });

    // Clear all existing watchers
    state.watchers.clear();

    let path_str = path.to_string_lossy().to_string();
    let watched_folder = path_str.clone();
    let app_clone = app.clone();

    // Create debounced watcher (waits 500ms for file writes to complete)
    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        None,
        move |result: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| {
            match result {
                Ok(events) => {
                    for event in events {
                        handle_file_event(&app_clone, &event, &watched_folder);
                    }
                }
                Err(errors) => {
                    for error in errors {
                        eprintln!("Watcher error: {:?}", error);
                    }
                }
            }
        },
    )
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

    // Start watching the path
    debouncer
        .watch(&path, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch path: {}", e))?;

    state.watchers.insert(path_str, FolderWatcher {
        debouncer,
        path: path.clone(),
    });
    state.watching_path = Some(path);
    state.enabled = true;

    Ok(())
}

/// Add a folder to watch (multi-folder mode)
pub fn add_watched_folder(
    app: AppHandle,
    handle: WatcherHandle,
    path: PathBuf,
) -> Result<(), String> {
    let mut state = handle.lock().unwrap_or_else(|poisoned| {
        eprintln!("Watcher state mutex was poisoned, recovering...");
        poisoned.into_inner()
    });

    let path_str = path.to_string_lossy().to_string();

    // Skip if already watching
    if state.watchers.contains_key(&path_str) {
        return Ok(());
    }

    let watched_folder = path_str.clone();
    let app_clone = app.clone();

    // Create debounced watcher
    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        None,
        move |result: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| {
            match result {
                Ok(events) => {
                    for event in events {
                        handle_file_event(&app_clone, &event, &watched_folder);
                    }
                }
                Err(errors) => {
                    for error in errors {
                        eprintln!("Watcher error: {:?}", error);
                    }
                }
            }
        },
    )
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

    debouncer
        .watch(&path, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch path: {}", e))?;

    state.watchers.insert(path_str, FolderWatcher {
        debouncer,
        path,
    });
    state.enabled = true;

    Ok(())
}

/// Remove a folder from watching
pub fn remove_watched_folder(
    handle: WatcherHandle,
    path: &str,
) -> Result<(), String> {
    let mut state = handle.lock().unwrap_or_else(|poisoned| {
        eprintln!("Watcher state mutex was poisoned, recovering...");
        poisoned.into_inner()
    });

    state.watchers.remove(path);

    // Update enabled state
    if state.watchers.is_empty() {
        state.enabled = false;
        state.watching_path = None;
    }

    Ok(())
}

/// Stop watching all folders
pub fn stop_watcher(handle: WatcherHandle) -> Result<(), String> {
    let mut state = handle.lock().unwrap_or_else(|poisoned| {
        eprintln!("Watcher state mutex was poisoned, recovering...");
        poisoned.into_inner()
    });
    state.watchers.clear();
    state.watching_path = None;
    state.enabled = false;
    Ok(())
}

/// Check if watcher is running
pub fn is_watcher_running(handle: &WatcherHandle) -> bool {
    match handle.lock() {
        Ok(state) => state.enabled && !state.watchers.is_empty(),
        Err(poisoned) => {
            eprintln!("Watcher state mutex was poisoned in is_watcher_running");
            let state = poisoned.into_inner();
            state.enabled && !state.watchers.is_empty()
        }
    }
}

/// Get the path being watched (legacy - returns first path)
pub fn get_watching_path(handle: &WatcherHandle) -> Option<PathBuf> {
    match handle.lock() {
        Ok(state) => state.watching_path.clone(),
        Err(poisoned) => {
            eprintln!("Watcher state mutex was poisoned in get_watching_path");
            poisoned.into_inner().watching_path.clone()
        }
    }
}

/// Get all paths being watched
pub fn get_all_watching_paths(handle: &WatcherHandle) -> Vec<String> {
    match handle.lock() {
        Ok(state) => state.watchers.keys().cloned().collect(),
        Err(poisoned) => {
            eprintln!("Watcher state mutex was poisoned in get_all_watching_paths");
            poisoned.into_inner().watchers.keys().cloned().collect()
        }
    }
}

/// Handle a file event
fn handle_file_event(app: &AppHandle, event: &DebouncedEvent, watched_folder: &str) {
    // Only handle create events for new files
    let is_create = matches!(event.kind, EventKind::Create(_));

    if !is_create {
        return;
    }

    for path in &event.paths {
        // Skip directories
        if path.is_dir() {
            continue;
        }

        // SECURITY: Skip symlinks to prevent escaping watched folder
        if path.is_symlink() {
            continue;
        }

        // SECURITY: Verify file is within watched folder (prevents symlink escape attacks)
        if let (Ok(canonical_path), Ok(canonical_watched)) = (
            path.canonicalize(),
            std::path::Path::new(watched_folder).canonicalize()
        ) {
            if !canonical_path.starts_with(&canonical_watched) {
                eprintln!("Security: Skipping file outside watched folder: {:?}", path);
                continue;
            }
        } else {
            // Can't verify path safety, skip
            continue;
        }

        // Skip temporary files and hidden files
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Skip hidden files, temp files, and partial downloads
        if file_name.starts_with('.')
            || file_name.ends_with(".tmp")
            || file_name.ends_with(".crdownload")
            || file_name.ends_with(".part")
            || file_name.ends_with(".download")
        {
            continue;
        }

        // Get file info (use symlink_metadata to not follow symlinks)
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Double-check it's not a symlink via metadata
        if metadata.file_type().is_symlink() {
            continue;
        }

        // Skip if file is still being written (size is 0)
        if metadata.len() == 0 {
            continue;
        }

        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_string());

        // Read content preview (first 4KB for text files) - pass watched_folder for security check
        let content_preview = read_content_preview(path, &extension, watched_folder);

        let file_event = FileChangeEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: "created".to_string(),
            path: path.to_string_lossy().to_string(),
            file_name,
            extension,
            size: metadata.len(),
            content_preview,
            watched_folder: watched_folder.to_string(),
        };

        // Emit event to frontend
        if let Err(e) = app.emit("sentinel://file-created", &file_event) {
            eprintln!("Failed to emit file event: {}", e);
        }
    }
}

/// Maximum bytes to read for content preview
const MAX_PREVIEW_BYTES: usize = 4096;

/// Read first 4KB of file content for text-based files
/// SECURITY: Validates path is within watched folder before reading
fn read_content_preview(path: &PathBuf, extension: &Option<String>, watched_folder: &str) -> Option<String> {
    // SECURITY: Verify path is within watched folder (redundant check but defense in depth)
    let canonical_path = path.canonicalize().ok()?;
    let canonical_watched = std::path::Path::new(watched_folder).canonicalize().ok()?;

    if !canonical_path.starts_with(&canonical_watched) {
        eprintln!("Security: Attempted read outside watched folder: {:?}", path);
        return None;
    }

    // SECURITY: Reject symlinks
    if path.is_symlink() {
        return None;
    }

    let text_extensions = [
        "txt", "md", "json", "yaml", "yml", "toml", "xml", "html", "css", "js", "ts",
        "jsx", "tsx", "py", "rb", "go", "rs", "java", "c", "cpp", "h", "hpp", "swift",
        "kt", "sh", "bash", "zsh", "csv", "log", "ini", "conf", "config", "env",
    ];

    let ext = extension.as_ref()?.to_lowercase();
    if !text_extensions.contains(&ext.as_str()) {
        return None;
    }

    // SECURITY: Use bounded read - only read MAX_PREVIEW_BYTES, never more
    // This prevents memory exhaustion from large files (DoS attack vector)
    use std::io::Read;

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return None,
    };

    // take() limits reads to exactly MAX_PREVIEW_BYTES - never reads entire file
    let mut reader = file.take(MAX_PREVIEW_BYTES as u64);
    let mut buffer = Vec::with_capacity(MAX_PREVIEW_BYTES);

    if reader.read_to_end(&mut buffer).is_err() {
        return None;
    }

    // SECURITY: Check for binary content (high proportion of non-printable chars)
    let non_printable_count = buffer.iter()
        .filter(|&&b| b < 0x20 && b != b'\n' && b != b'\r' && b != b'\t')
        .count();

    // If more than 10% non-printable, likely binary file
    if non_printable_count > buffer.len() / 10 {
        return None;
    }

    String::from_utf8(buffer).ok()
}
