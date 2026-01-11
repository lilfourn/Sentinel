//! Rule evaluator for matching files against rule expressions.
//!
//! The evaluator takes a parsed rule AST and evaluates it against files
//! to determine if they match the rule criteria.

use super::ast::*;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

/// Error type for rule evaluation failures
#[derive(Debug, Clone)]
pub struct RuleError {
    pub message: String,
}

impl RuleError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rule error: {}", self.message)
    }
}

impl std::error::Error for RuleError {}

/// Virtual file representation for rule evaluation.
/// This is a lightweight struct containing only the fields needed for evaluation.
#[derive(Debug, Clone)]
pub struct VirtualFile {
    /// File name without extension
    pub name: String,
    /// File extension (lowercase, no dot)
    pub ext: Option<String>,
    /// File size in bytes
    pub size: u64,
    /// Full file path
    pub path: String,
    /// Last modified timestamp (unix milliseconds)
    pub modified_at: Option<i64>,
    /// Created timestamp (unix milliseconds)
    pub created_at: Option<i64>,
    /// MIME type
    pub mime_type: Option<String>,
    /// Whether file is hidden
    pub is_hidden: bool,
    /// Whether this is a directory
    pub is_directory: bool,
}

impl VirtualFile {
    /// Create a VirtualFile from a path
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let ext = path
            .extension()
            .map(|s| s.to_string_lossy().to_lowercase());

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);

        let created_at = metadata
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);

        let mime_type = ext.as_ref().and_then(|e| {
            mime_guess::from_ext(e)
                .first()
                .map(|m| m.to_string())
        });

        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        Ok(Self {
            name,
            ext,
            size: metadata.len(),
            path: path.to_string_lossy().to_string(),
            modified_at,
            created_at,
            mime_type,
            is_hidden: file_name.starts_with('.'),
            is_directory: metadata.is_dir(),
        })
    }

    /// Create a VirtualFile from raw data (for testing or VFS)
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        ext: Option<String>,
        size: u64,
        path: String,
        modified_at: Option<i64>,
        created_at: Option<i64>,
        mime_type: Option<String>,
        is_hidden: bool,
        is_directory: bool,
    ) -> Self {
        Self {
            name,
            ext,
            size,
            path,
            modified_at,
            created_at,
            mime_type,
            is_hidden,
            is_directory,
        }
    }
}

/// Vector index for semantic similarity queries.
/// This is a trait to allow different implementations (mock, real embeddings, etc.)
pub trait VectorIndex: Send + Sync {
    /// Get the semantic similarity score between a file and a query string.
    /// Returns a score between 0.0 (no match) and 1.0 (perfect match).
    fn similarity(&self, file_path: &str, query: &str) -> Result<f32, RuleError>;
}

/// Simple in-memory vector index for testing.
/// In production, this would be replaced with actual embedding-based similarity.
pub struct SimpleVectorIndex {
    /// Mapping from file path to searchable content (filename + any extracted text)
    file_content: HashMap<String, String>,
}

impl SimpleVectorIndex {
    pub fn new() -> Self {
        Self {
            file_content: HashMap::new(),
        }
    }

    /// Add a file to the index
    pub fn add_file(&mut self, path: &str, content: &str) {
        self.file_content.insert(path.to_string(), content.to_lowercase());
    }

    /// Build index from a list of virtual files
    pub fn build_from_files(files: &[VirtualFile]) -> Self {
        let mut index = Self::new();
        for file in files {
            // Index the filename and extension as searchable content
            let content = format!(
                "{} {}",
                file.name,
                file.ext.as_deref().unwrap_or("")
            );
            index.add_file(&file.path, &content);
        }
        index
    }
}

impl Default for SimpleVectorIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorIndex for SimpleVectorIndex {
    fn similarity(&self, file_path: &str, query: &str) -> Result<f32, RuleError> {
        let content = self.file_content.get(file_path).ok_or_else(|| {
            RuleError::new(format!("File not in index: {}", file_path))
        })?;

        // Simple word overlap similarity for now
        // In production, use actual embeddings
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let content_words: Vec<&str> = content.split_whitespace().collect();

        if query_words.is_empty() {
            return Ok(0.0);
        }

        let mut matches = 0;
        for qw in &query_words {
            if content_words.iter().any(|cw| cw.contains(qw) || qw.contains(cw)) {
                matches += 1;
            }
        }

        Ok(matches as f32 / query_words.len() as f32)
    }
}

/// Rule evaluator that matches files against rule expressions.
pub struct RuleEvaluator<'a, V: VectorIndex> {
    vector_index: &'a V,
}

impl<'a, V: VectorIndex> RuleEvaluator<'a, V> {
    /// Create a new rule evaluator with the given vector index
    pub fn new(vector_index: &'a V) -> Self {
        Self { vector_index }
    }

    /// Evaluate an expression against a file
    pub fn evaluate(&self, expr: &Expression, file: &VirtualFile) -> Result<bool, RuleError> {
        match expr {
            Expression::Or(left, right) => {
                Ok(self.evaluate(left, file)? || self.evaluate(right, file)?)
            }
            Expression::And(left, right) => {
                Ok(self.evaluate(left, file)? && self.evaluate(right, file)?)
            }
            Expression::Not(inner) => Ok(!self.evaluate(inner, file)?),
            Expression::Comparison(cmp) => self.evaluate_comparison(cmp, file),
            Expression::FunctionCall(func) => self.evaluate_function(func, file),
            Expression::Literal(b) => Ok(*b),
        }
    }

    /// Evaluate a comparison against a file
    pub fn evaluate_comparison(
        &self,
        cmp: &Comparison,
        file: &VirtualFile,
    ) -> Result<bool, RuleError> {
        let field_value = self.get_field_value(&cmp.field, file);

        match cmp.op {
            ComparisonOp::Eq => self.compare_eq(&field_value, &cmp.value),
            ComparisonOp::Ne => Ok(!self.compare_eq(&field_value, &cmp.value)?),
            ComparisonOp::Gt => self.compare_ord(&field_value, &cmp.value, |a, b| a > b),
            ComparisonOp::Lt => self.compare_ord(&field_value, &cmp.value, |a, b| a < b),
            ComparisonOp::Gte => self.compare_ord(&field_value, &cmp.value, |a, b| a >= b),
            ComparisonOp::Lte => self.compare_ord(&field_value, &cmp.value, |a, b| a <= b),
            ComparisonOp::In => self.compare_in(&field_value, &cmp.value),
            ComparisonOp::Matches => self.compare_matches(&field_value, &cmp.value),
        }
    }

    /// Evaluate a function call against a file
    pub fn evaluate_function(
        &self,
        func: &FunctionCall,
        file: &VirtualFile,
    ) -> Result<bool, RuleError> {
        // Get the string value to operate on
        let target = if func.receiver == "file" {
            // Direct function on file (e.g., file.vector_similarity)
            None
        } else if func.receiver.starts_with("file.") {
            // Function on a field (e.g., file.name.contains)
            let field_name = &func.receiver[5..]; // Strip "file."
            let field = Field::from_str(field_name).ok_or_else(|| {
                RuleError::new(format!("Unknown field: {}", field_name))
            })?;
            Some(self.get_field_value(&field, file))
        } else {
            return Err(RuleError::new(format!(
                "Invalid function receiver: {}",
                func.receiver
            )));
        };

        match func.function {
            FunctionName::Contains => {
                let target = target.ok_or_else(|| {
                    RuleError::new("contains requires a field receiver")
                })?;
                let pattern = func
                    .args
                    .first()
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| RuleError::new("contains requires a string argument"))?;

                let target_str = target.as_string().unwrap_or_default();
                Ok(target_str.to_lowercase().contains(&pattern.to_lowercase()))
            }

            FunctionName::StartsWith => {
                let target = target.ok_or_else(|| {
                    RuleError::new("startsWith requires a field receiver")
                })?;
                let prefix = func
                    .args
                    .first()
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| RuleError::new("startsWith requires a string argument"))?;

                let target_str = target.as_string().unwrap_or_default();
                Ok(target_str.to_lowercase().starts_with(&prefix.to_lowercase()))
            }

            FunctionName::EndsWith => {
                let target = target.ok_or_else(|| {
                    RuleError::new("endsWith requires a field receiver")
                })?;
                let suffix = func
                    .args
                    .first()
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| RuleError::new("endsWith requires a string argument"))?;

                let target_str = target.as_string().unwrap_or_default();
                Ok(target_str.to_lowercase().ends_with(&suffix.to_lowercase()))
            }

            FunctionName::Matches => {
                let target = target.ok_or_else(|| {
                    RuleError::new("matches requires a field receiver")
                })?;
                let pattern = func
                    .args
                    .first()
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| RuleError::new("matches requires a regex pattern argument"))?;

                let regex = Regex::new(&pattern).map_err(|e| {
                    RuleError::new(format!("Invalid regex pattern: {}", e))
                })?;

                let target_str = target.as_string().unwrap_or_default();
                Ok(regex.is_match(&target_str))
            }

            FunctionName::VectorSimilarity => {
                let query = func
                    .args
                    .first()
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| RuleError::new("vector_similarity requires a query string"))?;

                let score = self.vector_index.similarity(&file.path, &query)?;
                // For standalone function call, return true if score > 0.5
                // When used with comparison, the caller handles the threshold
                Ok(score > 0.5)
            }
        }
    }

    /// Get the value of a field from a file
    pub fn get_field_value(&self, field: &Field, file: &VirtualFile) -> Value {
        match field {
            Field::FileName => Value::String(file.name.clone()),
            Field::FileExt => file
                .ext
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
            Field::FileSize => Value::SizeBytes(file.size),
            Field::FilePath => Value::String(file.path.clone()),
            Field::FileModifiedAt => file
                .modified_at
                .map(|t| Value::Number(t as f64))
                .unwrap_or(Value::Null),
            Field::FileCreatedAt => file
                .created_at
                .map(|t| Value::Number(t as f64))
                .unwrap_or(Value::Null),
            Field::FileMimeType => file
                .mime_type
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
            Field::FileIsHidden => Value::Boolean(file.is_hidden),
        }
    }

    // Helper methods for comparisons

    fn compare_eq(&self, left: &Value, right: &Value) -> Result<bool, RuleError> {
        match (left, right) {
            (Value::String(a), Value::String(b)) => Ok(a.to_lowercase() == b.to_lowercase()),
            (Value::Number(a), Value::Number(b)) => Ok((a - b).abs() < f64::EPSILON),
            (Value::SizeBytes(a), Value::SizeBytes(b)) => Ok(a == b),
            (Value::SizeBytes(a), Value::Number(b)) => Ok(*a as f64 == *b),
            (Value::Number(a), Value::SizeBytes(b)) => Ok(*a == *b as f64),
            (Value::Boolean(a), Value::Boolean(b)) => Ok(a == b),
            (Value::Null, Value::Null) => Ok(true),
            _ => Ok(false),
        }
    }

    fn compare_ord<F>(&self, left: &Value, right: &Value, cmp: F) -> Result<bool, RuleError>
    where
        F: Fn(f64, f64) -> bool,
    {
        let left_num = left.as_number();
        let right_num = right.as_number();

        match (left_num, right_num) {
            (Some(a), Some(b)) => Ok(cmp(a, b)),
            _ => {
                // Try string comparison for dates
                match (left, right) {
                    (Value::String(a), Value::String(b)) => Ok(cmp(
                        a.as_str().cmp(b.as_str()) as i32 as f64,
                        0.0,
                    )),
                    _ => Err(RuleError::new("Cannot compare non-numeric values")),
                }
            }
        }
    }

    fn compare_in(&self, left: &Value, right: &Value) -> Result<bool, RuleError> {
        let arr = right
            .as_array()
            .ok_or_else(|| RuleError::new("IN requires an array value"))?;

        for item in arr {
            if self.compare_eq(left, item)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn compare_matches(&self, left: &Value, right: &Value) -> Result<bool, RuleError> {
        let target = left
            .as_string()
            .ok_or_else(|| RuleError::new("MATCHES requires a string field"))?;
        let pattern = right
            .as_string()
            .ok_or_else(|| RuleError::new("MATCHES requires a string pattern"))?;

        let regex = Regex::new(&pattern).map_err(|e| {
            RuleError::new(format!("Invalid regex pattern: {}", e))
        })?;

        Ok(regex.is_match(&target))
    }
}

/// Evaluate a rule against multiple files and return matching ones
pub fn filter_files<V: VectorIndex>(
    expr: &Expression,
    files: &[VirtualFile],
    vector_index: &V,
) -> Vec<VirtualFile> {
    let evaluator = RuleEvaluator::new(vector_index);
    files
        .iter()
        .filter(|file| evaluator.evaluate(expr, file).unwrap_or(false))
        .cloned()
        .collect()
}

/// Implementation of VectorIndex trait for the fastembed-based VectorIndex.
///
/// This allows the rule evaluator to use the real semantic embedding index
/// for vector_similarity queries instead of the simple keyword-based mock.
impl VectorIndex for crate::vector::VectorIndex {
    fn similarity(&self, file_path: &str, query: &str) -> Result<f32, RuleError> {
        let path = std::path::PathBuf::from(file_path);
        self.similarity(&path, query)
            .map_err(RuleError::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::rules::parser::RuleParser;

    fn create_test_file(name: &str, ext: Option<&str>, size: u64) -> VirtualFile {
        VirtualFile::new(
            name.to_string(),
            ext.map(|e| e.to_string()),
            size,
            format!("/test/{}.{}", name, ext.unwrap_or("")),
            Some(1700000000000),
            Some(1690000000000),
            None,
            name.starts_with('.'),
            false,
        )
    }

    #[test]
    fn test_simple_comparison() {
        let index = SimpleVectorIndex::new();
        let evaluator = RuleEvaluator::new(&index);

        let file = create_test_file("document", Some("pdf"), 1024);
        let expr = RuleParser::parse("file.ext == 'pdf'").unwrap();

        assert!(evaluator.evaluate(&expr, &file).unwrap());
    }

    #[test]
    fn test_size_comparison() {
        let index = SimpleVectorIndex::new();
        let evaluator = RuleEvaluator::new(&index);

        let file = create_test_file("large", Some("bin"), 1024 * 1024 * 5); // 5MB
        let expr = RuleParser::parse("file.size > 1MB").unwrap();

        assert!(evaluator.evaluate(&expr, &file).unwrap());

        let expr2 = RuleParser::parse("file.size < 10MB").unwrap();
        assert!(evaluator.evaluate(&expr2, &file).unwrap());
    }

    #[test]
    fn test_in_operator() {
        let index = SimpleVectorIndex::new();
        let evaluator = RuleEvaluator::new(&index);

        let file = create_test_file("image", Some("jpg"), 1024);
        let expr = RuleParser::parse("file.ext IN ['jpg', 'png', 'gif']").unwrap();

        assert!(evaluator.evaluate(&expr, &file).unwrap());

        let file2 = create_test_file("document", Some("pdf"), 1024);
        assert!(!evaluator.evaluate(&expr, &file2).unwrap());
    }

    #[test]
    fn test_and_expression() {
        let index = SimpleVectorIndex::new();
        let evaluator = RuleEvaluator::new(&index);

        let file = create_test_file("invoice", Some("pdf"), 2048);
        let expr = RuleParser::parse("file.ext == 'pdf' AND file.size > 1KB").unwrap();

        assert!(evaluator.evaluate(&expr, &file).unwrap());
    }

    #[test]
    fn test_or_expression() {
        let index = SimpleVectorIndex::new();
        let evaluator = RuleEvaluator::new(&index);

        let jpg = create_test_file("photo", Some("jpg"), 1024);
        let png = create_test_file("icon", Some("png"), 512);
        let pdf = create_test_file("doc", Some("pdf"), 2048);

        let expr = RuleParser::parse("file.ext == 'jpg' OR file.ext == 'png'").unwrap();

        assert!(evaluator.evaluate(&expr, &jpg).unwrap());
        assert!(evaluator.evaluate(&expr, &png).unwrap());
        assert!(!evaluator.evaluate(&expr, &pdf).unwrap());
    }

    #[test]
    fn test_not_expression() {
        let index = SimpleVectorIndex::new();
        let evaluator = RuleEvaluator::new(&index);

        let normal = create_test_file("document", Some("txt"), 100);
        let hidden = create_test_file(".hidden", Some("txt"), 100);

        let expr = RuleParser::parse("NOT file.isHidden").unwrap();

        assert!(evaluator.evaluate(&expr, &normal).unwrap());
        assert!(!evaluator.evaluate(&expr, &hidden).unwrap());
    }

    #[test]
    fn test_contains_function() {
        let index = SimpleVectorIndex::new();
        let evaluator = RuleEvaluator::new(&index);

        let file = create_test_file("invoice-2024-jan", Some("pdf"), 1024);
        let expr = RuleParser::parse("file.name.contains('invoice')").unwrap();

        assert!(evaluator.evaluate(&expr, &file).unwrap());

        let other = create_test_file("receipt", Some("pdf"), 1024);
        assert!(!evaluator.evaluate(&expr, &other).unwrap());
    }

    #[test]
    fn test_filter_files() {
        let files = vec![
            create_test_file("doc1", Some("pdf"), 1024),
            create_test_file("image1", Some("jpg"), 2048),
            create_test_file("doc2", Some("pdf"), 512),
            create_test_file("image2", Some("png"), 4096),
        ];

        let index = SimpleVectorIndex::build_from_files(&files);
        let expr = RuleParser::parse("file.ext == 'pdf'").unwrap();

        let matches = filter_files(&expr, &files, &index);
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|f| f.ext.as_deref() == Some("pdf")));
    }
}
