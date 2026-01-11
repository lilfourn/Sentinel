//! VFS Simulator
//!
//! Provides simulation of file operations on the VFS without touching
//! the real filesystem. This enables validation and preview of changes
//! before they are committed.

use std::path::{Path, PathBuf};

use super::graph::{ShadowVFS, VFSError};
use super::node::FileNode;

/// Simulate a move operation on the VFS
///
/// This stages the move and optionally applies it to the in-memory graph.
/// The real filesystem is never touched.
///
/// # Arguments
/// * `vfs` - The shadow VFS to operate on
/// * `src` - Source path to move
/// * `dest` - Destination path
///
/// # Returns
/// * `Ok(())` if the move is valid and staged
/// * `Err(VFSError)` if the move would fail
pub fn simulate_move(vfs: &mut ShadowVFS, src: PathBuf, dest: PathBuf) -> Result<(), VFSError> {
    vfs.stage_move(src, dest)
}

/// Simulate folder creation on the VFS
///
/// # Arguments
/// * `vfs` - The shadow VFS to operate on
/// * `path` - Path of the folder to create
///
/// # Returns
/// * `Ok(())` if the folder can be created
/// * `Err(VFSError)` if creation would fail
pub fn simulate_create_folder(vfs: &mut ShadowVFS, path: PathBuf) -> Result<(), VFSError> {
    vfs.stage_create_folder(path)
}

/// Simulate deletion on the VFS
///
/// # Arguments
/// * `vfs` - The shadow VFS to operate on
/// * `path` - Path to delete
///
/// # Returns
/// * `Ok(())` if deletion is valid
/// * `Err(VFSError)` if deletion would fail
pub fn simulate_delete(vfs: &mut ShadowVFS, path: PathBuf) -> Result<(), VFSError> {
    vfs.stage_delete(path)
}

/// Apply all staged changes to the VFS nodes
///
/// This commits staged operations to the in-memory graph, updating
/// the nodes HashMap to reflect the new state. The real filesystem
/// is still not touched.
///
/// After this operation, the staged operations are cleared.
///
/// # Arguments
/// * `vfs` - The shadow VFS to apply changes to
///
/// # Returns
/// * `Ok(())` if all staged operations were applied
/// * `Err(Vec<VFSError>)` if validation failed (no changes made)
pub fn apply_all_staged(vfs: &mut ShadowVFS) -> Result<(), Vec<VFSError>> {
    // Validate first
    vfs.validate_staged()?;

    // Clone staged operations since we'll be mutating
    let creates: Vec<PathBuf> = vfs.staged_creates().iter().cloned().collect();
    let deletes: Vec<PathBuf> = vfs.staged_deletes().iter().cloned().collect();
    let moves: Vec<(PathBuf, PathBuf)> = vfs.staged_moves().iter().map(|(k, v)| (k.clone(), v.clone())).collect();

    // 1. Apply creates (new folders)
    for path in creates {
        let mut node = FileNode::directory(path.clone());
        node.is_staged = true;

        if let Some(parent) = path.parent() {
            let parent_path = parent.to_path_buf();
            node.parent = Some(parent_path.clone());

            // Update parent's children
            if let Some(parent_node) = vfs.get_mut(&parent_path) {
                parent_node.add_child(path.clone());
            }
        }

        vfs.insert(node);
    }

    // 2. Apply moves
    for (src, dest) in moves {
        // Remove node from old location
        if let Some(mut node) = vfs.remove(&src) {
            // Update the node's path and mark as staged
            let original_path = node.original_path.clone().unwrap_or_else(|| src.clone());
            node.path = dest.clone();
            node.name = dest
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            node.mark_staged(original_path);

            // Update parent reference
            if let Some(parent) = dest.parent() {
                let new_parent = parent.to_path_buf();

                // Remove from old parent's children
                if let Some(old_parent_path) = &node.parent {
                    if let Some(old_parent) = vfs.get_mut(old_parent_path) {
                        old_parent.remove_child(&src);
                    }
                }

                // Add to new parent's children
                if let Some(new_parent_node) = vfs.get_mut(&new_parent) {
                    new_parent_node.add_child(dest.clone());
                }

                node.parent = Some(new_parent);
            }

            // If this is a directory, update children paths recursively
            if node.is_directory() {
                update_children_paths(vfs, &src, &dest);
            }

            // Insert at new location
            vfs.insert(node);
        }
    }

    // 3. Apply deletes
    for path in deletes {
        // Remove from parent's children first
        if let Some(node) = vfs.get(&path) {
            if let Some(parent_path) = node.parent.clone() {
                if let Some(parent) = vfs.get_mut(&parent_path) {
                    parent.remove_child(&path);
                }
            }
        }

        // Recursively remove all descendants if it's a directory
        let descendants = collect_descendants(vfs, &path);
        for descendant in descendants {
            vfs.remove(&descendant);
        }

        vfs.remove(&path);
    }

    // Clear staged operations
    vfs.clear_staged();

    Ok(())
}

/// Recursively update children paths when a parent is moved
fn update_children_paths(vfs: &mut ShadowVFS, old_parent: &Path, new_parent: &Path) {
    // Collect children that need updating
    let children_to_update: Vec<(PathBuf, PathBuf)> = vfs
        .iter()
        .filter_map(|(path, _)| {
            if path.starts_with(old_parent) && path != old_parent {
                // Calculate new path
                let relative = path.strip_prefix(old_parent).ok()?;
                let new_path = new_parent.join(relative);
                Some((path.clone(), new_path))
            } else {
                None
            }
        })
        .collect();

    // Apply updates
    for (old_path, new_path) in children_to_update {
        if let Some(mut node) = vfs.remove(&old_path) {
            node.path = new_path.clone();

            // Update parent reference
            if let Some(parent) = new_path.parent() {
                node.parent = Some(parent.to_path_buf());
            }

            // Update children references if this is a directory
            let old_children = node.children.clone();
            node.children = old_children
                .iter()
                .filter_map(|child| {
                    child
                        .strip_prefix(old_parent)
                        .ok()
                        .map(|relative| new_parent.join(relative))
                })
                .collect();

            vfs.insert(node);
        }
    }
}

/// Collect all descendants of a path
fn collect_descendants(vfs: &ShadowVFS, parent: &PathBuf) -> Vec<PathBuf> {
    vfs.iter()
        .filter_map(|(path, _)| {
            if path.starts_with(parent) && path != parent {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Simulate a batch of operations from an organize plan
///
/// This validates that all operations in a plan can be executed without conflicts.
///
/// # Arguments
/// * `vfs` - The shadow VFS to simulate on
/// * `operations` - List of operations to simulate
///
/// # Returns
/// * `Ok(())` if all operations are valid
/// * `Err(Vec<String>)` list of error messages for failed operations
pub fn simulate_plan(
    vfs: &mut ShadowVFS,
    operations: Vec<SimulatedOperation>,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    for op in operations {
        let result = match op {
            SimulatedOperation::Move { source, destination } => {
                simulate_move(vfs, PathBuf::from(&source), PathBuf::from(&destination))
            }
            SimulatedOperation::CreateFolder { path } => {
                simulate_create_folder(vfs, PathBuf::from(&path))
            }
            SimulatedOperation::Delete { path } => simulate_delete(vfs, PathBuf::from(&path)),
        };

        if let Err(e) = result {
            errors.push(e.to_string());
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Rollback all staged changes without applying them
pub fn rollback_staged(vfs: &mut ShadowVFS) {
    vfs.clear_staged();
}

/// A simulated operation for plan validation
#[derive(Debug, Clone)]
pub enum SimulatedOperation {
    Move {
        source: String,
        destination: String,
    },
    CreateFolder {
        path: String,
    },
    Delete {
        path: String,
    },
}

impl SimulatedOperation {
    /// Create a move operation
    pub fn move_op(source: impl Into<String>, destination: impl Into<String>) -> Self {
        Self::Move {
            source: source.into(),
            destination: destination.into(),
        }
    }

    /// Create a create folder operation
    pub fn create_folder(path: impl Into<String>) -> Self {
        Self::CreateFolder { path: path.into() }
    }

    /// Create a delete operation
    pub fn delete(path: impl Into<String>) -> Self {
        Self::Delete { path: path.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_vfs() -> ShadowVFS {
        let mut vfs = ShadowVFS::new(PathBuf::from("/root"));

        // Create directory structure
        let mut docs = FileNode::directory(PathBuf::from("/root/docs"));
        docs.parent = Some(PathBuf::from("/root"));

        let mut archive = FileNode::directory(PathBuf::from("/root/archive"));
        archive.parent = Some(PathBuf::from("/root"));

        let mut readme = FileNode::file(PathBuf::from("/root/docs/readme.txt"));
        readme.parent = Some(PathBuf::from("/root/docs"));
        readme.size = 1024;

        docs.add_child(PathBuf::from("/root/docs/readme.txt"));

        // Update root's children
        if let Some(root) = vfs.get_mut(&PathBuf::from("/root")) {
            root.add_child(PathBuf::from("/root/docs"));
            root.add_child(PathBuf::from("/root/archive"));
        }

        vfs.insert(docs);
        vfs.insert(archive);
        vfs.insert(readme);

        vfs
    }

    #[test]
    fn test_simulate_and_apply_move() {
        let mut vfs = create_test_vfs();

        // Simulate moving readme.txt to archive
        simulate_move(
            &mut vfs,
            PathBuf::from("/root/docs/readme.txt"),
            PathBuf::from("/root/archive/readme.txt"),
        )
        .unwrap();

        // Apply staged changes
        apply_all_staged(&mut vfs).unwrap();

        // Verify move was applied
        assert!(vfs.get(&PathBuf::from("/root/docs/readme.txt")).is_none());
        assert!(vfs
            .get(&PathBuf::from("/root/archive/readme.txt"))
            .is_some());
    }

    #[test]
    fn test_simulate_and_apply_create_folder() {
        let mut vfs = create_test_vfs();

        // Create new folder
        simulate_create_folder(&mut vfs, PathBuf::from("/root/new_folder")).unwrap();

        apply_all_staged(&mut vfs).unwrap();

        let new_folder = vfs.get(&PathBuf::from("/root/new_folder"));
        assert!(new_folder.is_some());
        assert!(new_folder.unwrap().is_directory());
    }

    #[test]
    fn test_simulate_and_apply_delete() {
        let mut vfs = create_test_vfs();

        simulate_delete(&mut vfs, PathBuf::from("/root/docs/readme.txt")).unwrap();

        apply_all_staged(&mut vfs).unwrap();

        assert!(vfs.get(&PathBuf::from("/root/docs/readme.txt")).is_none());
    }

    #[test]
    fn test_rollback_staged() {
        let mut vfs = create_test_vfs();

        simulate_delete(&mut vfs, PathBuf::from("/root/docs/readme.txt")).unwrap();

        // Rollback instead of applying
        rollback_staged(&mut vfs);

        // File should still exist
        assert!(vfs.get(&PathBuf::from("/root/docs/readme.txt")).is_some());
    }

    #[test]
    fn test_simulate_plan() {
        let mut vfs = create_test_vfs();

        let operations = vec![
            SimulatedOperation::create_folder("/root/projects"),
            SimulatedOperation::move_op("/root/docs/readme.txt", "/root/projects/readme.txt"),
        ];

        let result = simulate_plan(&mut vfs, operations);
        assert!(result.is_ok());
    }

    #[test]
    fn test_simulate_plan_with_errors() {
        let mut vfs = create_test_vfs();

        let operations = vec![
            // This will fail - moving to existing path
            SimulatedOperation::move_op("/root/docs", "/root/archive"),
        ];

        let result = simulate_plan(&mut vfs, operations);
        assert!(result.is_err());
        assert!(!result.unwrap_err().is_empty());
    }
}
