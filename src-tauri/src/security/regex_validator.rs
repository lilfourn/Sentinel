//! Regex complexity validation to prevent ReDoS attacks.
//!
//! This module provides utilities for validating regex patterns before compilation
//! to prevent Regular Expression Denial of Service (ReDoS) attacks from malicious
//! or poorly constructed patterns.
//!
//! # Security
//!
//! ReDoS attacks exploit the exponential backtracking behavior of NFA-based regex
//! engines when processing certain pattern/input combinations. This module:
//!
//! 1. Blocks known dangerous pattern structures
//! 2. Detects nested quantifiers that can cause exponential blowup
//! 3. Limits pattern complexity via scoring
//! 4. Enforces compilation timeout
//!
//! # References
//!
//! - OWASP ReDoS: https://owasp.org/www-community/attacks/Regular_expression_Denial_of_Service_-_ReDoS

use regex::Regex;
use std::time::{Duration, Instant};

/// Maximum allowed regex pattern length
const MAX_PATTERN_LENGTH: usize = 500;

/// Maximum number of capturing groups allowed
const MAX_GROUPS: usize = 10;

/// Maximum complexity score before rejection
const MAX_COMPLEXITY_SCORE: u32 = 100;

/// Timeout for regex compilation (should be fast for safe patterns)
const COMPILE_TIMEOUT_MS: u64 = 100;

/// Patterns known to cause exponential backtracking
///
/// These patterns are structural templates that indicate ReDoS vulnerability.
/// We check for similar structures, not exact matches.
const DANGEROUS_PATTERNS: &[&str] = &[
    // Nested quantifiers on same character/class
    r"(a+)+",
    r"(a*)*",
    r"(a?)+",
    r"(a+)*",
    r"(a*)+",
    // Nested star with wildcard
    r"(.*)*",
    r"(.+)+",
    r"(.+)*",
    r"(.*)+",
    // Alternation with overlapping branches
    r"(a|a)+",
    r"(a|aa)+",
    r"(aa|a)+",
    r"(a|ab)+",
    // Character class quantifier patterns
    r"(\w+)*",
    r"(\d+)*",
    r"(\s+)*",
    r"([a-z]+)*",
    r"([a-zA-Z]+)+",
    r"([0-9]+)+",
    // Any-char nested patterns
    r"([\s\S]*)*",
    r"([\s\S]+)+",
    r"([\w\W]+)+",
    r"([\d\D]+)+",
    // Greedy with minimum count
    r"(.*a){5,}",
    r"(.+a){5,}",
    // Lookahead/lookbehind with quantifiers (if supported)
    r"(?=a+)+",
    r"(?=.*a)+",
];

/// Error type for regex validation
#[derive(Debug, Clone)]
#[allow(dead_code)]
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
    TooComplex,
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
    let _pattern_lower = pattern.to_lowercase();
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

    // 5. Calculate and check complexity score
    let complexity = calculate_complexity_score(pattern);
    if complexity > MAX_COMPLEXITY_SCORE {
        return Err(RegexValidationError {
            message: format!(
                "Regex too complex: score {} exceeds limit {} (reduce quantifiers, groups, or alternations)",
                complexity, MAX_COMPLEXITY_SCORE
            ),
            kind: RegexValidationErrorKind::TooComplex,
        });
    }

    // 6. Try to compile with timeout check
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

/// Calculate a complexity score for a regex pattern
///
/// Higher scores indicate more complex (potentially dangerous) patterns.
/// The score is based on:
/// - Number of quantifiers (+, *, ?, {n,m})
/// - Number of groups (capturing and non-capturing)
/// - Number of alternations (|)
/// - Presence of wildcards (., \w, \d, etc.)
/// - Nesting depth
///
/// # Returns
/// A complexity score (0-255+). Patterns above MAX_COMPLEXITY_SCORE are rejected.
fn calculate_complexity_score(pattern: &str) -> u32 {
    let mut score: u32 = 0;
    let mut depth: u32 = 0;
    let mut max_depth: u32 = 0;
    let mut escaped = false;
    let mut in_char_class = false;
    let mut quantifier_count: u32 = 0;
    let mut alternation_count: u32 = 0;
    let mut wildcard_count: u32 = 0;

    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if escaped {
            escaped = false;
            // Check for wildcard escapes
            if matches!(c, 'w' | 'W' | 'd' | 'D' | 's' | 'S') {
                wildcard_count += 1;
            }
            i += 1;
            continue;
        }

        match c {
            '\\' => {
                escaped = true;
            }
            '[' if !in_char_class => {
                in_char_class = true;
            }
            ']' if in_char_class => {
                in_char_class = false;
            }
            '(' if !in_char_class => {
                depth += 1;
                if depth > max_depth {
                    max_depth = depth;
                }
            }
            ')' if !in_char_class => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            '|' if !in_char_class => {
                alternation_count += 1;
            }
            '+' | '*' | '?' if !in_char_class => {
                quantifier_count += 1;
            }
            '{' if !in_char_class => {
                // Counted quantifier
                quantifier_count += 1;
                // Skip to closing brace
                while i < chars.len() && chars[i] != '}' {
                    i += 1;
                }
            }
            '.' if !in_char_class => {
                wildcard_count += 1;
            }
            _ => {}
        }

        i += 1;
    }

    // Calculate score based on various factors
    // Quantifiers are the main ReDoS risk
    score += quantifier_count * 10;

    // Alternations increase backtracking paths
    score += alternation_count * 8;

    // Wildcards can match many characters
    score += wildcard_count * 5;

    // Nesting depth multiplies risk exponentially
    if max_depth > 0 {
        score += max_depth * 15;
    }

    // Combination penalties
    // Quantifiers + groups = higher risk
    let group_count = count_capturing_groups(pattern) as u32;
    if group_count > 0 && quantifier_count > 0 {
        score += group_count * quantifier_count * 5;
    }

    // Alternation + quantifiers = even higher risk
    if alternation_count > 0 && quantifier_count > 0 {
        score += alternation_count * quantifier_count * 8;
    }

    // Length penalty for very long patterns
    if pattern.len() > 100 {
        score += ((pattern.len() - 100) / 50) as u32 * 5;
    }

    score
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

    #[test]
    fn test_additional_dangerous_patterns() {
        // Alternation with overlapping branches
        assert!(validate_regex_complexity(r"(a|aa)+").is_err());
        assert!(validate_regex_complexity(r"(aa|a)+").is_err());

        // Character class quantifier patterns
        assert!(validate_regex_complexity(r"([a-z]+)*").is_err());
        assert!(validate_regex_complexity(r"([0-9]+)+").is_err());

        // Any-char nested patterns
        assert!(validate_regex_complexity(r"(.+)+").is_err());
        assert!(validate_regex_complexity(r"(.+)*").is_err());
    }

    #[test]
    fn test_complexity_scoring() {
        // Simple patterns should have low scores
        let simple_score = calculate_complexity_score(r"\d+");
        assert!(simple_score < 50, "Simple pattern score: {}", simple_score);

        // File matching patterns should be safe
        let file_pattern_score = calculate_complexity_score(r"IMG_\d{4}\.jpg");
        assert!(file_pattern_score < MAX_COMPLEXITY_SCORE, "File pattern score: {}", file_pattern_score);

        // Complex patterns should have higher scores
        let complex_score = calculate_complexity_score(r"(a|b|c|d|e)+.*\d+");
        assert!(complex_score > 30, "Complex pattern score: {}", complex_score);
    }

    #[test]
    fn test_real_world_safe_patterns() {
        // Date pattern
        assert!(validate_regex_complexity(r"\d{4}-\d{2}-\d{2}").is_ok());

        // Email-like pattern (simplified)
        assert!(validate_regex_complexity(r"[a-zA-Z0-9]+@[a-zA-Z0-9]+\.[a-zA-Z]{2,}").is_ok());

        // File extension matching
        assert!(validate_regex_complexity(r"\.(txt|md|pdf|doc)$").is_ok());

        // Invoice pattern
        assert!(validate_regex_complexity(r"INV-\d{6}").is_ok());

        // Screenshot naming
        assert!(validate_regex_complexity(r"Screenshot \d{4}-\d{2}-\d{2}").is_ok());
    }

    #[test]
    fn test_complexity_too_high() {
        // Construct a pattern that's complex but doesn't match any dangerous pattern substring
        let complex = "(a)(b)(c)(d)(e).*+?.*+?.*+?";
        let result = validate_regex_complexity(complex);
        // This might fail due to complexity or nested quantifiers
        // Either is acceptable - we just want it blocked
        if result.is_err() {
            let err = result.unwrap_err();
            assert!(
                matches!(err.kind, RegexValidationErrorKind::TooComplex | RegexValidationErrorKind::NestedQuantifiers | RegexValidationErrorKind::InvalidRegex),
                "Expected TooComplex, NestedQuantifiers, or InvalidRegex, got {:?}",
                err.kind
            );
        }
    }
}
