//! File coordination module for notifying the OS of file system changes.
//!
//! This module ensures that files/folders created by the app are properly
//! tracked by the operating system (Finder/iCloud on macOS, Explorer on Windows).

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::*;

// Fallback for non-macOS platforms (Linux, Windows without shell notifications)
#[cfg(not(target_os = "macos"))]
pub fn notify_file_created(_path: &std::path::Path) -> Result<(), String> {
    // No-op on unsupported platforms
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn notify_directory_created(_path: &std::path::Path) -> Result<(), String> {
    // No-op on unsupported platforms
    Ok(())
}
