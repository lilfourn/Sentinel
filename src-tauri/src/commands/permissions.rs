use std::path::Path;

/// Check if a path is accessible (can we read it?)
#[tauri::command]
pub fn check_path_permission(path: String) -> Result<bool, String> {
    let path = Path::new(&path);

    if !path.exists() {
        return Ok(false);
    }

    // Try to read the directory to check actual permission
    match std::fs::read_dir(path) {
        Ok(_) => Ok(true),
        Err(e) => {
            if e.raw_os_error() == Some(1) {
                // Operation not permitted - TCC issue
                Ok(false)
            } else {
                Err(format!("Error checking path: {}", e))
            }
        }
    }
}

/// Get protected directories with their accessibility status
#[tauri::command]
pub fn get_protected_directories() -> Vec<(String, String, bool)> {
    let mut results = Vec::new();

    let protected_dirs = [
        ("Pictures", dirs::picture_dir()),
        ("Desktop", dirs::desktop_dir()),
        ("Downloads", dirs::download_dir()),
        ("Documents", dirs::document_dir()),
    ];

    for (name, path_opt) in protected_dirs {
        if let Some(path) = path_opt {
            let path_str = path.to_string_lossy().to_string();
            let accessible = std::fs::read_dir(&path).is_ok();
            results.push((name.to_string(), path_str, accessible));
        }
    }

    results
}

/// Open System Preferences to Full Disk Access
#[tauri::command]
pub async fn open_privacy_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // Open Privacy & Security > Full Disk Access directly
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
            .spawn()
            .map_err(|e| format!("Failed to open System Preferences: {}", e))?;
    }
    Ok(())
}
