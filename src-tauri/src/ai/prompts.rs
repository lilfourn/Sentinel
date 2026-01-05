/// System prompt for file renaming (Claude Sonnet)
pub const RENAME_SYSTEM_PROMPT: &str = r#"You are a file naming assistant. Your task is to generate a clean, descriptive kebab-case filename based on file content or metadata.

RULES:
1. Output ONLY the filename in kebab-case (lowercase, hyphens between words)
2. Keep names concise: 3-6 meaningful words maximum
3. Include relevant identifiers: dates (mmm-yy or yyyy), names, document types
4. Preserve the original file extension
5. Remove special characters, spaces, underscores
6. For dates, prefer formats like: jan24, oct-2024, q3-2024
7. If the content is unclear, use the original filename structure but cleaned up

EXAMPLES:
- Invoice from Apple dated October 2024 -> invoice-apple-oct24.pdf
- Screenshot 2024-12-28 at 10.30.45 AM -> screenshot-2024-12-28.png
- Meeting notes with John about Q4 planning -> meeting-notes-john-q4-planning.md
- IMG_20241215_143022 (photo of cat) -> photo-cat-dec24.jpg
- resume_final_v3_FINAL.docx -> resume-final.docx
- Document (1).pdf -> document.pdf
- bank-statement-december.pdf -> bank-statement-dec24.pdf"#;

/// Build user prompt for file renaming
pub fn build_rename_prompt(
    filename: &str,
    extension: Option<&str>,
    size: u64,
    content_preview: Option<&str>,
) -> String {
    let mut prompt = format!(
        r#"Analyze this file and suggest a kebab-case filename:

FILENAME: {}
EXTENSION: {}
FILE_SIZE: {} bytes"#,
        filename,
        extension.unwrap_or("unknown"),
        size
    );

    if let Some(content) = content_preview {
        prompt.push_str(&format!(
            r#"

CONTENT PREVIEW (first 4KB):
---
{}
---"#,
            content
        ));
    }

    prompt.push_str("\n\nRespond with ONLY the new filename including extension. No explanation.");

    prompt
}
