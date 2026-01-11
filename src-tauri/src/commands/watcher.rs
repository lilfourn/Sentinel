use std::path::PathBuf;
use tauri::{AppHandle, State};

use crate::services::watcher::{
    self, is_watcher_running, get_watching_path, get_all_watching_paths, WatcherHandle,
};

/// System directories that should never be watched
#[cfg(target_os = "macos")]
const FORBIDDEN_PREFIXES: &[&str] = &[
    "/System",
    "/Library",
    "/private",
    "/usr",
    "/bin",
    "/sbin",
    "/var",
    "/etc",
    "/dev",
    "/tmp",
    "/cores",
];

#[cfg(target_os = "windows")]
const FORBIDDEN_PREFIXES: &[&str] = &[
    "C:\\Windows",
    "C:\\Program Files",
    "C:\\Program Files (x86)",
    "C:\\ProgramData",
];

#[cfg(target_os = "linux")]
const FORBIDDEN_PREFIXES: &[&str] = &[
    "/usr",
    "/bin",
    "/sbin",
    "/var",
    "/etc",
    "/dev",
    "/proc",
    "/sys",
    "/tmp",
    "/boot",
    "/root",
];

/// Validate that a path is safe to watch
fn validate_watch_path(path: &std::path::Path) -> Result<(), String> {
    // Resolve symlinks and get canonical path
    let canonical = path.canonicalize()
        .map_err(|e| format!("Cannot resolve path: {}", e))?;

    let path_str = canonical.to_string_lossy();

    // Check against forbidden system directories
    for forbidden in FORBIDDEN_PREFIXES {
        if path_str.starts_with(forbidden) {
            return Err(format!("Cannot watch system directory: {}", forbidden));
        }
    }

    // Don't allow watching root
    if canonical.parent().is_none() {
        return Err("Cannot watch root directory".to_string());
    }

    // Verify it's within user's home directory (optional but recommended)
    if let Some(home) = dirs::home_dir() {
        if let Ok(canonical_home) = home.canonicalize() {
            if !canonical.starts_with(&canonical_home) {
                // Allow common system directories that are safe
                let allowed_outside_home = ["/Volumes", "/mnt", "/media"];
                let is_allowed = allowed_outside_home.iter()
                    .any(|prefix| path_str.starts_with(prefix));

                if !is_allowed {
                    return Err("Can only watch directories within your home folder or mounted volumes".to_string());
                }
            }
        }
    }

    Ok(())
}

/// Watcher status response
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherStatus {
    pub enabled: bool,
    pub watching_path: Option<String>,
    pub watching_paths: Vec<String>,
}

/// Start the downloads watcher (legacy single folder)
#[tauri::command]
pub async fn start_downloads_watcher(
    app: AppHandle,
    handle: State<'_, WatcherHandle>,
    path: Option<String>,
) -> Result<(), String> {
    let watch_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        dirs::download_dir().ok_or("Could not determine downloads directory")?
    };

    if !watch_path.exists() {
        return Err(format!("Path does not exist: {}", watch_path.display()));
    }

    if !watch_path.is_dir() {
        return Err(format!("Path is not a directory: {}", watch_path.display()));
    }

    // SECURITY: Validate the path is safe to watch
    validate_watch_path(&watch_path)?;

    watcher::start_watcher(app, handle.inner().clone(), watch_path)?;

    Ok(())
}

/// Add a folder to watch (multi-folder mode)
#[tauri::command]
pub async fn add_watched_folder(
    app: AppHandle,
    handle: State<'_, WatcherHandle>,
    path: String,
) -> Result<(), String> {
    let watch_path = PathBuf::from(&path);

    if !watch_path.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    if !watch_path.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    // SECURITY: Validate the path is safe to watch
    validate_watch_path(&watch_path)?;

    watcher::add_watched_folder(app, handle.inner().clone(), watch_path)?;

    Ok(())
}

/// Remove a folder from watching
#[tauri::command]
pub async fn remove_watched_folder(
    handle: State<'_, WatcherHandle>,
    path: String,
) -> Result<(), String> {
    watcher::remove_watched_folder(handle.inner().clone(), &path)
}

/// Stop the downloads watcher (stops all folders)
#[tauri::command]
pub async fn stop_downloads_watcher(
    handle: State<'_, WatcherHandle>,
) -> Result<(), String> {
    watcher::stop_watcher(handle.inner().clone())
}

/// Get watcher status
#[tauri::command]
pub fn get_watcher_status(handle: State<'_, WatcherHandle>) -> WatcherStatus {
    let watching_paths = get_all_watching_paths(handle.inner());
    WatcherStatus {
        enabled: is_watcher_running(handle.inner()),
        watching_path: get_watching_path(handle.inner())
            .map(|p| p.to_string_lossy().to_string()),
        watching_paths,
    }
}
