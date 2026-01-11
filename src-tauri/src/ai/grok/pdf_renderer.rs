//! PDF Rendering Module
//!
//! Converts PDF pages to images for Grok Vision analysis.
//! Uses a fallback chain: pdfium -> image extraction -> skip
//!
//! Note: pdfium-render requires the pdfium library to be installed.
//! On macOS: brew install pdfium
//! On Linux: apt install libpdfium-dev
//! On Windows: Download from https://github.com/nickelc/pdfium-binaries

use image::{DynamicImage, ImageFormat, RgbaImage};
use std::io::Cursor;
use std::path::Path;

/// Target DPI for PDF rendering (150 = good balance of quality and size)
#[allow(dead_code)]
const RENDER_DPI: f32 = 150.0;

/// Maximum page dimension in pixels
#[allow(dead_code)]
const MAX_PAGE_DIMENSION: u32 = 1600;

/// PDF renderer using available backends
#[allow(dead_code)]
pub struct PdfRenderer {
    /// Whether pdfium is available
    pdfium_available: bool,
}

impl PdfRenderer {
    /// Create a new PDF renderer, detecting available backends
    pub fn new() -> Self {
        // Check if pdfium is available
        let pdfium_available = Self::check_pdfium_available();

        if pdfium_available {
            tracing::info!("[PdfRenderer] Using pdfium backend");
        } else {
            tracing::warn!("[PdfRenderer] pdfium not available, using fallback");
        }

        Self { pdfium_available }
    }

    /// Check if pdfium library is available
    fn check_pdfium_available() -> bool {
        // Try to load pdfium dynamically
        #[cfg(feature = "pdfium")]
        {
            pdfium_render::prelude::Pdfium::default().is_ok()
        }
        #[cfg(not(feature = "pdfium"))]
        {
            false
        }
    }

    /// Render the first page of a PDF to an image
    pub async fn render_first_page(&self, path: &Path) -> Result<Vec<u8>, String> {
        self.render_page(path, 0).await
    }

    /// Render a specific page of a PDF
    pub async fn render_page(&self, path: &Path, page_index: usize) -> Result<Vec<u8>, String> {
        let path = path.to_path_buf();

        // Run in blocking task since PDF rendering is CPU-intensive
        tokio::task::spawn_blocking(move || {
            Self::render_page_blocking(&path, page_index)
        })
        .await
        .map_err(|e| format!("Task failed: {}", e))?
    }

    /// Render PDF page (blocking version)
    fn render_page_blocking(path: &Path, page_index: usize) -> Result<Vec<u8>, String> {
        #[cfg(feature = "pdfium")]
        {
            Self::render_with_pdfium(path, page_index)
        }
        #[cfg(not(feature = "pdfium"))]
        {
            Self::render_fallback(path, page_index)
        }
    }

    /// Render using pdfium (when available)
    #[cfg(feature = "pdfium")]
    fn render_with_pdfium(path: &Path, page_index: usize) -> Result<Vec<u8>, String> {
        use pdfium_render::prelude::*;

        let pdfium = Pdfium::default()
            .map_err(|e| format!("Failed to initialize pdfium: {}", e))?;

        let document = pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| format!("Failed to load PDF: {}", e))?;

        let page = document
            .pages()
            .get(page_index as u16)
            .map_err(|e| format!("Failed to get page {}: {}", page_index, e))?;

        // Calculate render size
        let page_width = page.width().value;
        let page_height = page.height().value;
        let scale = (MAX_PAGE_DIMENSION as f32 / page_width.max(page_height)).min(RENDER_DPI / 72.0);
        let render_width = (page_width * scale) as u32;
        let render_height = (page_height * scale) as u32;

        let config = PdfRenderConfig::new()
            .set_target_width(render_width as i32)
            .set_target_height(render_height as i32)
            .render_form_data(true)
            .render_annotations(true);

        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| format!("Failed to render page: {}", e))?;

        // Convert to image and encode as JPEG
        let image = bitmap.as_image();
        let mut buffer = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut buffer), ImageFormat::Jpeg)
            .map_err(|e| format!("Failed to encode image: {}", e))?;

        Ok(buffer)
    }

    /// Fallback rendering using pdf-extract for text overlay on blank image
    #[cfg(not(feature = "pdfium"))]
    fn render_fallback(path: &Path, _page_index: usize) -> Result<Vec<u8>, String> {
        // Create a placeholder image with text indicating PDF content
        // This is a degraded mode when pdfium is not available

        let width = 800u32;
        let height = 600u32;
        let mut img = RgbaImage::new(width, height);

        // Fill with light gray background
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([245, 245, 245, 255]);
        }

        // Try to extract text from PDF
        let text = Self::extract_pdf_text(path).unwrap_or_else(|_| {
            format!("PDF: {}", path.file_name().unwrap_or_default().to_string_lossy())
        });

        // For now, just return the placeholder
        // In production, you'd want to render text onto the image
        tracing::warn!(
            "[PdfRenderer] Using fallback for {}, extracted {} chars of text",
            path.display(),
            text.len()
        );

        let dynamic_image = DynamicImage::ImageRgba8(img);
        let mut buffer = Vec::new();
        dynamic_image
            .write_to(&mut Cursor::new(&mut buffer), ImageFormat::Jpeg)
            .map_err(|e| format!("Failed to encode image: {}", e))?;

        Ok(buffer)
    }

    /// Extract text from PDF as fallback
    #[cfg(not(feature = "pdfium"))]
    fn extract_pdf_text(path: &Path) -> Result<String, String> {
        // Read raw PDF and look for text streams
        // This is a very basic implementation
        let content = std::fs::read(path)
            .map_err(|e| format!("Failed to read PDF: {}", e))?;

        // Look for text in PDF content (very basic extraction)
        let content_str = String::from_utf8_lossy(&content);

        // Extract text between BT and ET markers (PDF text blocks)
        let mut text = String::new();
        let mut in_text_block = false;

        for line in content_str.lines() {
            if line.contains("BT") {
                in_text_block = true;
            } else if line.contains("ET") {
                in_text_block = false;
            } else if in_text_block {
                // Try to extract text from Tj or TJ operators
                if let Some(start) = line.find('(') {
                    if let Some(end) = line.rfind(')') {
                        text.push_str(&line[start + 1..end]);
                        text.push(' ');
                    }
                }
            }
        }

        if text.is_empty() {
            Err("No text found in PDF".to_string())
        } else {
            Ok(text)
        }
    }

    /// Get the number of pages in a PDF
    #[allow(dead_code)]
    pub async fn page_count(&self, path: &Path) -> Result<usize, String> {
        let path = path.to_path_buf();

        tokio::task::spawn_blocking(move || {
            Self::page_count_blocking(&path)
        })
        .await
        .map_err(|e| format!("Task failed: {}", e))?
    }

    #[allow(dead_code)]
    fn page_count_blocking(path: &Path) -> Result<usize, String> {
        #[cfg(feature = "pdfium")]
        {
            use pdfium_render::prelude::*;

            let pdfium = Pdfium::default()
                .map_err(|e| format!("Failed to initialize pdfium: {}", e))?;

            let document = pdfium
                .load_pdf_from_file(path, None)
                .map_err(|e| format!("Failed to load PDF: {}", e))?;

            Ok(document.pages().len() as usize)
        }
        #[cfg(not(feature = "pdfium"))]
        {
            // Estimate page count from file size (rough heuristic)
            let size = std::fs::metadata(path)
                .map_err(|e| format!("Failed to get file size: {}", e))?
                .len();

            // Rough estimate: ~50KB per page for average PDF
            Ok((size / 50_000).max(1) as usize)
        }
    }

    /// Render sample pages from a PDF (first, middle, last for long docs)
    #[allow(dead_code)]
    pub async fn render_sample_pages(
        &self,
        path: &Path,
        max_pages: usize,
    ) -> Result<Vec<Vec<u8>>, String> {
        let page_count = self.page_count(path).await?;

        let indices: Vec<usize> = if page_count <= max_pages {
            (0..page_count).collect()
        } else if max_pages == 1 {
            vec![0]
        } else if max_pages == 2 {
            vec![0, page_count - 1]
        } else {
            // First, middle, last
            vec![0, page_count / 2, page_count - 1]
        };

        let mut results = Vec::new();
        for index in indices {
            match self.render_page(path, index).await {
                Ok(image) => results.push(image),
                Err(e) => {
                    tracing::warn!("Failed to render page {} of {}: {}", index, path.display(), e);
                }
            }
        }

        if results.is_empty() {
            Err("Failed to render any pages".to_string())
        } else {
            Ok(results)
        }
    }
}

impl Default for PdfRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_creation() {
        // Creating a renderer should not panic
        let _renderer = PdfRenderer::new();
    }
}
