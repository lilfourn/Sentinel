//! macOS file coordination implementation.
//!
//! Uses NSFileCoordinator to properly notify iCloud and Finder of file system changes.
//! This is the Apple-recommended way to coordinate file operations with cloud services.
//!
//! Strategies used (in order):
//! 1. NSFileCoordinator - proper Apple API for iCloud coordination
//! 2. Shell `touch` command - fallback for FSEvents triggering
//! 3. filetime crate - last resort fallback

use std::ffi::CString;
use std::path::Path;
use std::process::Command;
use std::ptr;

use block2::StackBlock;
use filetime::{set_file_mtime, FileTime};
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};

/// NSFileCoordinatorWritingOptions
const NS_FILE_COORDINATOR_WRITING_FOR_MERGING: usize = 1;

/// Coordinate file creation using NSFileCoordinator.
/// This notifies iCloud daemon and Finder that a file was created.
fn coordinate_with_ns_file_coordinator(path: &Path) -> Result<(), String> {
    let path_str = path.to_str().ok_or("Invalid UTF-8 in path")?;
    let c_path = CString::new(path_str).map_err(|_| "Path contains null byte")?;

    unsafe {
        // Get NSString class
        let ns_string_class =
            AnyClass::get("NSString").ok_or("NSString class not found - is Foundation loaded?")?;

        // Create NSString from path
        let ns_path: *mut AnyObject =
            msg_send![ns_string_class, stringWithUTF8String: c_path.as_ptr()];
        if ns_path.is_null() {
            return Err("Failed to create NSString from path".into());
        }

        // Get NSURL class
        let nsurl_class =
            AnyClass::get("NSURL").ok_or("NSURL class not found - is Foundation loaded?")?;

        // Create NSURL from path string
        let url: *mut AnyObject = msg_send![nsurl_class, fileURLWithPath: ns_path];
        if url.is_null() {
            return Err("Failed to create NSURL from path".into());
        }

        // Get NSFileCoordinator class
        let coordinator_class = AnyClass::get("NSFileCoordinator")
            .ok_or("NSFileCoordinator class not found - is Foundation loaded?")?;

        // Create NSFileCoordinator instance (alloc + initWithFilePresenter:nil)
        let coordinator_alloc: *mut AnyObject = msg_send![coordinator_class, alloc];
        if coordinator_alloc.is_null() {
            return Err("Failed to allocate NSFileCoordinator".into());
        }

        let coordinator: *mut AnyObject =
            msg_send![coordinator_alloc, initWithFilePresenter: ptr::null::<AnyObject>()];
        if coordinator.is_null() {
            return Err("Failed to initialize NSFileCoordinator".into());
        }

        // Create the accessor block
        // This block is called by the coordinator to perform the "write" operation
        // Since the file already exists, we don't need to do anything inside
        let block = StackBlock::new(|_new_url: *mut AnyObject| {
            // File already created - this just signals to the system
            // that we're done with a coordinated write
        });
        let block = block.copy();

        // Call coordinateWritingItemAtURL:options:error:byAccessor:
        let mut error: *mut AnyObject = ptr::null_mut();
        let _: () = msg_send![
            coordinator,
            coordinateWritingItemAtURL: url
            options: NS_FILE_COORDINATOR_WRITING_FOR_MERGING
            error: &mut error
            byAccessor: &*block
        ];

        // Check for error
        if !error.is_null() {
            let desc: *mut AnyObject = msg_send![error, localizedDescription];
            if !desc.is_null() {
                let utf8: *const i8 = msg_send![desc, UTF8String];
                if !utf8.is_null() {
                    let err_str = std::ffi::CStr::from_ptr(utf8).to_string_lossy();
                    return Err(format!("NSFileCoordinator error: {}", err_str));
                }
            }
            return Err("NSFileCoordinator failed with unknown error".into());
        }

        eprintln!("[FileCoord] Successfully coordinated file: {}", path_str);
    }

    Ok(())
}

/// Touch a path using the shell `touch` command.
/// This is a reliable way to trigger FSEvents on macOS.
fn shell_touch(path: &Path) -> Result<(), String> {
    let status = Command::new("touch")
        .arg(path)
        .status()
        .map_err(|e| format!("Failed to run touch command: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("touch command failed with status: {}", status))
    }
}

/// Touch a path using the filetime crate.
fn touch_mtime(path: &Path) -> Result<(), String> {
    let now = std::time::SystemTime::now();
    set_file_mtime(path, FileTime::from_system_time(now))
        .map_err(|e| format!("Failed to set mtime: {}", e))
}

/// Notify the system that a file was created.
///
/// Uses multiple strategies to ensure the file is properly tracked:
/// 1. NSFileCoordinator - Apple's official API for iCloud coordination
/// 2. Shell touch command - triggers FSEvents
/// 3. Touch parent directory - helps Finder recognize the new file
pub fn notify_file_created(path: &Path) -> Result<(), String> {
    // Strategy 1: Use NSFileCoordinator (most proper for iCloud)
    if let Err(e) = coordinate_with_ns_file_coordinator(path) {
        eprintln!(
            "[FileCoord] NSFileCoordinator failed: {}, trying shell touch",
            e
        );

        // Strategy 2: Fallback to shell touch
        if let Err(e2) = shell_touch(path) {
            eprintln!(
                "[FileCoord] shell_touch failed: {}, trying filetime",
                e2
            );

            // Strategy 3: Last resort - filetime crate
            touch_mtime(path)?;
        }
    }

    // Also touch the parent directory to help Finder recognize changes
    if let Some(parent) = path.parent() {
        if parent.exists() {
            // Use shell touch on parent (ignore errors)
            let _ = shell_touch(parent);
        }
    }

    Ok(())
}

/// Notify the system that a directory was created.
///
/// Uses the same strategies as file creation.
pub fn notify_directory_created(path: &Path) -> Result<(), String> {
    notify_file_created(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_coordinate_temp_file() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("sentinel_test_coord.txt");

        // Create a test file
        fs::write(&test_file, "test content").expect("Failed to create test file");

        // Try to coordinate it
        let result = notify_file_created(&test_file);

        // Clean up
        let _ = fs::remove_file(&test_file);

        assert!(result.is_ok(), "Coordination failed: {:?}", result);
    }
}
