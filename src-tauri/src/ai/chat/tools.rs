//! Chat-specific tools for the ReAct agent
//!
//! Tools:
//! - search_hybrid: Semantic + keyword search using LocalVectorIndex
//! - read_file: Read file contents
//! - inspect_pattern: Sample files from hologram pattern
//! - list_directory: List directory contents

use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

/// Result of executing a chat tool
pub enum ChatToolResult {
    Success(String),
    Error(String),
}

/// Get tool definitions for the chat agent
pub fn get_chat_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "search_hybrid",
            "description": "Search files using semantic understanding and keyword matching. Use when user asks to find files.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language search query (e.g., 'tax documents 2024', 'vacation photos')"
                    },
                    "directory": {
                        "type": "string",
                        "description": "Optional: Limit search to this directory path"
                    },
                    "file_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional: Filter by extensions (e.g., ['pdf', 'docx'])"
                    },
                    "max_results": {
                        "type": "integer",
                        "default": 20,
                        "description": "Maximum results to return"
                    }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "read_file",
            "description": "Read the text content of a file. Use when you need to examine file contents.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the file"
                    },
                    "max_lines": {
                        "type": "integer",
                        "default": 200,
                        "description": "Maximum lines to read"
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "inspect_pattern",
            "description": "Get sample files from a detected hologram pattern. Use to verify pattern contents.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern from hologram (e.g., 'IMG_\\\\d+\\\\.jpg')"
                    },
                    "directory": {
                        "type": "string",
                        "description": "Directory containing the pattern"
                    },
                    "sample_count": {
                        "type": "integer",
                        "default": 3,
                        "description": "Number of sample files to return"
                    }
                },
                "required": ["pattern", "directory"]
            }
        }),
        json!({
            "name": "list_directory",
            "description": "List files and folders in a directory. Use for exploring structure.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list"
                    },
                    "max_items": {
                        "type": "integer",
                        "default": 50,
                        "description": "Maximum items to return"
                    }
                },
                "required": ["path"]
            }
        }),
    ]
}

/// Execute a chat tool
pub async fn execute_chat_tool(name: &str, input: &Value) -> ChatToolResult {
    eprintln!("[ChatTool] Executing: {} with input: {:?}", name, input);

    match name {
        "search_hybrid" => execute_search_hybrid(input).await,
        "read_file" => execute_read_file(input),
        "inspect_pattern" => execute_inspect_pattern(input),
        "list_directory" => execute_list_directory(input),
        _ => ChatToolResult::Error(format!("Unknown tool: {}", name)),
    }
}

async fn execute_search_hybrid(input: &Value) -> ChatToolResult {
    let query = match input.get("query").and_then(|q| q.as_str()) {
        Some(q) => q,
        None => return ChatToolResult::Error("Missing 'query' parameter".to_string()),
    };

    let directory = input.get("directory").and_then(|d| d.as_str());
    let max_results = input
        .get("max_results")
        .and_then(|m| m.as_u64())
        .unwrap_or(20) as usize;

    // Get file types filter
    let file_types: Option<Vec<&str>> = input.get("file_types").and_then(|ft| {
        ft.as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
    });

    // For now, do a simple filename-based search
    // In production, this would use LocalVectorIndex
    let search_dir = directory.unwrap_or(".");
    let search_path = PathBuf::from(search_dir);

    if !search_path.is_dir() {
        return ChatToolResult::Error(format!("Directory not found: {}", search_dir));
    }

    let query_lower = query.to_lowercase();
    let mut results: Vec<String> = Vec::new();

    fn search_recursive(
        dir: &PathBuf,
        query: &str,
        file_types: &Option<Vec<&str>>,
        results: &mut Vec<String>,
        max_results: usize,
        depth: usize,
    ) {
        if depth > 5 || results.len() >= max_results {
            return;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if results.len() >= max_results {
                    break;
                }

                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_lowercase();

                // Check if name matches query
                if name.contains(query) {
                    // Check file type filter
                    if let Some(types) = file_types {
                        if let Some(ext) = path.extension() {
                            let ext_str = ext.to_string_lossy().to_lowercase();
                            if !types.iter().any(|t| t.to_lowercase() == ext_str) {
                                continue;
                            }
                        } else if !types.is_empty() {
                            continue;
                        }
                    }
                    results.push(path.display().to_string());
                }

                // Recurse into directories
                if path.is_dir() && !name.starts_with('.') {
                    search_recursive(&path, query, file_types, results, max_results, depth + 1);
                }
            }
        }
    }

    search_recursive(
        &search_path,
        &query_lower,
        &file_types,
        &mut results,
        max_results,
        0,
    );

    if results.is_empty() {
        ChatToolResult::Success("No files found matching the query.".to_string())
    } else {
        ChatToolResult::Success(format!(
            "Found {} files:\n{}",
            results.len(),
            results.iter().map(|p| format!("- {}", p)).collect::<Vec<_>>().join("\n")
        ))
    }
}

fn execute_read_file(input: &Value) -> ChatToolResult {
    let path = match input.get("path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return ChatToolResult::Error("Missing 'path' parameter".to_string()),
    };

    let max_lines = input
        .get("max_lines")
        .and_then(|m| m.as_u64())
        .unwrap_or(200) as usize;

    // Security: Validate path
    let path_buf = PathBuf::from(path);
    if !path_buf.exists() {
        return ChatToolResult::Error(format!("File not found: {}", path));
    }

    if path_buf.is_dir() {
        return ChatToolResult::Error("Path is a directory, not a file".to_string());
    }

    match fs::read_to_string(&path_buf) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().take(max_lines).collect();
            let truncated = lines.len() < content.lines().count();

            let result = if truncated {
                format!(
                    "{}\n\n[Truncated at {} lines]",
                    lines.join("\n"),
                    max_lines
                )
            } else {
                lines.join("\n")
            };

            ChatToolResult::Success(result)
        }
        Err(e) => ChatToolResult::Error(format!("Failed to read file: {}", e)),
    }
}

fn execute_inspect_pattern(input: &Value) -> ChatToolResult {
    let pattern = match input.get("pattern").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return ChatToolResult::Error("Missing 'pattern' parameter".to_string()),
    };

    let directory = match input.get("directory").and_then(|d| d.as_str()) {
        Some(d) => d,
        None => return ChatToolResult::Error("Missing 'directory' parameter".to_string()),
    };

    let sample_count = input
        .get("sample_count")
        .and_then(|s| s.as_u64())
        .unwrap_or(3) as usize;

    // Compile regex
    let regex = match Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return ChatToolResult::Error(format!("Invalid regex: {}", e)),
    };

    // Find matching files
    let dir_path = PathBuf::from(directory);
    if !dir_path.is_dir() {
        return ChatToolResult::Error("Directory not found".to_string());
    }

    let mut matches: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir_path) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if regex.is_match(name) {
                    matches.push(entry.path().display().to_string());
                    if matches.len() >= sample_count {
                        break;
                    }
                }
            }
        }
    }

    if matches.is_empty() {
        ChatToolResult::Success("No files matched the pattern.".to_string())
    } else {
        ChatToolResult::Success(format!(
            "Sample files matching '{}':\n{}",
            pattern,
            matches.join("\n")
        ))
    }
}

fn execute_list_directory(input: &Value) -> ChatToolResult {
    let path = match input.get("path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return ChatToolResult::Error("Missing 'path' parameter".to_string()),
    };

    let max_items = input
        .get("max_items")
        .and_then(|m| m.as_u64())
        .unwrap_or(50) as usize;

    let dir_path = PathBuf::from(path);
    if !dir_path.is_dir() {
        return ChatToolResult::Error("Path is not a directory".to_string());
    }

    match fs::read_dir(&dir_path) {
        Ok(entries) => {
            let mut items: Vec<(String, bool)> = entries
                .flatten()
                .take(max_items)
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = e.path().is_dir();
                    (name, is_dir)
                })
                .collect();

            // Sort: directories first, then alphabetically
            items.sort_by(|a, b| match (a.1, b.1) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.0.cmp(&b.0),
            });

            let formatted: Vec<String> = items
                .iter()
                .map(|(name, is_dir)| {
                    if *is_dir {
                        format!("📁 {}/", name)
                    } else {
                        format!("📄 {}", name)
                    }
                })
                .collect();

            ChatToolResult::Success(format!(
                "Contents of {}:\n{}",
                path,
                formatted.join("\n")
            ))
        }
        Err(e) => ChatToolResult::Error(format!("Failed to list directory: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_chat_tools() {
        let tools = get_chat_tools();
        assert_eq!(tools.len(), 4);

        // Verify tool names
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"search_hybrid"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"inspect_pattern"));
        assert!(names.contains(&"list_directory"));
    }
}
