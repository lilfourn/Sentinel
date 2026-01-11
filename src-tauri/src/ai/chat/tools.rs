//! Chat-specific tools for the ReAct agent
//!
//! Tools:
//! - search_hybrid: Semantic + keyword search using LocalVectorIndex
//! - read_file: Read file contents
//! - inspect_pattern: Sample files from hologram pattern
//! - list_directory: List directory contents
//! - shell: Execute safe shell commands (allowlist only)
//! - grep: Search file contents with regex

use super::tools_terminal::{execute_bash, execute_grep, execute_shell, get_terminal_tools};
use crate::ai::grok::document_parser::{is_parseable, parse_document};
use crate::security::{safe_regex, PathValidator};
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Directories to skip during search (large caches, build outputs, etc.)
const EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".cache",
    ".npm",
    ".cargo",
    "Library/Caches",
    "Library/Application Support",
    "target",
    "build",
    "dist",
    ".venv",
    "__pycache__",
    ".Trash",
    "Pods",
    ".gradle",
    ".m2",
    ".pnpm",
    ".yarn",
    "vendor",
    ".next",
    ".nuxt",
];

/// Priority search paths for document-like queries (searched first)
const PRIORITY_PATHS: &[&str] = &[
    "Documents",
    "Desktop",
    "Downloads",
    "Drive",
    "Dropbox",
    "OneDrive",
    "Google Drive",
    "iCloud Drive",
];

/// Document extensions (boost score for these when searching)
const DOC_EXTENSIONS: &[&str] = &[
    "pdf", "docx", "doc", "txt", "rtf", "odt", "pages", "md", "xlsx", "xls", "pptx", "ppt",
];

/// Result of executing a chat tool
pub enum ChatToolResult {
    Success(String),
    Error(String),
}

/// Get tool definitions for the chat agent
pub fn get_chat_tools() -> Vec<Value> {
    let mut tools = vec![
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
    ];

    // Add terminal tools (bash, grep)
    tools.extend(get_terminal_tools());
    tools
}

/// Execute a chat tool
pub async fn execute_chat_tool(name: &str, input: &Value) -> ChatToolResult {
    eprintln!("[ChatTool] Executing: {} with input: {:?}", name, input);

    match name {
        "search_hybrid" => execute_search_hybrid(input).await,
        "read_file" => execute_read_file(input),
        "inspect_pattern" => execute_inspect_pattern(input),
        "list_directory" => execute_list_directory(input),
        // "shell" is the new safe command; "bash" kept for backward compatibility
        "shell" => match execute_shell(input).await {
            Ok(output) => ChatToolResult::Success(output),
            Err(e) => ChatToolResult::Error(e),
        },
        "bash" => match execute_bash(input).await {
            Ok(output) => ChatToolResult::Success(output),
            Err(e) => ChatToolResult::Error(e),
        },
        "grep" => match execute_grep(input).await {
            Ok(output) => ChatToolResult::Success(output),
            Err(e) => ChatToolResult::Error(e),
        },
        _ => ChatToolResult::Error(format!("Unknown tool: {}", name)),
    }
}

/// Tokenize a query into searchable words
/// "cover letters 2024" → ["cover", "letters", "2024"]
fn tokenize_query(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-' || c == '.' || c == ',')
        .filter(|s| s.len() >= 2) // Skip single chars
        .map(|s| s.to_string())
        .collect()
}

/// Capitalize the first letter of a string
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Expand a query into multiple pattern variations for better matching
/// "cover letters" → ["cover letters", "cover_letters", "coverletters", "cover-letters", "CoverLetters"]
fn expand_query_to_patterns(query: &str) -> Vec<String> {
    let base = query.to_lowercase();
    let words: Vec<&str> = base.split_whitespace().collect();

    let mut patterns = vec![base.clone()];

    // Also add individual words as patterns
    for word in &words {
        if word.len() >= 2 && !patterns.contains(&word.to_string()) {
            patterns.push(word.to_string());
        }
    }

    if words.len() > 1 {
        // "cover letters" -> various joined forms
        patterns.push(words.join("_")); // cover_letters
        patterns.push(words.join("")); // coverletters
        patterns.push(words.join("-")); // cover-letters

        // CamelCase: CoverLetters
        let camel: String = words.iter().map(|w| capitalize(w)).collect();
        patterns.push(camel.clone());

        // Also add lowercase camelCase: coverLetters
        if let Some((first, rest)) = words.split_first() {
            let lower_camel =
                first.to_string() + &rest.iter().map(|w| capitalize(w)).collect::<String>();
            patterns.push(lower_camel);
        }
    }

    patterns
}

/// Check if a directory name should be excluded from search
fn should_exclude_dir(name: &str) -> bool {
    EXCLUDED_DIRS.contains(&name)
}

/// Check if a file has a document extension (for score boosting)
fn is_document_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| DOC_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Score how well a filename matches the query tokens
/// Returns the count of tokens that appear in the filename
fn score_filename(name: &str, tokens: &[String]) -> usize {
    let name_lower = name.to_lowercase();
    tokens
        .iter()
        .filter(|t| name_lower.contains(t.as_str()))
        .count()
}

/// Check if query is a glob pattern (contains *, ?, or [])
fn is_glob_pattern(query: &str) -> bool {
    query.contains('*') || query.contains('?') || query.contains('[')
}

/// Convert a glob pattern to a regex pattern
fn glob_to_regex(glob: &str) -> Result<Regex, String> {
    let mut regex = String::from("(?i)^"); // Case insensitive, anchor start

    for c in glob.chars() {
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' => regex.push_str("\\."),
            '[' => regex.push('['),
            ']' => regex.push(']'),
            c => regex.push(c),
        }
    }

    regex.push('$'); // Anchor end
    Regex::new(&regex).map_err(|e| format!("Invalid glob pattern: {}", e))
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
    let file_types: Option<Vec<String>> = input.get("file_types").and_then(|ft| {
        ft.as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_lowercase())).collect())
    });

    let search_dir = directory.unwrap_or(".");
    let search_path = PathBuf::from(search_dir);

    // Security: Validate search path
    let validated_search_path = match PathValidator::validate_for_read(&search_path, None) {
        Ok(p) => p,
        Err(e) => return ChatToolResult::Error(format!("Path validation failed: {}", e)),
    };

    if !validated_search_path.is_dir() {
        return ChatToolResult::Error(format!("Directory not found: {}", search_dir));
    }

    eprintln!("[SearchHybrid] Query: '{}' in '{}'", query, validated_search_path.display());

    // Determine search strategy
    let is_glob = is_glob_pattern(query);
    let glob_regex = if is_glob {
        match glob_to_regex(query) {
            Ok(r) => Some(r),
            Err(e) => return ChatToolResult::Error(e),
        }
    } else {
        None
    };

    // Tokenize for word-based matching
    let tokens = tokenize_query(query);
    if tokens.is_empty() && !is_glob {
        return ChatToolResult::Error("Query too short or contains only special characters".to_string());
    }

    eprintln!("[SearchHybrid] Strategy: {}, tokens: {:?}",
        if is_glob { "glob" } else { "word-match" },
        tokens
    );

    // Results with scores: (path, score)
    let mut scored_results: Vec<(String, usize)> = Vec::new();

    // Expand query into pattern variations for better matching
    let expanded_patterns = expand_query_to_patterns(query);
    eprintln!(
        "[SearchHybrid] Expanded patterns: {:?}",
        expanded_patterns
    );

    #[allow(clippy::too_many_arguments)]
    fn search_recursive(
        dir: &Path,
        tokens: &[String],
        expanded_patterns: &[String],
        glob_regex: &Option<Regex>,
        file_types: &Option<Vec<String>>,
        results: &mut Vec<(String, usize)>,
        max_results: usize,
        depth: usize,
    ) {
        // Increased depth limit for better coverage
        if depth > 15 || results.len() >= max_results * 2 {
            return;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let name_lower = name.to_lowercase();

                // Skip hidden files/dirs
                if name.starts_with('.') {
                    continue;
                }

                // Skip excluded directories (node_modules, .git, caches, etc.)
                if path.is_dir() && should_exclude_dir(&name) {
                    continue;
                }

                // Check file type filter if specified
                if let Some(types) = file_types {
                    if !types.is_empty() {
                        if let Some(ext) = path.extension() {
                            let ext_str = ext.to_string_lossy().to_lowercase();
                            if !types.contains(&ext_str) {
                                // Still recurse into directories
                                if path.is_dir() {
                                    search_recursive(
                                        &path,
                                        tokens,
                                        expanded_patterns,
                                        glob_regex,
                                        file_types,
                                        results,
                                        max_results,
                                        depth + 1,
                                    );
                                }
                                continue;
                            }
                        } else if path.is_file() {
                            // File has no extension but filter requires one
                            continue;
                        }
                    }
                }

                // Calculate match score
                let mut score = if let Some(regex) = glob_regex {
                    // Glob pattern matching
                    if regex.is_match(&name) {
                        10
                    } else {
                        0
                    }
                } else {
                    // Word-based token matching (original)
                    let token_score = score_filename(&name_lower, tokens);

                    // Also check expanded patterns for additional matches
                    let pattern_score = expanded_patterns
                        .iter()
                        .filter(|p| name_lower.contains(p.as_str()))
                        .count();

                    // Take the better score
                    token_score.max(pattern_score)
                };

                // Boost score for document extensions
                if score > 0 && path.is_file() && is_document_extension(&path) {
                    score += 2;
                }

                // Add to results if matched
                if score > 0 && path.is_file() {
                    results.push((path.display().to_string(), score));
                }

                // Also match directories by name (for folders like "Cover Letters")
                if score > 0 && path.is_dir() {
                    // Note: we don't add the dir itself, but we prioritize searching it
                }

                // Recurse into directories
                if path.is_dir() {
                    search_recursive(
                        &path,
                        tokens,
                        expanded_patterns,
                        glob_regex,
                        file_types,
                        results,
                        max_results,
                        depth + 1,
                    );
                }
            }
        }
    }

    // Check if searching from home directory - if so, search priority paths first
    let home_dir = dirs::home_dir();
    let is_home_search = home_dir
        .as_ref()
        .map(|h| validated_search_path == *h)
        .unwrap_or(false);

    if is_home_search {
        eprintln!("[SearchHybrid] Home directory search - prioritizing common document locations");

        // Search priority paths first
        for priority_dir in PRIORITY_PATHS {
            let priority_path = validated_search_path.join(priority_dir);
            if priority_path.is_dir() {
                search_recursive(
                    &priority_path,
                    &tokens,
                    &expanded_patterns,
                    &glob_regex,
                    &file_types,
                    &mut scored_results,
                    max_results,
                    0,
                );

                // If we found enough results in priority paths, stop early
                if scored_results.len() >= max_results {
                    break;
                }
            }
        }
    }

    // If not enough results from priority paths, do full search
    if scored_results.len() < max_results {
        search_recursive(
            &validated_search_path,
            &tokens,
            &expanded_patterns,
            &glob_regex,
            &file_types,
            &mut scored_results,
            max_results,
            0,
        );
    }

    // Sort by score (highest first), then truncate
    scored_results.sort_by(|a, b| b.1.cmp(&a.1));
    scored_results.truncate(max_results);

    eprintln!("[SearchHybrid] Found {} results", scored_results.len());

    // If no filename matches and not a glob, try content search as fallback
    if scored_results.is_empty() && !is_glob {
        eprintln!("[SearchHybrid] No filename matches, trying content search...");

        // Use grep for content search
        let grep_input = json!({
            "pattern": query,
            "path": search_dir,
            "max_results": max_results
        });

        match execute_grep(&grep_input).await {
            Ok(grep_output) => {
                if !grep_output.contains("(Command executed successfully, no output)")
                    && !grep_output.is_empty()
                {
                    return ChatToolResult::Success(format!(
                        "No files with matching names. Content search results:\n{}",
                        grep_output
                    ));
                }
            }
            Err(_) => {
                // Grep failed, continue with empty result
            }
        }

        return ChatToolResult::Success("No files found matching the query.".to_string());
    }

    if scored_results.is_empty() {
        ChatToolResult::Success("No files found matching the query.".to_string())
    } else {
        let formatted: Vec<String> = scored_results
            .iter()
            .map(|(path, score)| {
                if *score > 1 {
                    format!("- {} (matched {} tokens)", path, score)
                } else {
                    format!("- {}", path)
                }
            })
            .collect();

        ChatToolResult::Success(format!(
            "Found {} files:\n{}",
            scored_results.len(),
            formatted.join("\n")
        ))
    }
}

/// Known binary file extensions that cannot be read as text
const BINARY_EXTENSIONS: &[&str] = &[
    // Images
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "svg", "tiff", "tif", "heic", "heif",
    // Audio
    "mp3", "wav", "flac", "aac", "ogg", "m4a", "wma",
    // Video
    "mp4", "avi", "mkv", "mov", "wmv", "flv", "webm",
    // Archives
    "zip", "tar", "gz", "rar", "7z", "bz2", "xz",
    // Executables
    "exe", "dll", "so", "dylib", "app", "dmg", "pkg", "deb", "rpm",
    // Other binary
    "bin", "dat", "db", "sqlite", "sqlite3",
];

/// Check if a file extension indicates a binary file
fn is_binary_extension(ext: Option<&str>) -> bool {
    ext.map(|e| BINARY_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Check if a file extension indicates an image
fn is_image_extension(ext: Option<&str>) -> bool {
    const IMAGE_EXTENSIONS: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "svg", "tiff", "tif", "heic", "heif",
    ];
    ext.map(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
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

    // Security: Validate path using PathValidator
    let path_buf = PathBuf::from(path);
    let validated_path = match PathValidator::validate_for_read(&path_buf, None) {
        Ok(p) => p,
        Err(e) => return ChatToolResult::Error(format!("Path validation failed: {}", e)),
    };

    if validated_path.is_dir() {
        return ChatToolResult::Error("Path is a directory, not a file".to_string());
    }

    let extension = validated_path
        .extension()
        .and_then(|e| e.to_str());

    // Handle images - can't extract text
    if is_image_extension(extension) {
        return ChatToolResult::Success(format!(
            "[Image file: {}]\nThis is an image file. I cannot read its text content, but I can see it was attached to the conversation.",
            validated_path.file_name().unwrap_or_default().to_string_lossy()
        ));
    }

    // Handle parseable documents (PDF, DOCX, etc.) - use document parser
    if is_parseable(extension) {
        eprintln!("[ChatTool] Using document parser for: {}", path);
        match parse_document(&validated_path) {
            Ok(parsed) => {
                let text = &parsed.text;
                let lines: Vec<&str> = text.lines().take(max_lines).collect();
                let truncated = lines.len() < text.lines().count();

                // Get filename from path
                let filename = validated_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "document".to_string());

                let header = format!(
                    "[Document: {} | {} pages | {} words]\n\n",
                    filename,
                    parsed.metadata.page_count.unwrap_or(1),
                    parsed.metadata.word_count.unwrap_or(0)
                );

                let result = if truncated {
                    format!(
                        "{}{}\n\n[Truncated at {} lines]",
                        header,
                        lines.join("\n"),
                        max_lines
                    )
                } else {
                    format!("{}{}", header, lines.join("\n"))
                };

                return ChatToolResult::Success(result);
            }
            Err(e) => {
                return ChatToolResult::Error(format!("Failed to parse document: {}", e));
            }
        }
    }

    // Handle other known binary files
    if is_binary_extension(extension) {
        return ChatToolResult::Error(format!(
            "Cannot read binary file: {}. This file type ({}) is not readable as text.",
            validated_path.file_name().unwrap_or_default().to_string_lossy(),
            extension.unwrap_or("unknown")
        ));
    }

    // Try to read as UTF-8 text
    match fs::read_to_string(&validated_path) {
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
        Err(e) => {
            // Check if it's a UTF-8 error - file might be binary
            let err_str = e.to_string();
            if err_str.contains("UTF-8") || err_str.contains("utf-8") || err_str.contains("valid") {
                ChatToolResult::Error(format!(
                    "Cannot read file as text: {} appears to be a binary file or uses non-UTF-8 encoding.",
                    validated_path.file_name().unwrap_or_default().to_string_lossy()
                ))
            } else {
                ChatToolResult::Error(format!("Failed to read file: {}", e))
            }
        }
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

    // Security: Validate regex using safe_regex to prevent ReDoS
    let regex = match safe_regex(pattern) {
        Ok(r) => r,
        Err(e) => return ChatToolResult::Error(format!("Invalid or unsafe regex: {}", e)),
    };

    // Security: Validate directory path
    let dir_path = PathBuf::from(directory);
    let validated_dir = match PathValidator::validate_for_read(&dir_path, None) {
        Ok(p) => p,
        Err(e) => return ChatToolResult::Error(format!("Path validation failed: {}", e)),
    };

    if !validated_dir.is_dir() {
        return ChatToolResult::Error("Path is not a directory".to_string());
    }

    let mut matches: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(&validated_dir) {
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

    // Security: Validate path
    let dir_path = PathBuf::from(path);
    let validated_path = match PathValidator::validate_for_read(&dir_path, None) {
        Ok(p) => p,
        Err(e) => return ChatToolResult::Error(format!("Path validation failed: {}", e)),
    };

    if !validated_path.is_dir() {
        return ChatToolResult::Error("Path is not a directory".to_string());
    }

    match fs::read_dir(&validated_path) {
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
        assert_eq!(tools.len(), 6); // 4 original + 2 terminal tools (shell, grep)

        // Verify tool names
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"search_hybrid"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"inspect_pattern"));
        assert!(names.contains(&"list_directory"));
        assert!(names.contains(&"shell")); // Renamed from "bash" to "shell"
        assert!(names.contains(&"grep"));
    }
}
