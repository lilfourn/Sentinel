//! Cycle detection for drag-and-drop operations.
//!
//! Prevents operations that would create infinite directory cycles, such as:
//! - Dropping a directory into itself
//! - Dropping a directory into one of its descendants

use std::path::{Path, PathBuf};

/// Errors that can occur during cycle detection
#[derive(Debug, Clone)]
pub enum CycleError {
    /// Attempting to drop a directory into itself
    SameDirectory(PathBuf),
    /// Attempting to drop a directory into one of its descendants
    TargetIsDescendant { source: PathBuf, target: PathBuf },
    /// Target is one of the items being dragged (multi-drag edge case)
    TargetIsSource(PathBuf),
    /// Source path does not exist or cannot be resolved
    SourceNotFound(PathBuf),
    /// Target path does not exist or cannot be resolved
    TargetNotFound(PathBuf),
    /// Symlink loop detected (would cause ELOOP)
    SymlinkLoop(PathBuf),
}

impl std::fmt::Display for CycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CycleError::SameDirectory(p) => {
                write!(f, "Cannot move directory into itself: {:?}", p)
            }
            CycleError::TargetIsDescendant { source, target } => {
                write!(
                    f,
                    "Cannot move {:?} into its descendant {:?}",
                    source, target
                )
            }
            CycleError::TargetIsSource(p) => {
                write!(f, "Cannot drop into a selected item: {:?}", p)
            }
            CycleError::SourceNotFound(p) => {
                write!(f, "Source path not found: {:?}", p)
            }
            CycleError::TargetNotFound(p) => {
                write!(f, "Target path not found: {:?}", p)
            }
            CycleError::SymlinkLoop(p) => {
                write!(f, "Symlink loop detected: {:?}", p)
            }
        }
    }
}

impl std::error::Error for CycleError {}

/// Check for symlink loops that would cause ELOOP errors.
///
/// Follows symlink chains and detects:
/// 1. Circular symlink references (A -> B -> A)
/// 2. Excessively deep symlink chains (> MAX_DEPTH)
///
/// # Arguments
/// * `path` - The path to check for symlink loops
///
/// # Returns
/// * `Ok(())` if path is safe (no loops)
/// * `Err(CycleError::SymlinkLoop)` if a loop is detected
pub fn check_symlink_loop(path: &Path) -> Result<(), CycleError> {
    const MAX_DEPTH: usize = 40; // POSIX SYMLOOP_MAX is typically 40

    let mut current = path.to_path_buf();
    let mut visited = std::collections::HashSet::new();

    while current.is_symlink() {
        let key = current.to_string_lossy().to_string();

        // Check for direct loop or excessive depth
        if visited.contains(&key) || visited.len() > MAX_DEPTH {
            return Err(CycleError::SymlinkLoop(path.to_path_buf()));
        }
        visited.insert(key);

        // Read the symlink target
        match std::fs::read_link(&current) {
            Ok(target) => {
                current = if target.is_absolute() {
                    target
                } else {
                    // Resolve relative symlink against parent directory
                    current
                        .parent()
                        .unwrap_or(Path::new("/"))
                        .join(target)
                };
            }
            Err(_) => break, // Can't read link, stop checking
        }
    }

    Ok(())
}

/// Check if moving `source` into `target` would create a cycle.
///
/// A cycle would occur when:
/// 1. `source == target` (dropping directory into itself)
/// 2. `target` is a descendant of `source` (would orphan the parent)
///
/// # Arguments
/// * `source` - The path being moved
/// * `target` - The destination directory
///
/// # Returns
/// * `Ok(())` if the move is safe
/// * `Err(CycleError)` if the move would create a cycle
pub fn would_create_cycle(source: &Path, target: &Path) -> Result<(), CycleError> {
    // Pre-check for symlink loops before canonicalization
    // This prevents hangs/errors from circular symlinks
    check_symlink_loop(source)?;
    check_symlink_loop(target)?;

    // Canonicalize paths to resolve symlinks and normalize
    let source_canonical = source
        .canonicalize()
        .map_err(|_| CycleError::SourceNotFound(source.to_path_buf()))?;
    let target_canonical = target
        .canonicalize()
        .map_err(|_| CycleError::TargetNotFound(target.to_path_buf()))?;

    // Check 1: Same path (dropping into itself)
    if source_canonical == target_canonical {
        return Err(CycleError::SameDirectory(source_canonical));
    }

    // Check 2: Target is a descendant of source
    // e.g., moving /a/b into /a/b/c would orphan /a/b
    if target_canonical.starts_with(&source_canonical) {
        return Err(CycleError::TargetIsDescendant {
            source: source_canonical,
            target: target_canonical,
        });
    }

    Ok(())
}

/// Validate multiple sources for a drop operation.
///
/// Checks:
/// 1. None of the sources would create a cycle when moved to target
/// 2. Target is not one of the sources being moved
///
/// # Arguments
/// * `sources` - Slice of paths being moved
/// * `target` - The destination directory
///
/// # Returns
/// * `Ok(())` if all moves are safe
/// * `Err(CycleError)` if any move would create a cycle
pub fn validate_multi_drop(sources: &[&Path], target: &Path) -> Result<(), CycleError> {
    let target_canonical = target
        .canonicalize()
        .map_err(|_| CycleError::TargetNotFound(target.to_path_buf()))?;

    // First, check if target is one of the sources being dragged
    for source in sources {
        let source_canonical = source
            .canonicalize()
            .map_err(|_| CycleError::SourceNotFound(source.to_path_buf()))?;

        if source_canonical == target_canonical {
            return Err(CycleError::TargetIsSource(target_canonical));
        }
    }

    // Then check each source for cycle creation
    for source in sources {
        would_create_cycle(source, target)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_dirs() -> TempDir {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        // Create test structure:
        // temp/
        //   a/
        //     b/
        //       c/
        //   d/
        fs::create_dir_all(base.join("a/b/c")).unwrap();
        fs::create_dir_all(base.join("d")).unwrap();

        temp
    }

    #[test]
    fn test_same_directory_is_cycle() {
        let temp = setup_test_dirs();
        let dir_a = temp.path().join("a");

        let result = would_create_cycle(&dir_a, &dir_a);
        assert!(matches!(result, Err(CycleError::SameDirectory(_))));
    }

    #[test]
    fn test_descendant_is_cycle() {
        let temp = setup_test_dirs();
        let dir_a = temp.path().join("a");
        let dir_c = temp.path().join("a/b/c");

        // Moving /a into /a/b/c would create a cycle
        let result = would_create_cycle(&dir_a, &dir_c);
        assert!(matches!(result, Err(CycleError::TargetIsDescendant { .. })));
    }

    #[test]
    fn test_sibling_is_safe() {
        let temp = setup_test_dirs();
        let dir_a = temp.path().join("a");
        let dir_d = temp.path().join("d");

        // Moving /a into /d is safe (they're siblings)
        let result = would_create_cycle(&dir_a, &dir_d);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parent_is_safe() {
        let temp = setup_test_dirs();
        let dir_c = temp.path().join("a/b/c");
        let dir_a = temp.path().join("a");

        // Moving /a/b/c into /a is safe (moving up)
        let result = would_create_cycle(&dir_c, &dir_a);
        assert!(result.is_ok());
    }

    #[test]
    fn test_multi_drop_target_is_source() {
        let temp = setup_test_dirs();
        let dir_a = temp.path().join("a");
        let dir_d = temp.path().join("d");

        let sources: Vec<&Path> = vec![dir_a.as_path(), dir_d.as_path()];

        // Dropping [a, d] onto d should fail (target is in sources)
        let result = validate_multi_drop(&sources, &dir_d);
        assert!(matches!(result, Err(CycleError::TargetIsSource(_))));
    }

    #[test]
    fn test_multi_drop_with_descendant() {
        let temp = setup_test_dirs();
        let dir_a = temp.path().join("a");
        let dir_d = temp.path().join("d");
        let dir_c = temp.path().join("a/b/c");

        let sources: Vec<&Path> = vec![dir_a.as_path(), dir_d.as_path()];

        // Dropping [a, d] onto a/b/c should fail (c is descendant of a)
        let result = validate_multi_drop(&sources, &dir_c);
        assert!(matches!(result, Err(CycleError::TargetIsDescendant { .. })));
    }

    #[test]
    fn test_nonexistent_source() {
        let temp = setup_test_dirs();
        let nonexistent = temp.path().join("nonexistent");
        let dir_d = temp.path().join("d");

        let result = would_create_cycle(&nonexistent, &dir_d);
        assert!(matches!(result, Err(CycleError::SourceNotFound(_))));
    }
}
