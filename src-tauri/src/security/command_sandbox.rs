//! Sandboxed command execution for AI agent tools.
//!
//! This module provides a secure command execution environment that uses an
//! allowlist approach. Only explicitly permitted, read-only commands can be
//! executed, preventing arbitrary command injection attacks.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::PathValidator;

// Shell metacharacter constants moved to reject_shell_metacharacters() method
// to distinguish between never-allowed and user-approvable characters

/// Maximum output size in bytes (10MB)
const MAX_OUTPUT_SIZE: usize = 10 * 1024 * 1024;

/// Allowed commands that can be executed through the sandbox
#[derive(Debug, Clone)]
pub enum AllowedCommand {
    /// List directory contents: ls [args] <path>
    Ls {
        path: PathBuf,
        args: Vec<String>,
    },
    /// Find files: find <path> -name|-iname <pattern>
    Find {
        path: PathBuf,
        name_pattern: String,
        max_depth: Option<u32>,
        case_insensitive: bool,
    },
    /// Get file type info: file <path>
    File {
        path: PathBuf,
    },
    /// Show disk usage: du [args] <path>
    Du {
        path: PathBuf,
        args: Vec<String>,
    },
    /// Count lines/words/chars: wc [args] <path>
    Wc {
        path: PathBuf,
        args: Vec<String>,
    },
    /// Show first N lines: head -n <lines> <path>
    Head {
        path: PathBuf,
        lines: usize,
    },
    /// Show last N lines: tail -n <lines> <path>
    Tail {
        path: PathBuf,
        lines: usize,
    },
    /// Show file contents: cat <path>
    Cat {
        path: PathBuf,
    },
    /// Page through file: less <path> (outputs content, not interactive)
    Less {
        path: PathBuf,
    },
    /// Search with grep: grep [args] <pattern> <path>
    Grep {
        pattern: String,
        path: PathBuf,
        args: Vec<String>,
    },
    /// Search with ripgrep: rg [args] <pattern> <path>
    Rg {
        pattern: String,
        path: PathBuf,
        args: Vec<String>,
    },
    /// Git status (read-only): git status
    GitStatus {
        path: PathBuf,
    },
    /// Git log (read-only): git log -n <count>
    GitLog {
        path: PathBuf,
        count: usize,
    },
}

/// Command sandbox for secure execution
#[derive(Clone)]
pub struct CommandSandbox {
    /// Optional root directory constraint
    root_dir: Option<PathBuf>,
    /// User-approved command patterns (bypass metacharacter check)
    approved_patterns: Vec<String>,
    /// Force mode - bypass metacharacter checks (use with caution)
    force_mode: bool,
}

/// Error type for command sandbox operations
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CommandSandboxError {
    pub message: String,
    pub kind: CommandSandboxErrorKind,
}

#[derive(Debug, Clone)]
pub enum CommandSandboxErrorKind {
    NotAllowed,
    ShellMetacharacter,
    PathTraversal,
    ProtectedPath,
    ParseError,
    ExecutionFailed,
    /// Command blocked but can be approved by user
    NeedsApproval {
        command: String,
        reason: String,
    },
}

impl std::fmt::Display for CommandSandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CommandSandboxError {}

impl CommandSandbox {
    /// Create a new command sandbox
    ///
    /// # Arguments
    /// * `root_dir` - Optional directory to constrain all paths within
    pub fn new(root_dir: Option<PathBuf>) -> Self {
        Self {
            root_dir,
            approved_patterns: Vec::new(),
            force_mode: false,
        }
    }

    /// Enable force mode to bypass metacharacter checks
    ///
    /// WARNING: Use only for user-approved commands
    pub fn with_force_mode(mut self, force: bool) -> Self {
        self.force_mode = force;
        self
    }

    /// Set approved patterns that bypass metacharacter checks
    #[allow(dead_code)]
    pub fn with_approved_patterns(mut self, patterns: Vec<String>) -> Self {
        self.approved_patterns = patterns;
        self
    }

    /// Check if a command matches any approved pattern
    fn is_approved(&self, cmd: &str) -> bool {
        if self.force_mode {
            return true;
        }
        self.approved_patterns
            .iter()
            .any(|pattern| cmd.contains(pattern) || pattern == cmd)
    }

    /// Expand ~ to home directory path
    ///
    /// Shell expansion doesn't happen when we use Command::new() directly,
    /// so we need to handle ~ manually.
    fn expand_tilde(cmd: &str) -> String {
        let Some(home) = dirs::home_dir() else {
            return cmd.to_string();
        };
        let home_str = home.to_string_lossy();

        let mut result = String::with_capacity(cmd.len() + 50);
        let mut chars = cmd.chars().peekable();
        let mut prev_was_space_or_start = true;

        while let Some(c) = chars.next() {
            if c == '~' && prev_was_space_or_start {
                // Check if it's ~/ or ~ followed by space/end
                match chars.peek() {
                    Some('/') | Some(' ') | None => {
                        result.push_str(&home_str);
                    }
                    Some('"') => {
                        // Handle quoted paths like "~/"
                        result.push_str(&home_str);
                    }
                    _ => {
                        // ~username style - don't expand, just keep ~
                        result.push(c);
                    }
                }
            } else {
                result.push(c);
            }
            prev_was_space_or_start = c == ' ' || c == '"' || c == '\'';
        }

        result
    }

    /// Parse a user command string into an allowed command
    ///
    /// # Arguments
    /// * `cmd` - The raw command string from user/AI
    ///
    /// # Returns
    /// * `Ok(AllowedCommand)` if the command is in the allowlist
    /// * `Err(CommandSandboxError)` if the command is not allowed
    pub fn parse_command(&self, cmd: &str) -> Result<AllowedCommand, CommandSandboxError> {
        // Expand ~ to home directory (shell doesn't do this when we bypass it)
        let expanded_cmd = Self::expand_tilde(cmd);
        let cmd = expanded_cmd.as_str();

        // First, reject any shell metacharacters (unless approved)
        if !self.is_approved(cmd) {
            Self::reject_shell_metacharacters(cmd)?;
        }

        // Split into parts
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Err(CommandSandboxError {
                message: "Empty command".to_string(),
                kind: CommandSandboxErrorKind::ParseError,
            });
        }

        // Match against allowlist
        match parts[0] {
            "ls" => self.parse_ls(&parts[1..]),
            "find" => self.parse_find(&parts[1..]),
            "file" => self.parse_file(&parts[1..]),
            "du" => self.parse_du(&parts[1..]),
            "wc" => self.parse_wc(&parts[1..]),
            "head" => self.parse_head(&parts[1..]),
            "tail" => self.parse_tail(&parts[1..]),
            "cat" => self.parse_cat(&parts[1..]),
            "less" => self.parse_less(&parts[1..]),
            "grep" => self.parse_grep(&parts[1..]),
            "rg" | "ripgrep" => self.parse_rg(&parts[1..]),
            "git" => self.parse_git(&parts[1..]),
            _ => Err(CommandSandboxError {
                message: format!("Command '{}' is not in the allowlist. Allowed: ls, find, file, du, wc, head, tail, cat, less, grep, rg, git", parts[0]),
                kind: CommandSandboxErrorKind::NotAllowed,
            }),
        }
    }

    /// Execute an allowed command safely
    ///
    /// Uses `std::process::Command` with explicit argument separation
    /// to prevent shell injection.
    pub fn execute(&self, allowed: &AllowedCommand) -> Result<String, CommandSandboxError> {
        let output = match allowed {
            AllowedCommand::Ls { path, args } => {
                self.validate_path(path)?;
                Command::new("ls").args(args).arg(path).output()
            }
            AllowedCommand::Find {
                path,
                name_pattern,
                max_depth,
                case_insensitive,
            } => {
                self.validate_path(path)?;
                let mut cmd = Command::new("find");
                cmd.arg(path);
                if let Some(depth) = max_depth {
                    cmd.args(["-maxdepth", &depth.to_string()]);
                }
                // Use -iname for case-insensitive, -name otherwise
                let name_flag = if *case_insensitive { "-iname" } else { "-name" };
                cmd.args([name_flag, name_pattern]);
                cmd.output()
            }
            AllowedCommand::File { path } => {
                self.validate_path(path)?;
                Command::new("file").arg(path).output()
            }
            AllowedCommand::Du { path, args } => {
                self.validate_path(path)?;
                Command::new("du").args(args).arg(path).output()
            }
            AllowedCommand::Wc { path, args } => {
                self.validate_path(path)?;
                Command::new("wc").args(args).arg(path).output()
            }
            AllowedCommand::Head { path, lines } => {
                self.validate_path(path)?;
                Command::new("head")
                    .args(["-n", &lines.to_string()])
                    .arg(path)
                    .output()
            }
            AllowedCommand::Tail { path, lines } => {
                self.validate_path(path)?;
                Command::new("tail")
                    .args(["-n", &lines.to_string()])
                    .arg(path)
                    .output()
            }
            AllowedCommand::Cat { path } => {
                self.validate_path(path)?;
                Command::new("cat").arg(path).output()
            }
            AllowedCommand::Less { path } => {
                self.validate_path(path)?;
                // Use cat since less is interactive
                Command::new("cat").arg(path).output()
            }
            AllowedCommand::Grep { pattern, path, args } => {
                self.validate_path(path)?;
                // Note: Pattern is passed via .arg() so shell metacharacters are safe
                // (they're just regex syntax, not shell operators)
                Command::new("grep")
                    .args(args)
                    .arg(pattern)
                    .arg(path)
                    .output()
            }
            AllowedCommand::Rg { pattern, path, args } => {
                self.validate_path(path)?;
                // Note: Pattern is passed via .arg() so shell metacharacters are safe
                Command::new("rg")
                    .args(args)
                    .arg(pattern)
                    .arg(path)
                    .output()
            }
            AllowedCommand::GitStatus { path } => {
                self.validate_path(path)?;
                Command::new("git")
                    .current_dir(path)
                    .args(["status", "--porcelain"])
                    .output()
            }
            AllowedCommand::GitLog { path, count } => {
                self.validate_path(path)?;
                Command::new("git")
                    .current_dir(path)
                    .args(["log", "--oneline", "-n", &count.to_string()])
                    .output()
            }
        };

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);

                // Truncate if too large
                let result = if stdout.len() > MAX_OUTPUT_SIZE {
                    format!(
                        "{}...\n\n[OUTPUT TRUNCATED: {} more bytes]",
                        &stdout[..MAX_OUTPUT_SIZE],
                        stdout.len() - MAX_OUTPUT_SIZE
                    )
                } else {
                    stdout.to_string()
                };

                // Include stderr if there was an error
                if !out.status.success() && !stderr.is_empty() {
                    Ok(format!("{}\n\nSTDERR:\n{}", result, stderr))
                } else {
                    Ok(result)
                }
            }
            Err(e) => Err(CommandSandboxError {
                message: format!("Command execution failed: {}", e),
                kind: CommandSandboxErrorKind::ExecutionFailed,
            }),
        }
    }

    /// Reject commands containing shell metacharacters
    ///
    /// Returns `NeedsApproval` for metacharacters that could be safe in certain contexts,
    /// allowing the user to approve them.
    fn reject_shell_metacharacters(input: &str) -> Result<(), CommandSandboxError> {
        // Characters that can be user-approved (used in regex, find expressions, etc.)
        const APPROVABLE_CHARS: &[char] = &['|', '(', ')', '<', '>'];

        // Characters that are never allowed (command injection, code execution)
        const NEVER_ALLOWED_CHARS: &[char] = &[';', '&', '$', '`', '{', '}', '!', '\\', '\n', '\r', '\0'];

        // Check for never-allowed characters first
        for c in NEVER_ALLOWED_CHARS {
            if input.contains(*c) {
                let char_display = match *c {
                    '\n' => "\\n".to_string(),
                    '\r' => "\\r".to_string(),
                    '\0' => "\\0".to_string(),
                    other => other.to_string(),
                };
                return Err(CommandSandboxError {
                    message: format!(
                        "Shell metacharacter '{}' not allowed in commands",
                        char_display
                    ),
                    kind: CommandSandboxErrorKind::ShellMetacharacter,
                });
            }
        }

        // Check for approvable characters - return NeedsApproval
        for c in APPROVABLE_CHARS {
            if input.contains(*c) {
                return Err(CommandSandboxError {
                    message: format!(
                        "Command contains '{}' which requires approval. This may be safe for regex patterns or find expressions.",
                        c
                    ),
                    kind: CommandSandboxErrorKind::NeedsApproval {
                        command: input.to_string(),
                        reason: format!(
                            "Contains '{}' - could be shell operator or valid regex/find syntax",
                            c
                        ),
                    },
                });
            }
        }

        Ok(())
    }

    /// Validate a path is safe to access
    fn validate_path(&self, path: &Path) -> Result<(), CommandSandboxError> {
        // 1. Must be absolute or we resolve relative to root
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(ref root) = self.root_dir {
            root.join(path)
        } else {
            // Without root_dir, require absolute paths
            return Err(CommandSandboxError {
                message: "Relative path without root directory context".to_string(),
                kind: CommandSandboxErrorKind::PathTraversal,
            });
        };

        // 2. Check against protected paths
        if PathValidator::is_protected_path(&abs_path) {
            return Err(CommandSandboxError {
                message: format!("Access to protected path denied: {}", abs_path.display()),
                kind: CommandSandboxErrorKind::ProtectedPath,
            });
        }

        // 3. If root_dir is set, ensure path is within it (after canonicalization)
        if let Some(ref root) = self.root_dir {
            // Try to canonicalize both paths
            let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());

            // For the path, we need to handle non-existent paths
            // Try canonicalize, or use the parent if the file doesn't exist yet
            let canonical_path = if abs_path.exists() {
                abs_path.canonicalize().unwrap_or(abs_path.clone())
            } else if let Some(parent) = abs_path.parent() {
                if parent.exists() {
                    parent
                        .canonicalize()
                        .map(|p| p.join(abs_path.file_name().unwrap_or_default()))
                        .unwrap_or(abs_path.clone())
                } else {
                    abs_path.clone()
                }
            } else {
                abs_path.clone()
            };

            if !canonical_path.starts_with(&canonical_root) {
                return Err(CommandSandboxError {
                    message: format!(
                        "Path traversal detected: {} escapes root {}",
                        canonical_path.display(),
                        canonical_root.display()
                    ),
                    kind: CommandSandboxErrorKind::PathTraversal,
                });
            }
        }

        Ok(())
    }

    // Parser functions for each command type

    fn parse_ls(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let mut cmd_args = Vec::new();
        let mut path = PathBuf::from(".");

        for arg in args {
            if arg.starts_with('-') {
                // Only allow safe flags
                let allowed_flags = ["-l", "-a", "-la", "-al", "-h", "-lh", "-1", "-R"];
                if allowed_flags.contains(arg) {
                    cmd_args.push(arg.to_string());
                }
            } else {
                path = PathBuf::from(arg);
            }
        }

        Ok(AllowedCommand::Ls { path, args: cmd_args })
    }

    fn parse_find(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let mut path = PathBuf::from(".");
        let mut name_pattern = String::new();
        let mut max_depth = None;
        let mut case_insensitive = false;
        let mut i = 0;

        while i < args.len() {
            match args[i] {
                "-name" if i + 1 < args.len() => {
                    name_pattern = args[i + 1].to_string();
                    case_insensitive = false;
                    i += 2;
                }
                "-iname" if i + 1 < args.len() => {
                    name_pattern = args[i + 1].to_string();
                    case_insensitive = true;
                    i += 2;
                }
                "-maxdepth" if i + 1 < args.len() => {
                    max_depth = args[i + 1].parse().ok();
                    i += 2;
                }
                "-type" if i + 1 < args.len() => {
                    // Allow -type f/d but ignore it for now (we accept it but don't filter)
                    i += 2;
                }
                arg if !arg.starts_with('-') && path.as_os_str() == "." => {
                    path = PathBuf::from(arg);
                    i += 1;
                }
                _ => i += 1,
            }
        }

        if name_pattern.is_empty() {
            return Err(CommandSandboxError {
                message: "find requires -name or -iname pattern".to_string(),
                kind: CommandSandboxErrorKind::ParseError,
            });
        }

        Ok(AllowedCommand::Find {
            path,
            name_pattern,
            max_depth,
            case_insensitive,
        })
    }

    fn parse_file(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let path = args
            .iter()
            .find(|a| !a.starts_with('-'))
            .map(PathBuf::from)
            .ok_or_else(|| CommandSandboxError {
                message: "file requires a path argument".to_string(),
                kind: CommandSandboxErrorKind::ParseError,
            })?;

        Ok(AllowedCommand::File { path })
    }

    fn parse_du(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let mut cmd_args = Vec::new();
        let mut path = PathBuf::from(".");

        for arg in args {
            if arg.starts_with('-') {
                let allowed_flags = ["-h", "-s", "-sh", "-hs", "-d", "-c"];
                if allowed_flags.iter().any(|f| arg.starts_with(f)) {
                    cmd_args.push(arg.to_string());
                }
            } else {
                path = PathBuf::from(arg);
            }
        }

        Ok(AllowedCommand::Du { path, args: cmd_args })
    }

    fn parse_wc(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let mut cmd_args = Vec::new();
        let mut path = None;

        for arg in args {
            if arg.starts_with('-') {
                let allowed_flags = ["-l", "-w", "-c", "-m"];
                if allowed_flags.contains(arg) {
                    cmd_args.push(arg.to_string());
                }
            } else {
                path = Some(PathBuf::from(arg));
            }
        }

        let path = path.ok_or_else(|| CommandSandboxError {
            message: "wc requires a path argument".to_string(),
            kind: CommandSandboxErrorKind::ParseError,
        })?;

        Ok(AllowedCommand::Wc { path, args: cmd_args })
    }

    fn parse_head(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let mut lines = 10;
        let mut path = None;
        let mut i = 0;

        while i < args.len() {
            if args[i] == "-n" && i + 1 < args.len() {
                lines = args[i + 1].parse().unwrap_or(10);
                i += 2;
            } else if !args[i].starts_with('-') {
                path = Some(PathBuf::from(args[i]));
                i += 1;
            } else {
                i += 1;
            }
        }

        let path = path.ok_or_else(|| CommandSandboxError {
            message: "head requires a path argument".to_string(),
            kind: CommandSandboxErrorKind::ParseError,
        })?;

        Ok(AllowedCommand::Head { path, lines })
    }

    fn parse_tail(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let mut lines = 10;
        let mut path = None;
        let mut i = 0;

        while i < args.len() {
            if args[i] == "-n" && i + 1 < args.len() {
                lines = args[i + 1].parse().unwrap_or(10);
                i += 2;
            } else if !args[i].starts_with('-') {
                path = Some(PathBuf::from(args[i]));
                i += 1;
            } else {
                i += 1;
            }
        }

        let path = path.ok_or_else(|| CommandSandboxError {
            message: "tail requires a path argument".to_string(),
            kind: CommandSandboxErrorKind::ParseError,
        })?;

        Ok(AllowedCommand::Tail { path, lines })
    }

    fn parse_cat(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let path = args
            .iter()
            .find(|a| !a.starts_with('-'))
            .map(PathBuf::from)
            .ok_or_else(|| CommandSandboxError {
                message: "cat requires a path argument".to_string(),
                kind: CommandSandboxErrorKind::ParseError,
            })?;

        Ok(AllowedCommand::Cat { path })
    }

    fn parse_less(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let path = args
            .iter()
            .find(|a| !a.starts_with('-'))
            .map(PathBuf::from)
            .ok_or_else(|| CommandSandboxError {
                message: "less requires a path argument".to_string(),
                kind: CommandSandboxErrorKind::ParseError,
            })?;

        Ok(AllowedCommand::Less { path })
    }

    fn parse_grep(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let mut cmd_args = Vec::new();
        let mut pattern = None;
        let mut path = None;

        for arg in args {
            if arg.starts_with('-') {
                let allowed_flags = ["-i", "-n", "-r", "-l", "-c", "-v", "-w", "-E"];
                if allowed_flags.contains(arg) {
                    cmd_args.push(arg.to_string());
                }
            } else if pattern.is_none() {
                pattern = Some(arg.to_string());
            } else {
                path = Some(PathBuf::from(arg));
            }
        }

        let pattern = pattern.ok_or_else(|| CommandSandboxError {
            message: "grep requires a pattern argument".to_string(),
            kind: CommandSandboxErrorKind::ParseError,
        })?;

        let path = path.unwrap_or_else(|| PathBuf::from("."));

        Ok(AllowedCommand::Grep {
            pattern,
            path,
            args: cmd_args,
        })
    }

    fn parse_rg(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        let mut cmd_args = Vec::new();
        let mut pattern = None;
        let mut path = None;

        for arg in args {
            if arg.starts_with('-') {
                let allowed_flags = [
                    "-i", "-n", "-l", "-c", "-v", "-w", "-g", "--no-heading", "--hidden",
                ];
                if allowed_flags.iter().any(|f| arg.starts_with(f)) {
                    cmd_args.push(arg.to_string());
                }
            } else if pattern.is_none() {
                pattern = Some(arg.to_string());
            } else {
                path = Some(PathBuf::from(arg));
            }
        }

        let pattern = pattern.ok_or_else(|| CommandSandboxError {
            message: "rg requires a pattern argument".to_string(),
            kind: CommandSandboxErrorKind::ParseError,
        })?;

        let path = path.unwrap_or_else(|| PathBuf::from("."));

        Ok(AllowedCommand::Rg {
            pattern,
            path,
            args: cmd_args,
        })
    }

    fn parse_git(&self, args: &[&str]) -> Result<AllowedCommand, CommandSandboxError> {
        if args.is_empty() {
            return Err(CommandSandboxError {
                message: "git requires a subcommand".to_string(),
                kind: CommandSandboxErrorKind::ParseError,
            });
        }

        match args[0] {
            "status" => Ok(AllowedCommand::GitStatus {
                path: PathBuf::from("."),
            }),
            "log" => {
                let mut count = 10;
                let mut i = 1;
                while i < args.len() {
                    if args[i] == "-n" && i + 1 < args.len() {
                        count = args[i + 1].parse().unwrap_or(10);
                        break;
                    } else if args[i].starts_with("-") && args[i].len() > 1 {
                        // Try parsing -N format
                        if let Ok(n) = args[i][1..].parse::<usize>() {
                            count = n;
                        }
                    }
                    i += 1;
                }
                Ok(AllowedCommand::GitLog {
                    path: PathBuf::from("."),
                    count,
                })
            }
            _ => Err(CommandSandboxError {
                message: format!(
                    "git subcommand '{}' not allowed. Only 'status' and 'log' are permitted",
                    args[0]
                ),
                kind: CommandSandboxErrorKind::NotAllowed,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reject_shell_metacharacters() {
        // Safe commands
        assert!(CommandSandbox::reject_shell_metacharacters("ls -la").is_ok());

        // Never-allowed characters (command injection)
        assert!(CommandSandbox::reject_shell_metacharacters("ls; rm -rf /").is_err());
        assert!(CommandSandbox::reject_shell_metacharacters("$(whoami)").is_err());
        assert!(CommandSandbox::reject_shell_metacharacters("ls && rm").is_err());

        // Approvable characters (return NeedsApproval, not ShellMetacharacter)
        let pipe_result = CommandSandbox::reject_shell_metacharacters("ls | grep foo");
        assert!(pipe_result.is_err());
        if let Err(e) = pipe_result {
            assert!(matches!(e.kind, CommandSandboxErrorKind::NeedsApproval { .. }));
        }
    }

    #[test]
    fn test_approved_commands() {
        // With force mode, metacharacters are allowed
        let sandbox = CommandSandbox::new(None).with_force_mode(true);
        assert!(sandbox.parse_command("find . -name '*.rs' -o -name '*.md'").is_ok());

        // With approved pattern, matching commands are allowed
        let sandbox = CommandSandbox::new(None)
            .with_approved_patterns(vec!["find".to_string()]);
        assert!(sandbox.parse_command("find . -name '*.rs'").is_ok());
    }

    #[test]
    fn test_parse_allowed_commands() {
        let sandbox = CommandSandbox::new(Some(PathBuf::from("/tmp")));

        assert!(sandbox.parse_command("ls -la /tmp").is_ok());
        assert!(sandbox.parse_command("find /tmp -name '*.txt'").is_ok());
        assert!(sandbox.parse_command("cat /tmp/test.txt").is_ok());
        assert!(sandbox.parse_command("grep pattern /tmp/file").is_ok());
        assert!(sandbox.parse_command("git status").is_ok());
        assert!(sandbox.parse_command("git log -10").is_ok());
    }

    #[test]
    fn test_reject_unknown_commands() {
        let sandbox = CommandSandbox::new(None);

        assert!(sandbox.parse_command("rm -rf /").is_err());
        assert!(sandbox.parse_command("curl http://evil.com").is_err());
        assert!(sandbox.parse_command("wget http://evil.com").is_err());
        assert!(sandbox.parse_command("chmod 777 /").is_err());
    }

    #[test]
    fn test_reject_dangerous_git_subcommands() {
        let sandbox = CommandSandbox::new(None);

        assert!(sandbox.parse_command("git push").is_err());
        assert!(sandbox.parse_command("git reset --hard").is_err());
        assert!(sandbox.parse_command("git rm file").is_err());
    }

    #[test]
    fn test_path_validation() {
        let sandbox = CommandSandbox::new(Some(PathBuf::from("/home/user/project")));

        // Would fail if /home/user/project doesn't exist, but validates the logic
        let cmd = sandbox.parse_command("ls ../../../etc/passwd");
        // The parse succeeds, but execution would fail path validation
        assert!(cmd.is_ok());
    }
}
