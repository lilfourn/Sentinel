//! Vector Embedder Module
//!
//! Handles fastembed integration for generating text embeddings.
//! Uses the AllMiniLmL6V2 model by default for fast, quality embeddings.

use super::{VectorConfig, VectorDocument, VectorIndex};
use fastembed::{InitOptions, TextEmbedding};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Wrapper around fastembed's TextEmbedding model
///
/// Provides a clean interface for embedding generation with error handling
/// and batch processing support.
pub struct VectorEmbedder {
    /// The underlying fastembed model
    model: Arc<TextEmbedding>,
}

impl VectorEmbedder {
    /// Create a new embedder with the given configuration
    ///
    /// This will download the model on first use if not cached locally.
    /// Model cache location: ~/.cache/fastembed (or platform equivalent)
    pub fn new(config: &VectorConfig) -> Result<Self, String> {
        eprintln!("[VectorEmbedder] Initializing with model: {:?}", config.model);

        let init_options = InitOptions::new(config.model.to_fastembed_model())
            .with_show_download_progress(true);

        let model = TextEmbedding::try_new(init_options)
            .map_err(|e| format!("Failed to initialize embedding model: {}", e))?;

        eprintln!("[VectorEmbedder] Model initialized successfully");
        Ok(Self {
            model: Arc::new(model),
        })
    }

    /// Generate an embedding for a single text string
    pub fn get_embedding(&self, text: &str) -> Result<Vec<f32>, String> {
        if text.is_empty() {
            return Err("Cannot embed empty text".to_string());
        }

        let embeddings = self
            .model
            .embed(vec![text], None)
            .map_err(|e| format!("Failed to generate embedding: {}", e))?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| "No embedding generated".to_string())
    }

    /// Generate embeddings for multiple texts in a batch
    ///
    /// More efficient than calling get_embedding multiple times
    pub fn get_embeddings_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        self.model
            .embed(texts, None)
            .map_err(|e| format!("Failed to generate batch embeddings: {}", e))
    }
}

impl VectorIndex {
    /// Index a single node (file or folder)
    ///
    /// # Arguments
    /// * `path` - Absolute path to the file/folder
    /// * `name` - Display name (filename)
    /// * `content_preview` - Optional content preview for better semantic matching
    ///
    /// # Returns
    /// Ok(()) on success, Err(String) on failure
    pub fn index_node(
        &mut self,
        path: &Path,
        name: &str,
        content_preview: Option<&str>,
    ) -> Result<(), String> {
        // Build text for embedding: combine name and content preview
        let text = match content_preview {
            Some(preview) if !preview.is_empty() => format!("{} {}", name, preview),
            _ => name.to_string(),
        };

        // Generate embedding
        let embedding = self.embedder.get_embedding(&text)?;

        // Assign semantic tags based on similarity to category embeddings
        let tags = self.compute_tags(&embedding);

        let doc = VectorDocument {
            path: path.to_path_buf(),
            text,
            embedding,
            tags,
        };

        self.insert_document(doc);
        Ok(())
    }

    /// Index multiple nodes in a batch for efficiency
    ///
    /// # Arguments
    /// * `nodes` - Vector of (path, name, optional content_preview) tuples
    ///
    /// # Returns
    /// Number of successfully indexed nodes
    pub fn index_batch(
        &mut self,
        nodes: Vec<(PathBuf, String, Option<String>)>,
    ) -> Result<usize, String> {
        if nodes.is_empty() {
            return Ok(0);
        }

        // Build texts for batch embedding
        let texts: Vec<String> = nodes
            .iter()
            .map(|(_, name, preview)| match preview {
                Some(p) if !p.is_empty() => format!("{} {}", name, p),
                _ => name.clone(),
            })
            .collect();

        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        // Generate all embeddings in a single batch
        let embeddings = self.embedder.get_embeddings_batch(text_refs)?;

        if embeddings.len() != nodes.len() {
            return Err(format!(
                "Embedding count mismatch: expected {}, got {}",
                nodes.len(),
                embeddings.len()
            ));
        }

        let mut indexed_count = 0;
        for ((path, _, _), (text, embedding)) in nodes.into_iter().zip(texts.into_iter().zip(embeddings)) {
            let tags = self.compute_tags(&embedding);

            let doc = VectorDocument {
                path: path.clone(),
                text,
                embedding,
                tags,
            };

            self.insert_document(doc);
            indexed_count += 1;
        }

        Ok(indexed_count)
    }

    /// Compute semantic tags for a document based on similarity to category embeddings
    ///
    /// Returns tags for categories that exceed the similarity threshold
    fn compute_tags(&self, embedding: &[f32]) -> Vec<String> {
        let mut tags = Vec::new();
        let threshold = self.config.similarity_threshold;

        for (category, cat_embedding) in self.category_embeddings() {
            let similarity = cosine_similarity(embedding, cat_embedding);
            if similarity >= threshold {
                tags.push(category.clone());
            }
        }

        // Sort by similarity (tags are already filtered by threshold)
        tags.sort();
        tags
    }
}

/// Compute cosine similarity between two vectors
///
/// Returns a value between -1.0 and 1.0, where 1.0 means identical direction
///
/// Optimized implementation that:
/// 1. Computes all three values (dot, norm_a, norm_b) in a single pass
/// 2. Uses SIMD-friendly loop pattern (the compiler auto-vectorizes this)
/// 3. Combines the final sqrt operations
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    // Single-pass computation of dot product and both norms
    // This pattern is SIMD-friendly and the compiler can auto-vectorize it
    let (dot, norm_a_sq, norm_b_sq) = a.iter().zip(b.iter()).fold(
        (0.0f32, 0.0f32, 0.0f32),
        |(dot, na, nb), (&x, &y)| (dot + x * y, na + x * x, nb + y * y),
    );

    // Fast path: avoid sqrt if either norm is zero
    if norm_a_sq == 0.0 || norm_b_sq == 0.0 {
        return 0.0;
    }

    // Combined sqrt for both norms (slightly faster than two separate sqrts)
    dot / (norm_a_sq * norm_b_sq).sqrt()
}

/// Compute cosine similarity when norm of vector `a` is pre-computed
///
/// This is useful when comparing one query vector against many documents,
/// as we only compute the query norm once.
#[allow(dead_code)]
pub fn cosine_similarity_with_norm(a: &[f32], a_norm: f32, b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() || a_norm == 0.0 {
        return 0.0;
    }

    let (dot, norm_b_sq) = a.iter().zip(b.iter()).fold(
        (0.0f32, 0.0f32),
        |(dot, nb), (&x, &y)| (dot + x * y, nb + y * y),
    );

    if norm_b_sq == 0.0 {
        return 0.0;
    }

    dot / (a_norm * norm_b_sq.sqrt())
}

/// Compute the L2 norm of a vector
#[allow(dead_code)]
pub fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
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
        assert!((cosine_similarity(&a, &b)).abs() < 0.001);
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
}
