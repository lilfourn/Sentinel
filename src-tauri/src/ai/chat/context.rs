//! Context hydration module
//!
//! Converts ContextItems from the frontend into text suitable for LLM system prompts.
//! - Files → Read text content (truncated to 20KB)
//! - Folders → V5 Hologram compression
//! - Images → Base64 for vision (future)

use crate::ai::rules::VirtualFile;
use crate::ai::v2::compression::generate_hologram;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Maximum text content size per file (20KB)
const MAX_FILE_CONTENT: usize = 20_000;

/// Maximum context items per request
const MAX_CONTEXT_ITEMS: usize = 10;

/// Maximum folder scan depth
const MAX_SCAN_DEPTH: usize = 3;

/// Maximum files to scan per folder
const MAX_FILES_PER_FOLDER: usize = 10_000;

/// Context item from frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String, // "file" | "folder" | "image"
    pub path: String,
    pub name: String,
    pub strategy: String, // "hologram" | "read" | "vision"
    pub size: Option<u64>,
    pub mime_type: Option<String>,
}

/// Hydrated context ready for LLM
pub struct HydratedContext {
    /// Text to add to the system prompt
    pub system_addition: String,
    /// Images for multimodal requests (base64 encoded)
    pub images: Vec<ImageContext>,
}

/// Image context for multimodal
pub struct ImageContext {
    pub name: String,
    pub base64: String,
    pub mime_type: String,
}

/// Build the system prompt addition from context items
///
/// # Arguments
/// * `context_items` - Items from frontend (files, folders, images)
///
/// # Returns
/// HydratedContext with text for system prompt and images for multimodal
pub fn hydrate_context(context_items: &[ContextItem]) -> Result<HydratedContext, String> {
    let mut sections: Vec<String> = Vec::new();
    let mut images: Vec<ImageContext> = Vec::new();

    // Limit context items
    let items = if context_items.len() > MAX_CONTEXT_ITEMS {
        eprintln!(
            "[ChatContext] Limiting context from {} to {} items",
            context_items.len(),
            MAX_CONTEXT_ITEMS
        );
        &context_items[..MAX_CONTEXT_ITEMS]
    } else {
        context_items
    };

    for item in items {
        match item.strategy.as_str() {
            "hologram" => {
                // V5 HOLOGRAM - Folder compression
                match hydrate_folder_hologram(&item.path, &item.name) {
                    Ok(hologram) => sections.push(hologram),
                    Err(e) => {
                        eprintln!("[ChatContext] Hologram error for {}: {}", item.path, e);
                        sections.push(format!(
                            "### Folder: {}\nPath: {}\n\nError generating summary: {}",
                            item.name, item.path, e
                        ));
                    }
                }
            }
            "read" => {
                // Text file content
                match hydrate_file_content(&item.path, &item.name) {
                    Ok(content) => sections.push(content),
                    Err(e) => {
                        eprintln!("[ChatContext] Read error for {}: {}", item.path, e);
                        sections.push(format!(
                            "### File: {}\nPath: {}\n\nError reading file: {}",
                            item.name, item.path, e
                        ));
                    }
                }
            }
            "vision" => {
                // Image for multimodal
                match hydrate_image(&item.path, &item.name, item.mime_type.as_deref()) {
                    Ok(Some(img)) => images.push(img),
                    Ok(None) => {
                        eprintln!("[ChatContext] Skipping large image: {}", item.path);
                    }
                    Err(e) => {
                        eprintln!("[ChatContext] Image error for {}: {}", item.path, e);
                    }
                }
            }
            _ => {
                eprintln!("[ChatContext] Unknown strategy: {}", item.strategy);
            }
        }
    }

    let system_addition = if sections.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n## User-Provided Context\n\n{}",
            sections.join("\n\n---\n\n")
        )
    };

    Ok(HydratedContext {
        system_addition,
        images,
    })
}

/// Generate V5 Hologram for a folder
fn hydrate_folder_hologram(path: &str, name: &str) -> Result<String, String> {
    eprintln!("[ChatContext] Generating hologram for folder: {}", path);

    let folder_path = Path::new(path);
    if !folder_path.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }

    // Scan folder into VirtualFiles
    let mut files: Vec<VirtualFile> = Vec::new();
    scan_folder_recursive(folder_path, &mut files, MAX_SCAN_DEPTH)?;

    // Generate hologram using V5 compression
    let hologram = generate_hologram(&files);

    // Format for LLM
    Ok(format!(
        "### Folder: {}\nPath: {}\n\n{}",
        name,
        path,
        hologram.to_prompt_text()
    ))
}

/// Read text content from a file (truncated)
fn hydrate_file_content(path: &str, name: &str) -> Result<String, String> {
    eprintln!("[ChatContext] Reading file: {}", path);

    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;

    let truncated = if content.len() > MAX_FILE_CONTENT {
        format!(
            "{}...\n\n[Truncated: {} bytes total]",
            &content[..MAX_FILE_CONTENT],
            content.len()
        )
    } else {
        content
    };

    Ok(format!(
        "### File: {}\nPath: {}\n\n```\n{}\n```",
        name, path, truncated
    ))
}

/// Load image as base64 for vision
fn hydrate_image(
    path: &str,
    name: &str,
    mime_type: Option<&str>,
) -> Result<Option<ImageContext>, String> {
    eprintln!("[ChatContext] Loading image: {}", path);

    let bytes = fs::read(path).map_err(|e| format!("Failed to read image {}: {}", path, e))?;

    // Skip very large images (> 5MB)
    if bytes.len() > 5 * 1024 * 1024 {
        eprintln!(
            "[ChatContext] Skipping large image: {} bytes",
            bytes.len()
        );
        return Ok(None);
    }

    let mime = mime_type.unwrap_or("image/png").to_string();

    Ok(Some(ImageContext {
        name: name.to_string(),
        base64: BASE64.encode(&bytes),
        mime_type: mime,
    }))
}

/// Recursively scan folder into VirtualFiles
fn scan_folder_recursive(
    path: &Path,
    files: &mut Vec<VirtualFile>,
    max_depth: usize,
) -> Result<(), String> {
    if max_depth == 0 {
        return Ok(());
    }

    let entries =
        fs::read_dir(path).map_err(|e| format!("Failed to read directory {}: {}", path.display(), e))?;

    for entry in entries.flatten() {
        let entry_path = entry.path();

        match VirtualFile::from_path(&entry_path) {
            Ok(vfile) => {
                let is_dir = vfile.is_directory;
                files.push(vfile);

                // Recurse into subdirectories
                if is_dir {
                    scan_folder_recursive(&entry_path, files, max_depth - 1)?;
                }
            }
            Err(e) => {
                eprintln!(
                    "[ChatContext] Skipping file {}: {}",
                    entry_path.display(),
                    e
                );
            }
        }

        // Limit total files scanned
        if files.len() >= MAX_FILES_PER_FOLDER {
            eprintln!(
                "[ChatContext] Folder scan limit reached: {} files",
                files.len()
            );
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_item_deserialize() {
        let json = r#"{
            "id": "test-1",
            "type": "folder",
            "path": "/test/path",
            "name": "test",
            "strategy": "hologram"
        }"#;

        let item: ContextItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.item_type, "folder");
        assert_eq!(item.strategy, "hologram");
    }
}
