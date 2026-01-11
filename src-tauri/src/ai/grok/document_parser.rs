//! Document Parser Module
//!
//! Pure Rust text extraction from documents - no external dependencies required.
//! Works out of the box without Tesseract, pdfium, or any other system libraries.
//!
//! ## Supported Formats
//! - PDF: Text extraction via pdf-extract
//! - Excel: .xlsx, .xls via calamine
//! - Word: .docx via docx-rs
//! - Text: .txt, .md, .csv, .json, .xml, .html (direct read)
//!
//! ## Strategy
//! 1. Try text extraction first (fast, pure Rust)
//! 2. For scanned/image PDFs, fall back to Vision API

use calamine::{open_workbook, Reader, Xlsx, Xls};
use std::path::Path;

/// Maximum text length to extract (to avoid memory issues with huge docs)
const MAX_TEXT_LENGTH: usize = 500_000; // ~500KB of text

/// Minimum text length to consider extraction successful
const MIN_TEXT_LENGTH: usize = 50;

/// Result of document parsing
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    /// Extracted text content
    pub text: String,
    /// Document metadata (title, author, etc.)
    pub metadata: DocumentMetadata,
    /// Whether OCR was used (always false for pure Rust - Vision API handles OCR)
    #[allow(dead_code)]
    pub used_ocr: bool,
    /// Extraction method used
    pub method: ExtractionMethod,
}

/// Document metadata from extraction
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    #[allow(dead_code)]
    pub subject: Option<String>,
    #[allow(dead_code)]
    pub creation_date: Option<String>,
    pub page_count: Option<u32>,
    pub word_count: Option<u32>,
}

/// How the document was parsed
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ExtractionMethod {
    /// Native text extraction (fastest)
    NativeText,
    /// OCR (handled by Vision API, not this module)
    Ocr,
    /// Simple file read (for plain text files)
    DirectRead,
    /// Failed to extract
    Failed,
}

/// Document parser using pure Rust crates
pub struct DocumentParser;

impl DocumentParser {
    /// Create a new document parser
    pub fn new() -> Self {
        tracing::info!("[DocumentParser] Initialized with pure Rust extraction (no external deps)");
        Self
    }

    /// Parse a document and extract text
    pub fn parse(&self, path: &Path) -> Result<ParsedDocument, String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());

        match ext.as_deref() {
            // Plain text files - read directly
            Some(e) if Self::is_plain_text_ext(e) => self.read_plain_text(path),

            // PDF files
            Some("pdf") => self.extract_pdf(path),

            // Excel files
            Some("xlsx") => self.extract_xlsx(path),
            Some("xls") => self.extract_xls(path),

            // Word documents
            Some("docx") => self.extract_docx(path),

            // HTML files
            Some("html") | Some("htm") => self.read_plain_text(path),

            // Unsupported - let Vision API handle it
            _ => Err(format!("Unsupported file type for text extraction: {:?}", ext)),
        }
    }

    /// Check if extension is plain text
    fn is_plain_text_ext(ext: &str) -> bool {
        matches!(
            ext,
            "txt" | "md" | "csv" | "json" | "xml" | "yaml" | "yml" | "log" | "ini" | "cfg"
                | "conf" | "toml" | "env" | "sh" | "bash" | "zsh" | "rs" | "ts" | "js" | "py"
        )
    }

    /// Read plain text file directly
    fn read_plain_text(&self, path: &Path) -> Result<ParsedDocument, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read text file: {}", e))?;

        let text = Self::truncate_text(&text);
        let word_count = text.split_whitespace().count() as u32;

        tracing::debug!(
            "[DocumentParser] Direct read: {} chars, {} words from {}",
            text.len(),
            word_count,
            path.display()
        );

        Ok(ParsedDocument {
            text,
            metadata: DocumentMetadata {
                word_count: Some(word_count),
                ..Default::default()
            },
            used_ocr: false,
            method: ExtractionMethod::DirectRead,
        })
    }

    /// Extract text from PDF using pdf-extract
    /// Wrapped in catch_unwind to handle panics from malformed PDFs
    fn extract_pdf(&self, path: &Path) -> Result<ParsedDocument, String> {
        tracing::info!("[DocumentParser] Starting PDF extraction: {}", path.display());

        let bytes = std::fs::read(path)
            .map_err(|e| format!("Failed to read PDF file: {}", e))?;

        tracing::debug!("[DocumentParser] PDF file size: {} bytes", bytes.len());

        // Use catch_unwind to handle panics from malformed PDFs
        // The pdf_extract crate (and its cff-parser dependency) can panic on certain fonts/glyphs
        let text = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_extract::extract_text_from_mem(&bytes)
        })) {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                tracing::warn!(
                    "[DocumentParser] PDF extraction FAILED for {}: {}",
                    path.display(),
                    e
                );
                return Err(format!("PDF extraction failed: {}", e));
            }
            Err(_panic) => {
                tracing::error!(
                    "[DocumentParser] PDF extraction PANICKED for {} - likely malformed font/glyph",
                    path.display()
                );
                return Err("PDF extraction panicked - likely contains malformed fonts".to_string());
            }
        };

        let raw_len = text.len();
        let text = Self::clean_text(&text);

        tracing::info!(
            "[DocumentParser] PDF raw extraction: {} chars -> {} chars after cleaning from {}",
            raw_len,
            text.len(),
            path.file_name().unwrap_or_default().to_string_lossy()
        );

        // Show first 200 chars for debugging
        if !text.is_empty() {
            let preview: String = text.chars().take(200).collect();
            tracing::debug!("[DocumentParser] Content preview: {}...", preview);
        }

        if text.len() < MIN_TEXT_LENGTH {
            tracing::warn!(
                "[DocumentParser] PDF text too short ({} chars < {}) - likely scanned/image: {}",
                text.len(),
                MIN_TEXT_LENGTH,
                path.display()
            );
            return Err(format!(
                "PDF text too short ({} chars) - likely scanned/image-based",
                text.len()
            ));
        }

        let text = Self::truncate_text(&text);
        let word_count = text.split_whitespace().count() as u32;

        // Try to estimate page count from content (rough heuristic)
        let page_count = (text.len() / 3000).max(1) as u32;

        tracing::info!(
            "[DocumentParser] PDF SUCCESS: {} chars, {} words, ~{} pages from {}",
            text.len(),
            word_count,
            page_count,
            path.file_name().unwrap_or_default().to_string_lossy()
        );

        Ok(ParsedDocument {
            text,
            metadata: DocumentMetadata {
                word_count: Some(word_count),
                page_count: Some(page_count),
                ..Default::default()
            },
            used_ocr: false,
            method: ExtractionMethod::NativeText,
        })
    }

    /// Extract text from XLSX using calamine
    fn extract_xlsx(&self, path: &Path) -> Result<ParsedDocument, String> {
        tracing::debug!("[DocumentParser] Extracting XLSX: {}", path.display());

        let mut workbook: Xlsx<_> = open_workbook(path)
            .map_err(|e| format!("Failed to open XLSX: {}", e))?;

        let mut all_text = String::new();
        let sheet_names: Vec<String> = workbook.sheet_names().to_vec();

        for sheet_name in &sheet_names {
            if let Ok(range) = workbook.worksheet_range(sheet_name) {
                all_text.push_str(&format!("\n=== Sheet: {} ===\n", sheet_name));

                for row in range.rows() {
                    let row_text: Vec<String> = row
                        .iter()
                        .map(|cell| cell.to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

                    if !row_text.is_empty() {
                        all_text.push_str(&row_text.join(" | "));
                        all_text.push('\n');
                    }
                }
            }
        }

        let text = Self::clean_text(&all_text);

        if text.len() < MIN_TEXT_LENGTH {
            return Err(format!(
                "Excel content too short ({} chars)",
                text.len()
            ));
        }

        let text = Self::truncate_text(&text);
        let word_count = text.split_whitespace().count() as u32;

        tracing::info!(
            "[DocumentParser] XLSX extracted: {} chars, {} words, {} sheets from {}",
            text.len(),
            word_count,
            sheet_names.len(),
            path.display()
        );

        Ok(ParsedDocument {
            text,
            metadata: DocumentMetadata {
                word_count: Some(word_count),
                page_count: Some(sheet_names.len() as u32),
                ..Default::default()
            },
            used_ocr: false,
            method: ExtractionMethod::NativeText,
        })
    }

    /// Extract text from XLS (older Excel format) using calamine
    fn extract_xls(&self, path: &Path) -> Result<ParsedDocument, String> {
        tracing::debug!("[DocumentParser] Extracting XLS: {}", path.display());

        let mut workbook: Xls<_> = open_workbook(path)
            .map_err(|e| format!("Failed to open XLS: {}", e))?;

        let mut all_text = String::new();
        let sheet_names: Vec<String> = workbook.sheet_names().to_vec();

        for sheet_name in &sheet_names {
            if let Ok(range) = workbook.worksheet_range(sheet_name) {
                all_text.push_str(&format!("\n=== Sheet: {} ===\n", sheet_name));

                for row in range.rows() {
                    let row_text: Vec<String> = row
                        .iter()
                        .map(|cell| cell.to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

                    if !row_text.is_empty() {
                        all_text.push_str(&row_text.join(" | "));
                        all_text.push('\n');
                    }
                }
            }
        }

        let text = Self::clean_text(&all_text);

        if text.len() < MIN_TEXT_LENGTH {
            return Err(format!(
                "Excel content too short ({} chars)",
                text.len()
            ));
        }

        let text = Self::truncate_text(&text);
        let word_count = text.split_whitespace().count() as u32;

        tracing::info!(
            "[DocumentParser] XLS extracted: {} chars, {} words from {}",
            text.len(),
            word_count,
            path.display()
        );

        Ok(ParsedDocument {
            text,
            metadata: DocumentMetadata {
                word_count: Some(word_count),
                page_count: Some(sheet_names.len() as u32),
                ..Default::default()
            },
            used_ocr: false,
            method: ExtractionMethod::NativeText,
        })
    }

    /// Extract text from DOCX using docx-rs
    fn extract_docx(&self, path: &Path) -> Result<ParsedDocument, String> {
        tracing::debug!("[DocumentParser] Extracting DOCX: {}", path.display());

        let bytes = std::fs::read(path)
            .map_err(|e| format!("Failed to read DOCX file: {}", e))?;

        let doc = docx_rs::read_docx(&bytes)
            .map_err(|e| format!("Failed to parse DOCX: {}", e))?;

        let mut all_text = String::new();

        // Extract text from document body
        for child in doc.document.children {
            Self::extract_docx_content(&child, &mut all_text);
        }

        let text = Self::clean_text(&all_text);

        if text.len() < MIN_TEXT_LENGTH {
            return Err(format!(
                "DOCX content too short ({} chars)",
                text.len()
            ));
        }

        let text = Self::truncate_text(&text);
        let word_count = text.split_whitespace().count() as u32;

        tracing::info!(
            "[DocumentParser] DOCX extracted: {} chars, {} words from {}",
            text.len(),
            word_count,
            path.display()
        );

        Ok(ParsedDocument {
            text,
            metadata: DocumentMetadata {
                word_count: Some(word_count),
                ..Default::default()
            },
            used_ocr: false,
            method: ExtractionMethod::NativeText,
        })
    }

    /// Recursively extract text from DOCX document elements
    fn extract_docx_content(element: &docx_rs::DocumentChild, output: &mut String) {
        match element {
            docx_rs::DocumentChild::Paragraph(para) => {
                for child in &para.children {
                    match child {
                        docx_rs::ParagraphChild::Run(run) => {
                            for run_child in &run.children {
                                if let docx_rs::RunChild::Text(text) = run_child {
                                    output.push_str(&text.text);
                                }
                            }
                        }
                        docx_rs::ParagraphChild::Hyperlink(link) => {
                            for run in &link.children {
                                if let docx_rs::ParagraphChild::Run(r) = run {
                                    for run_child in &r.children {
                                        if let docx_rs::RunChild::Text(text) = run_child {
                                            output.push_str(&text.text);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                output.push('\n');
            }
            docx_rs::DocumentChild::Table(table) => {
                for row in &table.rows {
                    let docx_rs::TableChild::TableRow(tr) = row;
                    for cell in &tr.cells {
                        let docx_rs::TableRowChild::TableCell(tc) = cell;
                        for child in &tc.children {
                            if let docx_rs::TableCellContent::Paragraph(para) = child {
                                for p_child in &para.children {
                                    if let docx_rs::ParagraphChild::Run(run) = p_child {
                                        for run_child in &run.children {
                                            if let docx_rs::RunChild::Text(text) = run_child {
                                                output.push_str(&text.text);
                                            }
                                        }
                                    }
                                }
                                output.push_str(" | ");
                            }
                        }
                    }
                    output.push('\n');
                }
            }
            _ => {}
        }
    }

    /// Clean extracted text
    fn clean_text(text: &str) -> String {
        text.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Truncate text to max length, preserving word boundaries
    fn truncate_text(text: &str) -> String {
        if text.len() <= MAX_TEXT_LENGTH {
            return text.to_string();
        }

        // Find a good break point (end of sentence or paragraph)
        let truncated = &text[..MAX_TEXT_LENGTH];

        // Try to break at paragraph
        if let Some(pos) = truncated.rfind("\n\n") {
            return truncated[..pos].to_string();
        }

        // Try to break at sentence
        if let Some(pos) = truncated.rfind(". ") {
            return truncated[..=pos].to_string();
        }

        // Fall back to word boundary
        if let Some(pos) = truncated.rfind(' ') {
            return truncated[..pos].to_string();
        }

        truncated.to_string()
    }

    /// Check if a file type is supported for text extraction
    pub fn is_supported(ext: Option<&str>) -> bool {
        match ext {
            Some(e) => matches!(
                e.to_lowercase().as_str(),
                // PDF
                "pdf" |
                // Office documents
                "docx" | "xlsx" | "xls" |
                // Text formats
                "txt" | "md" | "html" | "htm" | "xml" | "json" | "yaml" | "yml" |
                "csv" | "log" | "ini" | "cfg" | "conf" | "toml" | "env" |
                // Code files
                "rs" | "ts" | "js" | "py" | "sh" | "bash" | "zsh"
            ),
            None => false,
        }
    }

    /// Get a content preview suitable for AI analysis
    /// Returns a structured summary of the document
    #[allow(dead_code)]
    pub fn get_analysis_preview(&self, parsed: &ParsedDocument, max_chars: usize) -> String {
        let mut preview = String::new();

        // Add metadata context if available
        if let Some(ref title) = parsed.metadata.title {
            preview.push_str(&format!("Title: {}\n", title));
        }
        if let Some(ref author) = parsed.metadata.author {
            preview.push_str(&format!("Author: {}\n", author));
        }
        if let Some(pages) = parsed.metadata.page_count {
            preview.push_str(&format!("Pages: {}\n", pages));
        }
        if let Some(words) = parsed.metadata.word_count {
            preview.push_str(&format!("Words: {}\n", words));
        }

        if !preview.is_empty() {
            preview.push_str("\n---\n\n");
        }

        // Add content preview
        let remaining = max_chars.saturating_sub(preview.len());
        let content_preview: String = parsed.text.chars().take(remaining).collect();
        preview.push_str(&content_preview);

        preview
    }
}

impl Default for DocumentParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a document file and return extracted text
/// Convenience function for quick parsing
#[allow(dead_code)]
pub fn parse_document(path: &Path) -> Result<ParsedDocument, String> {
    let parser = DocumentParser::new();
    parser.parse(path)
}

/// Check if a file extension is supported for parsing
pub fn is_parseable(ext: Option<&str>) -> bool {
    DocumentParser::is_supported(ext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_plain_text_parsing() {
        let mut file = NamedTempFile::with_suffix(".txt").unwrap();
        writeln!(file, "This is a test document with some content.").unwrap();
        writeln!(file, "It has multiple lines and words.").unwrap();

        let parser = DocumentParser::new();
        let result = parser.parse(file.path());

        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.text.contains("test document"));
        assert_eq!(parsed.method, ExtractionMethod::DirectRead);
    }

    #[test]
    fn test_is_supported() {
        assert!(DocumentParser::is_supported(Some("pdf")));
        assert!(DocumentParser::is_supported(Some("docx")));
        assert!(DocumentParser::is_supported(Some("xlsx")));
        assert!(DocumentParser::is_supported(Some("txt")));
        assert!(!DocumentParser::is_supported(Some("exe")));
        assert!(!DocumentParser::is_supported(Some("mp4")));
    }

    #[test]
    fn test_truncate_text() {
        let long_text = "a ".repeat(300_000);
        let truncated = DocumentParser::truncate_text(&long_text);
        assert!(truncated.len() <= MAX_TEXT_LENGTH);
    }

    #[test]
    fn test_clean_text() {
        let messy = "  Line 1  \n\n  Line 2  \n  \n  Line 3  ";
        let cleaned = DocumentParser::clean_text(messy);
        assert_eq!(cleaned, "Line 1\nLine 2\nLine 3");
    }
}
