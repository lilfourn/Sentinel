pub mod command_sandbox;
pub mod cycle_detection;
pub mod regex_validator;

use regex::Regex;
use std::path::{Component, Path, PathBuf};

pub use command_sandbox::{AllowedCommand, CommandSandbox, CommandSandboxError};
pub use regex_validator::{safe_regex, validate_regex_complexity, RegexValidationError};

/// Security validator for path operations
pub struct PathValidator;

/// Command validator for shell operations
#[allow(dead_code)]
pub struct CommandValidator;

impl PathValidator {
    /// Check if a path is protected and should not be modified
    pub fn is_protected_path(path: &Path) -> bool {
        let protected_paths: Vec<PathBuf> = vec![
            PathBuf::from("/"),
            PathBuf::from("/System"),
            PathBuf::from("/usr"),
            PathBuf::from("/bin"),
            PathBuf::from("/sbin"),
            PathBuf::from("/Library"),
            PathBuf::from("/Applications"),
            PathBuf::from("/private"),
            PathBuf::from("/var"),
            // Windows system paths
            PathBuf::from("C:\\Windows"),
            PathBuf::from("C:\\Program Files"),
            PathBuf::from("C:\\Program Files (x86)"),
        ];

        // Get canonical path if possible
        let check_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        for protected in &protected_paths {
            if check_path == *protected {
                return true;
            }
            // Only protect the root of these paths, not subdirectories we own
            if check_path.starts_with(protected) {
                // Allow user directories within home
                if let Some(home) = dirs::home_dir() {
                    if check_path.starts_with(&home) {
                        return false;
                    }
                }
                // Block if it's a direct child of a protected path
                if check_path.parent() == Some(protected) {
                    return true;
                }
            }
        }

        // Block home directory itself (but not subdirectories)
        if let Some(home) = dirs::home_dir() {
            if check_path == home {
                return true;
            }
        }

        false
    }

    /// Check if a path is within allowed user directories
    #[allow(dead_code)]
    pub fn is_allowed_path(path: &Path) -> bool {
        if let Some(home) = dirs::home_dir() {
            let allowed_dirs = [
                home.join("Downloads"),
                home.join("Documents"),
                home.join("Desktop"),
                home.join("Pictures"),
                home.join("Music"),
                home.join("Videos"),
                home.join("Movies"),
            ];

            let check_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

            for allowed in &allowed_dirs {
                if check_path.starts_with(allowed) {
                    return true;
                }
            }
        }

        // Also allow any path that's not protected
        !Self::is_protected_path(path)
    }

    /// Validate a path for delete operations (more strict)
    pub fn validate_for_delete(path: &Path) -> Result<(), String> {
        if Self::is_protected_path(path) {
            return Err(format!("Cannot delete protected path: {:?}", path));
        }

        // Don't allow deleting home directory
        if let Some(home) = dirs::home_dir() {
            if path == home {
                return Err("Cannot delete home directory".to_string());
            }
        }

        Ok(())
    }

    /// Validate a path for reading operations
    ///
    /// Ensures the path:
    /// - Exists
    /// - Is not a protected system path
    /// - Is within the optional boundary directory (if specified)
    ///
    /// # Arguments
    /// * `path` - The path to validate
    /// * `boundary` - Optional directory that the path must be contained within
    ///
    /// # Returns
    /// * `Ok(PathBuf)` - The canonicalized, validated path
    /// * `Err(String)` - Error message if validation fails
    pub fn validate_for_read(path: &Path, boundary: Option<&Path>) -> Result<PathBuf, String> {
        // Canonicalize to resolve .. and symlinks
        let canonical = path
            .canonicalize()
            .map_err(|_| format!("Path does not exist or cannot be resolved: {}", path.display()))?;

        // Check protected paths
        if Self::is_protected_path(&canonical) {
            return Err(format!(
                "Access to protected path denied: {}",
                canonical.display()
            ));
        }

        // If boundary specified, ensure path is within it
        if let Some(boundary) = boundary {
            let boundary_canonical = boundary
                .canonicalize()
                .map_err(|_| format!("Boundary path invalid: {}", boundary.display()))?;

            if !canonical.starts_with(&boundary_canonical) {
                return Err(format!(
                    "Path traversal detected: {} escapes boundary {}",
                    canonical.display(),
                    boundary_canonical.display()
                ));
            }
        }

        Ok(canonical)
    }

    /// Validate a path for write/move operations (stricter than read)
    ///
    /// For write operations, the parent directory must exist even if the target
    /// file doesn't exist yet.
    ///
    /// # Arguments
    /// * `path` - The target path for writing
    /// * `boundary` - Optional directory that the path must be contained within
    ///
    /// # Returns
    /// * `Ok(PathBuf)` - The validated path (canonicalized where possible)
    /// * `Err(String)` - Error message if validation fails
    pub fn validate_for_write(path: &Path, boundary: Option<&Path>) -> Result<PathBuf, String> {
        // For writes, parent must exist even if file doesn't yet
        let parent = path
            .parent()
            .ok_or_else(|| format!("Invalid path: no parent directory: {}", path.display()))?;

        let parent_canonical = parent
            .canonicalize()
            .map_err(|_| format!("Parent directory does not exist: {}", parent.display()))?;

        if Self::is_protected_path(&parent_canonical) {
            return Err(format!(
                "Cannot write to protected path: {}",
                parent_canonical.display()
            ));
        }

        // Boundary check on parent
        if let Some(boundary) = boundary {
            let boundary_canonical = boundary
                .canonicalize()
                .map_err(|_| format!("Boundary path invalid: {}", boundary.display()))?;

            if !parent_canonical.starts_with(&boundary_canonical) {
                return Err(format!(
                    "Path traversal detected: {} escapes boundary {}",
                    parent_canonical.display(),
                    boundary_canonical.display()
                ));
            }
        }

        // Return the path with canonicalized parent + original filename
        let filename = path.file_name().unwrap_or_default();
        Ok(parent_canonical.join(filename))
    }

    /// Validate a destination path for organization rules
    ///
    /// This is specifically for validating `then_move_to` destinations in
    /// organization rules, where we need to ensure the destination doesn't
    /// escape the root directory.
    ///
    /// # Arguments
    /// * `dest` - The destination path string (may be relative or absolute)
    /// * `root` - The root directory that all operations must stay within
    /// * `allow_absolute` - Whether to allow absolute paths
    ///
    /// # Returns
    /// * `Ok(PathBuf)` - The validated, normalized destination path
    /// * `Err(String)` - Error message if validation fails
    pub fn validate_destination(
        dest: &str,
        root: &Path,
        allow_absolute: bool,
    ) -> Result<PathBuf, String> {
        let dest_path = if dest.starts_with('/') {
            if !allow_absolute {
                return Err(format!(
                    "Absolute paths not allowed in destination: {}",
                    dest
                ));
            }
            PathBuf::from(dest)
        } else {
            root.join(dest)
        };

        // Normalize the path to resolve .. without requiring existence
        let normalized = Self::normalize_path(&dest_path)?;

        // Get the canonical root (must exist)
        let root_canonical = root
            .canonicalize()
            .map_err(|_| format!("Root path invalid: {}", root.display()))?;

        // Check the normalized path doesn't escape root
        // We need to compare the normalized path against root
        if !normalized.starts_with(&root_canonical) {
            return Err(format!(
                "Destination escapes root directory: {} is not under {}",
                normalized.display(),
                root_canonical.display()
            ));
        }

        // Check against protected paths
        if Self::is_protected_path(&normalized) {
            return Err(format!(
                "Cannot use protected path as destination: {}",
                normalized.display()
            ));
        }

        Ok(normalized)
    }

    /// Normalize a path by resolving . and .. components without requiring
    /// the path to exist.
    ///
    /// This is useful for validating destination paths that may not exist yet.
    fn normalize_path(path: &Path) -> Result<PathBuf, String> {
        let mut normalized = PathBuf::new();

        for component in path.components() {
            match component {
                Component::ParentDir => {
                    // Pop the last component, but not past root
                    if !normalized.pop() {
                        // If we can't pop, we're trying to go above root
                        return Err(format!(
                            "Path traversal: too many parent references in {}",
                            path.display()
                        ));
                    }
                }
                Component::CurDir => {
                    // Skip . components
                }
                Component::Normal(name) => {
                    normalized.push(name);
                }
                Component::RootDir => {
                    normalized.push(Component::RootDir);
                }
                Component::Prefix(prefix) => {
                    normalized.push(prefix.as_os_str());
                }
            }
        }

        Ok(normalized)
    }

    /// Check if a path is a symlink
    ///
    /// Uses `symlink_metadata` to check without following the link.
    pub fn is_symlink(path: &Path) -> bool {
        match std::fs::symlink_metadata(path) {
            Ok(meta) => meta.is_symlink(),
            Err(_) => false,
        }
    }

    /// Ensure a path is not a symlink
    ///
    /// # Arguments
    /// * `path` - The path to check
    /// * `operation` - Description of the operation for error messages
    ///
    /// # Returns
    /// * `Ok(())` if not a symlink
    /// * `Err(String)` if it is a symlink
    pub fn ensure_not_symlink(path: &Path, operation: &str) -> Result<(), String> {
        if Self::is_symlink(path) {
            return Err(format!(
                "Refusing to {} symlink: {}",
                operation,
                path.display()
            ));
        }
        Ok(())
    }
}

#[allow(dead_code)]
impl CommandValidator {
    /// Dangerous command patterns that should be blocked
    const BLOCKED_PATTERNS: &'static [&'static str] = &[
        r"rm\s+-rf\s+/",          // rm -rf /
        r"rm\s+-rf\s+~",          // rm -rf ~
        r"rm\s+-rf\s+\$HOME",     // rm -rf $HOME
        r"rm\s+-rf\s+/home",      // rm -rf /home
        r"rm\s+-rf\s+/Users",     // rm -rf /Users
        r">\s*/dev/",             // redirect to /dev/
        r"dd\s+.*of=/dev/",       // dd to device
        r"mkfs\.",                // format filesystem
        r"chmod\s+-R\s+777\s+/",  // chmod 777 /
        r"chown\s+-R\s+.*\s+/",   // chown root stuff
        r":()\{:|:&\};:",         // fork bomb
        r"\|\s*bash",             // pipe to bash (potential injection)
        r"\|\s*sh\s",             // pipe to sh
        r"curl\s+.*\|\s*bash",    // curl | bash
        r"wget\s+.*\|\s*bash",    // wget | bash
        r"sudo\s+",               // sudo commands
        r"doas\s+",               // doas commands
    ];

    /// Validate a command before execution
    pub fn validate_command(command: &str) -> Result<(), String> {
        let command_lower = command.to_lowercase();

        for pattern in Self::BLOCKED_PATTERNS {
            if let Ok(regex) = Regex::new(pattern) {
                if regex.is_match(&command_lower) {
                    return Err(format!(
                        "Command blocked: matches dangerous pattern '{}'",
                        pattern
                    ));
                }
            }
        }

        // Check for attempts to modify system paths
        let system_paths = ["/bin", "/sbin", "/usr", "/System", "/Library", "/etc"];
        for sys_path in system_paths {
            if command.contains(sys_path) {
                // Allow read operations
                if command.starts_with("ls ")
                    || command.starts_with("cat ")
                    || command.starts_with("head ")
                    || command.starts_with("tail ")
                    || command.starts_with("grep ")
                    || command.starts_with("find ")
                {
                    continue;
                }
                // Block write operations to system paths
                if command.contains("rm ")
                    || command.contains("mv ")
                    || command.contains("cp ")
                    || command.contains(">")
                {
                    return Err(format!(
                        "Cannot modify system path: {}",
                        sys_path
                    ));
                }
            }
        }

        Ok(())
    }

    /// Sanitize a command for safe execution
    pub fn sanitize_command(command: &str) -> String {
        // Remove any null bytes
        let sanitized = command.replace('\0', "");
        // Remove any ANSI escape sequences
        let ansi_regex = Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap();
        ansi_regex.replace_all(&sanitized, "").to_string()
    }
}
