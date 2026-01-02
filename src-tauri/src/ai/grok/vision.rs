//! Vision utilities for document analysis
//!
//! Handles image preparation and format conversion for Grok Vision API.

use image::{DynamicImage, ImageFormat};
use std::io::Cursor;
use std::path::Path;

/// Maximum image dimension (width or height)
const MAX_DIMENSION: u32 = 1600;

/// Prepare an image for Grok Vision API
///
/// - Resizes if too large
/// - Converts to JPEG for optimal size
/// - Returns base64-ready bytes
pub fn prepare_image_for_vision(image_data: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(image_data)
        .map_err(|e| format!("Failed to load image: {}", e))?;

    let img = resize_if_needed(img);

    // Encode as JPEG
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);

    img.write_to(&mut cursor, ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode image: {}", e))?;

    Ok(buffer)
}

/// Resize image if it exceeds maximum dimensions
fn resize_if_needed(img: DynamicImage) -> DynamicImage {
    let (width, height) = (img.width(), img.height());

    if width <= MAX_DIMENSION && height <= MAX_DIMENSION {
        return img;
    }

    let scale = (MAX_DIMENSION as f32 / width.max(height) as f32).min(1.0);
    let new_width = (width as f32 * scale) as u32;
    let new_height = (height as f32 * scale) as u32;

    img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
}

/// Check if a file extension is an image type we can analyze
pub fn is_image_extension(ext: Option<&str>) -> bool {
    match ext {
        Some(e) => matches!(
            e.to_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tiff" | "tif"
        ),
        None => false,
    }
}

/// Check if a file extension is a document we should analyze
pub fn is_analyzable_extension(ext: Option<&str>) -> bool {
    match ext {
        Some(e) => matches!(
            e.to_lowercase().as_str(),
            // PDFs
            "pdf" |
            // Images
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tiff" | "tif" |
            // Office documents (will need conversion)
            "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
        ),
        None => false,
    }
}

/// Check if a file extension is a text file we can read directly
pub fn is_text_extension(ext: Option<&str>) -> bool {
    match ext {
        Some(e) => matches!(
            e.to_lowercase().as_str(),
            "txt" | "md" | "csv" | "json" | "xml" | "html" | "htm" | "yaml" | "yml" | "log"
                | "ini" | "cfg" | "conf" | "py" | "js" | "ts" | "rs" | "go" | "java" | "c"
                | "cpp" | "h" | "hpp" | "css" | "scss" | "less" | "sql" | "sh" | "bash"
                | "zsh" | "toml" | "env"
        ),
        None => false,
    }
}

/// Estimate tokens needed for an image
/// Based on Grok's vision pricing model
pub fn estimate_image_tokens(image_bytes: usize, detail: &str) -> u32 {
    match detail {
        "low" => 85, // Fixed cost for low detail
        "high" => {
            // ~170 tokens per 512x512 tile
            let tiles = (image_bytes / (512 * 512 * 3)).max(1);
            (tiles as u32 * 170) + 85
        }
        _ => 85,
    }
}

/// Load and prepare an image file for vision API
pub async fn load_image_for_vision(path: &Path) -> Result<Vec<u8>, String> {
    let data = tokio::fs::read(path).await
        .map_err(|e| format!("Failed to read image {}: {}", path.display(), e))?;

    prepare_image_for_vision(&data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_analyzable_extension() {
        assert!(is_analyzable_extension(Some("pdf")));
        assert!(is_analyzable_extension(Some("PDF")));
        assert!(is_analyzable_extension(Some("jpg")));
        assert!(is_analyzable_extension(Some("docx")));
        assert!(!is_analyzable_extension(Some("exe")));
        assert!(!is_analyzable_extension(None));
    }

    #[test]
    fn test_is_text_extension() {
        assert!(is_text_extension(Some("txt")));
        assert!(is_text_extension(Some("md")));
        assert!(is_text_extension(Some("json")));
        assert!(!is_text_extension(Some("pdf")));
        assert!(!is_text_extension(Some("jpg")));
    }

    #[test]
    fn test_estimate_image_tokens() {
        assert_eq!(estimate_image_tokens(1000, "low"), 85);
        assert!(estimate_image_tokens(1_000_000, "high") > 85);
    }
}
