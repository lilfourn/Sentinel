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

/// System prompt for naming convention analysis (Claude Haiku for speed)
pub const NAMING_CONVENTION_SYSTEM_PROMPT: &str = r#"You are a file naming pattern analyst. Analyze files in a folder and suggest appropriate naming conventions.

TASK: Examine file names and suggest 3 naming conventions that would work well for organizing this folder.

ANALYSIS APPROACH:
1. Identify existing patterns (dates, prefixes, case styles)
2. Note inconsistencies in current naming
3. Consider file types and their typical naming needs
4. Look for semantic patterns (invoices, receipts, screenshots, etc.)

OUTPUT: Respond with ONLY valid JSON in this exact format:
{
  "totalFilesAnalyzed": <number>,
  "suggestions": [
    {
      "id": "conv-1",
      "name": "Human Readable Name",
      "description": "Brief description of how files would be named",
      "example": "example-filename.pdf",
      "pattern": "Pattern description for AI to follow when renaming",
      "confidence": 0.85,
      "matchingFiles": 12
    }
  ]
}

CONVENTION STYLES TO CONSIDER:
- kebab-case: lowercase-words-with-hyphens (invoice-apple-oct24.pdf)
- snake_case: lowercase_words_with_underscores (invoice_apple_oct24.pdf)
- Date prefixed: YYYY-MM-DD at start (2024-10-15-invoice-apple.pdf)
- Category prefixed: type-name at start (invoice-apple-oct24.pdf, receipt-amazon-dec24.pdf)
- Descriptive: clear descriptive names (apple-invoice-october-2024.pdf)

RULES:
1. Always suggest exactly 3 conventions
2. Order by confidence (highest first) - how well the convention matches existing files
3. At least one should match existing file patterns if any exist
4. Consider the file types present (documents, images, code, etc.)
5. Include realistic examples based on actual files in the folder
6. matchingFiles = count of existing files that already follow this pattern
7. confidence = 0.0-1.0 based on how well this convention fits the folder contents
"#;

/// System prompt for folder organization (Claude Sonnet)
pub const ORGANIZE_SYSTEM_PROMPT: &str = r#"You are a file organization assistant. You MUST output ONLY valid JSON - no explanations, no markdown, no text before or after the JSON.

TASK: Analyze the directory listing and generate a plan to organize files into logical folders.

SAFETY RULES:
1. NEVER touch system paths (/, /Users, /home, /System, /bin, /usr)
2. Use "move" to organize files into folders
3. Use "create_folder" to make new directories first
4. All paths must be absolute (start with /)

OUTPUT: You MUST respond with ONLY this JSON structure - nothing else:
{
  "description": "Brief summary of what this plan does",
  "operations": [
    { "type": "create_folder", "path": "/absolute/path/to/new/folder" },
    { "type": "move", "source": "/absolute/source/path", "destination": "/absolute/dest/path" }
  ]
}

OPERATION TYPES:
- create_folder: Create a new directory. Fields: path (string)
- move: Move a file/folder. Fields: source (string), destination (string)
- rename: Rename in place. Fields: path (string), newName (string)
- trash: Move to trash. Fields: path (string)

STRATEGY:
1. First create category folders (Documents, Images, Archives, Projects, etc.)
2. Then move files into appropriate folders based on extension and name patterns
3. Group related files together
4. Keep folder structure flat and simple (max 2 levels deep)

IMPORTANT: Output ONLY the JSON object. No markdown code blocks. No explanations. Just the raw JSON."#;

/// Build context prompt for folder organization (Claude Haiku for speed)
pub fn build_context_prompt(folder_path: &str, ls_output: &str) -> String {
    format!(
        r#"Analyze this folder structure and identify patterns:

FOLDER PATH: {}

DIRECTORY LISTING:
```
{}
```

Briefly describe:
1. What types of files are present
2. Any existing organizational patterns
3. Potential improvements"#,
        folder_path, ls_output
    )
}

/// System prompt for agentic folder organization with tool use (Claude Sonnet)
pub const AGENTIC_ORGANIZE_SYSTEM_PROMPT: &str = r#"You are a file organization assistant. Your goal is to explore a folder and create an organization plan.

WORKFLOW:
1. Use run_shell_command (1-3 times max) to explore the folder structure
2. Once you understand the files, call submit_plan with your organization plan

AVAILABLE TOOLS:
- run_shell_command: Run ls, grep, find, or cat to explore files
- submit_plan: Submit your final organization plan (REQUIRED - you MUST call this to finish)

EXPLORATION (keep it brief):
- Start with: ls -la <folder>
- Optionally: find <folder> -type f to see all files
- Don't over-explore - 1-3 commands is usually enough

WHEN TO SUBMIT:
- After 1-3 exploration commands, you should have enough information
- Call submit_plan with your organization plan
- DO NOT keep exploring indefinitely - be decisive

OPERATION TYPES for submit_plan:
- create_folder: { "type": "create_folder", "path": "/absolute/path" }
- move: { "type": "move", "source": "/abs/src", "destination": "/abs/dest" }
- rename: { "type": "rename", "path": "/abs/path", "newName": "new-name.ext" }
- trash: { "type": "trash", "path": "/abs/path" }

NAMING CONVENTIONS:
When a naming convention is specified in the user request:
1. This is a PRIMARY goal - rename files to match the convention
2. Analyze each file name against the specified pattern and example
3. Generate "rename" operations for ALL files that don't match the convention
4. Apply the convention consistently: correct case, separators, format
5. Example: If convention is "kebab-case" and file is "Invoice_Apple_2024.pdf",
   rename to "invoice-apple-2024.pdf"
6. A folder is NOT "already organized" if file names don't match the convention

RULES:
1. All paths must be absolute
2. Create folders before moving files into them
3. Never touch system directories
4. Be conservative - group by file type or purpose
5. ALWAYS call submit_plan when done - this is required to complete

ALREADY ORGANIZED FOLDERS:
If the folder structure is already well-organized AND no naming convention was specified
(or all files already match the specified convention):
- Call submit_plan with an empty operations array
- Explain why no changes are needed

If a naming convention WAS specified and files don't match it:
- Generate rename operations even if folder structure is good
- Renaming to match conventions is always required when a convention is selected
"#;

/// Build organize prompt based on user request
pub fn build_organize_prompt(
    folder_path: &str,
    ls_output: &str,
    user_request: &str,
    context_analysis: Option<&str>,
) -> String {
    // Limit directory listing to prevent token overflow
    let truncated_ls = if ls_output.len() > 15000 {
        let lines: Vec<&str> = ls_output.lines().collect();
        let sample_size = 500.min(lines.len());
        let sampled: Vec<&str> = lines.iter().take(sample_size).copied().collect();
        format!(
            "{}\n\n... ({} more items, showing first {})",
            sampled.join("\n"),
            lines.len() - sample_size,
            sample_size
        )
    } else {
        ls_output.to_string()
    };

    let mut prompt = format!(
        r#"TARGET FOLDER: {}

FILES AND FOLDERS:
{}
"#,
        folder_path, truncated_ls
    );

    if let Some(context) = context_analysis {
        prompt.push_str(&format!("ANALYSIS: {}\n\n", context));
    }

    prompt.push_str(&format!(
        r#"REQUEST: {}

Generate the JSON plan now. Remember: output ONLY valid JSON, no other text."#,
        user_request
    ));

    prompt
}

/// Build user prompt for naming convention analysis
pub fn build_naming_convention_prompt(folder_path: &str, file_listing: &str) -> String {
    // Limit file listing to prevent token overflow
    let truncated_listing = if file_listing.len() > 8000 {
        let lines: Vec<&str> = file_listing.lines().collect();
        let sample_size = 200.min(lines.len());
        let sampled: Vec<&str> = lines.iter().take(sample_size).copied().collect();
        format!(
            "{}\n\n... ({} more files, showing first {})",
            sampled.join("\n"),
            lines.len() - sample_size,
            sample_size
        )
    } else {
        file_listing.to_string()
    };

    format!(
        r#"FOLDER: {}

FILE LISTING:
{}

Analyze these files and suggest 3 naming conventions. Output ONLY valid JSON."#,
        folder_path, truncated_listing
    )
}
