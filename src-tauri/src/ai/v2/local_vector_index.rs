//! LocalVectorIndex - Fast semantic search using fastembed embeddings
//!
//! This is a V3 implementation for semantic file search, optimized for:
//! - Fast search (<10ms per query after initialization)
//! - Memory-efficient storage with pre-computed embeddings
//! - Batch indexing during VFS creation
//!
//! Uses the AllMiniLM-L6-V2 model (384 dimensions) via fastembed.
//!
//! Implements the `VectorIndex` trait from the rules module for
//! compatibility with the rule evaluation system.

use crate::ai::rules::{RuleError, VectorIndex};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for LocalVectorIndex
#[derive(Debug, Clone)]
pub struct LocalVectorConfig {
    /// Similarity threshold for search results (0.0 - 1.0)
    pub similarity_threshold: f32,
    /// Maximum results to return from search
    pub max_results: usize,
}

impl Default for LocalVectorConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.3,
            max_results: 50,
        }
    }
}

/// A document in the local vector index
#[derive(Debug, Clone)]
pub struct IndexedDocument {
    /// File path
    pub path: PathBuf,
    /// Original text used for embedding (filename + extension)
    pub text: String,
    /// Pre-computed embedding vector (384 dimensions for AllMiniLM-L6-V2)
    pub embedding: Vec<f32>,
}

/// Local vector index using fastembed for semantic file search
///
/// This provides real semantic search capabilities using local embeddings,
/// without requiring any external API calls.
pub struct LocalVectorIndex {
    /// Fastembed model instance (shared across queries)
    model: Arc<TextEmbedding>,
    /// Indexed documents by path
    documents: HashMap<PathBuf, IndexedDocument>,
    /// Configuration
    config: LocalVectorConfig,
}

impl LocalVectorIndex {
    /// Create a new LocalVectorIndex
    ///
    /// Note: Model initialization downloads ~100MB on first use,
    /// then uses cached model from ~/.cache/fastembed/
    pub fn new(config: LocalVectorConfig) -> Result<Self, String> {
        eprintln!("[LocalVectorIndex] Initializing fastembed model...");

        let init_options = InitOptions::new(EmbeddingModel::AllMiniLML6V2)
            .with_show_download_progress(true);

        let model = TextEmbedding::try_new(init_options)
            .map_err(|e| format!("Failed to initialize embedding model: {}", e))?;

        eprintln!("[LocalVectorIndex] Model initialized successfully");

        Ok(Self {
            model: Arc::new(model),
            documents: HashMap::new(),
            config,
        })
    }

    /// Create with default configuration
    pub fn new_default() -> Result<Self, String> {
        Self::new(LocalVectorConfig::default())
    }

    /// Index a batch of files efficiently
    ///
    /// # Arguments
    /// * `files` - Vec of (path, searchable_text) tuples
    ///   searchable_text should be filename + extension for best results
    ///
    /// # Returns
    /// Number of successfully indexed documents
    pub fn index_batch(&mut self, files: Vec<(PathBuf, String)>) -> Result<usize, String> {
        if files.is_empty() {
            return Ok(0);
        }

        eprintln!(
            "[LocalVectorIndex] Indexing {} files...",
            files.len()
        );

        let texts: Vec<&str> = files.iter().map(|(_, t)| t.as_str()).collect();

        // Generate embeddings in batch (much faster than one-by-one)
        let embeddings = self
            .model
            .embed(texts, None)
            .map_err(|e| format!("Batch embedding failed: {}", e))?;

        if embeddings.len() != files.len() {
            return Err(format!(
                "Embedding count mismatch: {} embeddings vs {} files",
                embeddings.len(),
                files.len()
            ));
        }

        for ((path, text), embedding) in files.into_iter().zip(embeddings) {
            self.documents.insert(
                path.clone(),
                IndexedDocument {
                    path,
                    text,
                    embedding,
                },
            );
        }

        eprintln!(
            "[LocalVectorIndex] Indexed {} documents",
            self.documents.len()
        );

        Ok(self.documents.len())
    }

    /// Search for files matching a semantic query
    ///
    /// # Arguments
    /// * `query` - Natural language search query (e.g., "tax documents", "photos from vacation")
    ///
    /// # Returns
    /// Vec of (path, similarity_score) tuples sorted by score descending
    pub fn search(&self, query: &str) -> Result<Vec<(PathBuf, f32)>, String> {
        if query.is_empty() {
            return Err("Query cannot be empty".to_string());
        }

        if self.documents.is_empty() {
            return Ok(vec![]);
        }

        // Generate query embedding
        let query_embeddings = self
            .model
            .embed(vec![query], None)
            .map_err(|e| format!("Query embedding failed: {}", e))?;

        let query_embedding = query_embeddings
            .into_iter()
            .next()
            .ok_or("No query embedding generated")?;

        // Compute similarities against all documents
        let mut results: Vec<(PathBuf, f32)> = self
            .documents
            .iter()
            .map(|(path, doc)| {
                let score = cosine_similarity(&query_embedding, &doc.embedding);
                (path.clone(), score)
            })
            .filter(|(_, score)| *score >= self.config.similarity_threshold)
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(self.config.max_results);

        Ok(results)
    }

    /// Get similarity between a specific file and query
    ///
    /// # Arguments
    /// * `path` - Path to the file as string (must be indexed)
    /// * `query` - Natural language query
    ///
    /// # Returns
    /// Similarity score (0.0 - 1.0)
    pub fn file_similarity(&self, path: &str, query: &str) -> Result<f32, String> {
        let path_buf = PathBuf::from(path);
        let doc = self
            .documents
            .get(&path_buf)
            .ok_or_else(|| format!("Document not found: {}", path))?;

        let query_embeddings = self
            .model
            .embed(vec![query], None)
            .map_err(|e| format!("Query embedding failed: {}", e))?;

        let query_embedding = query_embeddings
            .into_iter()
            .next()
            .ok_or("No query embedding generated")?;

        Ok(cosine_similarity(&query_embedding, &doc.embedding))
    }

    /// Get number of indexed documents
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Get configuration
    pub fn config(&self) -> &LocalVectorConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: LocalVectorConfig) {
        self.config = config;
    }

    /// Get all indexed paths
    pub fn indexed_paths(&self) -> Vec<&PathBuf> {
        self.documents.keys().collect()
    }
}

/// Implementation of VectorIndex trait for rule evaluation compatibility
impl VectorIndex for LocalVectorIndex {
    fn similarity(&self, file_path: &str, query: &str) -> Result<f32, RuleError> {
        let path = PathBuf::from(file_path);

        // Look up the document
        let doc = self
            .documents
            .get(&path)
            .ok_or_else(|| RuleError::new(format!("Document not found: {}", file_path)))?;

        // Generate query embedding
        let query_embeddings = self
            .model
            .embed(vec![query], None)
            .map_err(|e| RuleError::new(format!("Query embedding failed: {}", e)))?;

        let query_embedding = query_embeddings
            .into_iter()
            .next()
            .ok_or_else(|| RuleError::new("No query embedding generated".to_string()))?;

        Ok(cosine_similarity(&query_embedding, &doc.embedding))
    }
}

/// Compute cosine similarity between two vectors
///
/// Returns a value between -1.0 and 1.0, where:
/// - 1.0 means identical direction
/// - 0.0 means orthogonal (no similarity)
/// - -1.0 means opposite direction
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched_length() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_default_config() {
        let config = LocalVectorConfig::default();
        assert_eq!(config.similarity_threshold, 0.3);
        assert_eq!(config.max_results, 50);
    }
}
