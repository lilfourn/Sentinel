//! Regex complexity validation to prevent ReDoS attacks.
//!
//! This module provides utilities for validating regex patterns before compilation
//! to prevent Regular Expression Denial of Service (ReDoS) attacks from malicious
//! or poorly constructed patterns.

use regex::Regex;
use std::time::{Duration, Instant};

/// Maximum allowed regex pattern length
const MAX_PATTERN_LENGTH: usize = 500;

/// Maximum number of capturing groups allowed
const MAX_GROUPS: usize = 10;

/// Timeout for regex compilation (should be fast for safe patterns)
const COMPILE_TIMEOUT_MS: u64 = 100;

/// Patterns known to cause exponential backtracking
const DANGEROUS_PATTERNS: &[&str] = &[
    r"(a+)+",       // Nested quantifiers on same char
    r"(a*)*",       // Nested star
    r"(.*)*",       // Nested star with wildcard
    r"(a|a)+",      // Alternation with identical branches
    r"(\w+)*",      // Common ReDoS pattern
    r"([\s\S]*)*",  // Nested any-char
];

/// Error type for regex validation
#[derive(Debug, Clone)]
pub struct RegexValidationError {
    pub message: String,
    pub kind: RegexValidationErrorKind,
}

#[derive(Debug, Clone)]
pub enum RegexValidationErrorKind {
    TooLong,
    TooManyGroups,
    NestedQuantifiers,
    DangerousPattern,
    CompilationTimeout,
    InvalidRegex,
}

impl std::fmt::Display for RegexValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RegexValidationError {}

/// Validate regex pattern for complexity/safety
///
/// This function checks for patterns that could cause catastrophic backtracking
/// or consume excessive resources during matching.
///
/// # Arguments
/// * `pattern` - The regex pattern string to validate
///
/// # Returns
/// * `Ok(())` if the pattern is safe to use
/// * `Err(RegexValidationError)` if the pattern is potentially dangerous
///
/// # Example
/// ```
/// use crate::security::regex_validator::validate_regex_complexity;
///
/// // Safe pattern
/// assert!(validate_regex_complexity(r"IMG_\d+\.jpg").is_ok());
///
/// // Dangerous pattern (nested quantifiers)
/// assert!(validate_regex_complexity(r"(a+)+").is_err());
/// ```
pub fn validate_regex_complexity(pattern: &str) -> Result<(), RegexValidationError> {
    // 1. Length check
    if pattern.len() > MAX_PATTERN_LENGTH {
        return Err(RegexValidationError {
            message: format!(
                "Regex pattern too long: {} chars (max {})",
                pattern.len(),
                MAX_PATTERN_LENGTH
            ),
            kind: RegexValidationErrorKind::TooLong,
        });
    }

    // 2. Check for known dangerous patterns
    let pattern_lower = pattern.to_lowercase();
    for dangerous in DANGEROUS_PATTERNS {
        // Check if the dangerous pattern appears as a substring (simplified check)
        // More sophisticated check would parse the AST
        if contains_similar_structure(pattern, dangerous) {
            return Err(RegexValidationError {
                message: format!(
                    "Pattern contains potentially dangerous structure similar to: {}",
                    dangerous
                ),
                kind: RegexValidationErrorKind::DangerousPattern,
            });
        }
    }

    // 3. Count capturing groups
    let group_count = count_capturing_groups(pattern);
    if group_count > MAX_GROUPS {
        return Err(RegexValidationError {
            message: format!(
                "Too many capturing groups: {} (max {})",
                group_count, MAX_GROUPS
            ),
            kind: RegexValidationErrorKind::TooManyGroups,
        });
    }

    // 4. Check for nested quantifiers (simplified heuristic)
    if has_nested_quantifiers(pattern) {
        return Err(RegexValidationError {
            message: "Nested quantifiers detected - potential ReDoS vulnerability".to_string(),
            kind: RegexValidationErrorKind::NestedQuantifiers,
        });
    }

    // 5. Try to compile with timeout check
    let start = Instant::now();
    match Regex::new(pattern) {
        Ok(_) => {
            if start.elapsed() > Duration::from_millis(COMPILE_TIMEOUT_MS) {
                return Err(RegexValidationError {
                    message: "Regex compilation took too long - pattern may be too complex"
                        .to_string(),
                    kind: RegexValidationErrorKind::CompilationTimeout,
                });
            }
            Ok(())
        }
        Err(e) => Err(RegexValidationError {
            message: format!("Invalid regex: {}", e),
            kind: RegexValidationErrorKind::InvalidRegex,
        }),
    }
}

/// Compile regex with safety checks
///
/// This is a convenience function that validates the pattern first,
/// then compiles it if safe.
///
/// # Arguments
/// * `pattern` - The regex pattern string
///
/// # Returns
/// * `Ok(Regex)` if pattern is safe and compiles successfully
/// * `Err(String)` with error message if validation or compilation fails
pub fn safe_regex(pattern: &str) -> Result<Regex, String> {
    validate_regex_complexity(pattern).map_err(|e| e.message)?;
    Regex::new(pattern).map_err(|e| format!("Invalid regex: {}", e))
}

/// Count the number of capturing groups in a pattern
fn count_capturing_groups(pattern: &str) -> usize {
    let mut count = 0;
    let mut chars = pattern.chars().peekable();
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }

        match c {
            '\\' => escaped = true,
            '(' => {
                // Check if it's a non-capturing group (?:...)
                if let Some(&next) = chars.peek() {
                    if next != '?' {
                        count += 1;
                    }
                } else {
                    count += 1;
                }
            }
            _ => {}
        }
    }

    count
}

/// Check for nested quantifiers which can cause catastrophic backtracking
fn has_nested_quantifiers(pattern: &str) -> bool {
    // Look for patterns like (...)+ where ... contains a quantifier
    // This is a simplified heuristic

    let quantifiers = ['+', '*', '?'];
    let mut depth = 0;
    let mut has_inner_quantifier = false;
    let mut chars = pattern.chars().peekable();
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }

        match c {
            '\\' => escaped = true,
            '(' => {
                depth += 1;
                has_inner_quantifier = false;
            }
            ')' => {
                if depth > 0 {
                    depth -= 1;
                    // Check if the next char is a quantifier
                    if let Some(&next) = chars.peek() {
                        if quantifiers.contains(&next) && has_inner_quantifier {
                            return true;
                        }
                    }
                }
                has_inner_quantifier = false;
            }
            c if quantifiers.contains(&c) => {
                if depth > 0 {
                    has_inner_quantifier = true;
                }
            }
            _ => {}
        }
    }

    false
}

/// Check if pattern contains a structure similar to a dangerous pattern
fn contains_similar_structure(pattern: &str, dangerous: &str) -> bool {
    // Simplified check: look for the dangerous pattern as a substring
    // This catches obvious cases; a full AST analysis would be more thorough

    // Direct substring check
    if pattern.contains(dangerous) {
        return true;
    }

    // Check for variations with whitespace
    let normalized_pattern = pattern.replace(' ', "");
    let normalized_dangerous = dangerous.replace(' ', "");
    if normalized_pattern.contains(&normalized_dangerous) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_patterns() {
        // Common file matching patterns
        assert!(validate_regex_complexity(r"IMG_\d+\.jpg").is_ok());
        assert!(validate_regex_complexity(r"^[a-z]+$").is_ok());
        assert!(validate_regex_complexity(r"\.(txt|md|rs)$").is_ok());
        assert!(validate_regex_complexity(r"[0-9]{4}-[0-9]{2}-[0-9]{2}").is_ok());
    }

    #[test]
    fn test_pattern_too_long() {
        let long_pattern = "a".repeat(600);
        let result = validate_regex_complexity(&long_pattern);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err().kind,
            RegexValidationErrorKind::TooLong
        ));
    }

    #[test]
    fn test_nested_quantifiers() {
        assert!(validate_regex_complexity(r"(a+)+").is_err());
        assert!(validate_regex_complexity(r"(.*)*").is_err());
        assert!(validate_regex_complexity(r"(\w+)*").is_err());
    }

    #[test]
    fn test_too_many_groups() {
        // Create pattern with many groups
        let pattern = "(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)(l)";
        let result = validate_regex_complexity(pattern);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err().kind,
            RegexValidationErrorKind::TooManyGroups
        ));
    }

    #[test]
    fn test_invalid_regex() {
        let result = validate_regex_complexity(r"[unclosed");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err().kind,
            RegexValidationErrorKind::InvalidRegex
        ));
    }

    #[test]
    fn test_safe_regex_function() {
        // Safe pattern
        let result = safe_regex(r"\d+");
        assert!(result.is_ok());

        // Dangerous pattern
        let result = safe_regex(r"(a+)+");
        assert!(result.is_err());
    }

    #[test]
    fn test_count_capturing_groups() {
        assert_eq!(count_capturing_groups(r"(a)(b)(c)"), 3);
        assert_eq!(count_capturing_groups(r"(?:a)(b)"), 1); // Non-capturing
        assert_eq!(count_capturing_groups(r"\(escaped\)"), 0);
        assert_eq!(count_capturing_groups(r"no groups"), 0);
    }
}
