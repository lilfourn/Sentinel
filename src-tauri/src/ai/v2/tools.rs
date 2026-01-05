//! V2 tool definitions for the semantic, rule-based agent.
//!
//! Four new tools replace the shell-based approach:
//! 1. query_semantic_index - Search files by semantic similarity
//! 2. apply_organization_rules - Define rules for bulk file operations
//! 3. preview_operations - Preview planned changes
//! 4. commit_plan - Finalize and submit the plan

use crate::ai::tools::ToolDefinition;
use crate::jobs::OrganizePlan;
use crate::utils::format_size;

use super::vfs::{OperationType, OrganizationRule, ShadowVFS};
use serde_json::json;

/// Get V2 tool definitions for the agent
pub fn get_v2_organize_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "query_semantic_index".to_string(),
            description: "Search files by semantic query. Returns ranked matches.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "filter_ext": { "type": "array", "items": { "type": "string" } },
                    "max_results": { "type": "integer", "default": 20 },
                    "min_similarity": { "type": "number", "default": 0.6 }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "apply_organization_rules".to_string(),
            description: "Apply DSL rules to generate file operations. See system prompt for DSL syntax.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "rules": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "if": { "type": "string" },
                                "thenMoveTo": { "type": "string" },
                                "thenRenameTo": { "type": "string" },
                                "priority": { "type": "integer" }
                            },
                            "required": ["name", "if"]
                        }
                    },
                    "mode": { "type": "string", "enum": ["append", "replace"], "default": "append" }
                },
                "required": ["rules"]
            }),
        },
        ToolDefinition {
            name: "preview_operations".to_string(),
            description: "Preview planned operations before committing.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "group_by": {
                        "type": "string",
                        "enum": ["operation_type", "destination_folder"],
                        "default": "operation_type"
                    },
                    "include_unchanged": { "type": "boolean", "default": false }
                }
            }),
        },
        ToolDefinition {
            name: "commit_plan".to_string(),
            description: "Finalize plan. Call ONCE when satisfied.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "description": { "type": "string" },
                    "confirm": { "type": "boolean" },
                    "dry_run": { "type": "boolean", "default": false }
                },
                "required": ["description", "confirm"]
            }),
        },
        ToolDefinition {
            name: "inspect_pattern_sample".to_string(),
            description: "Get sample files from a pattern to check dates/content. Use to examine a specific pattern more closely.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern_regex": {
                        "type": "string",
                        "description": "Regex pattern to match files (e.g., 'IMG_\\\\d+' for IMG_0001, IMG_0002, etc.)"
                    },
                    "max_samples": {
                        "type": "integer",
                        "description": "Maximum number of sample files to return (default 5)",
                        "default": 5
                    }
                },
                "required": ["pattern_regex"]
            }),
        },
    ]
}

/// Result of executing a V2 tool
pub enum V2ToolResult {
    /// Tool executed successfully, continue the loop
    Continue(String),
    /// Plan is ready to commit
    Commit(OrganizePlan),
    /// Tool execution failed
    Error(String),
}

/// Execute a V2 tool
pub fn execute_v2_tool(
    name: &str,
    input: &serde_json::Value,
    vfs: &mut ShadowVFS,
) -> V2ToolResult {
    match name {
        "query_semantic_index" => execute_query_semantic(input, vfs),
        "apply_organization_rules" => execute_apply_rules(input, vfs),
        "preview_operations" => execute_preview(input, vfs),
        "commit_plan" => execute_commit(input, vfs),
        "inspect_pattern_sample" => execute_inspect_pattern_sample(input, vfs),
        _ => V2ToolResult::Error(format!("Unknown tool: {}", name)),
    }
}

fn execute_query_semantic(input: &serde_json::Value, vfs: &ShadowVFS) -> V2ToolResult {
    let query = match input.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return V2ToolResult::Error("Missing 'query' parameter".to_string()),
    };

    let filter_ext: Option<Vec<String>> = input
        .get("filter_ext")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

    let min_size_bytes = input
        .get("min_size_bytes")
        .and_then(|v| v.as_u64());

    let max_results = input
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .min(30) as usize;  // Cap at 30 to reduce token usage

    let min_similarity = input
        .get("min_similarity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.6) as f32;

    eprintln!(
        "[V2Tool] query_semantic_index: query='{}', max_results={}",
        query, max_results
    );

    let results = vfs.query_semantic(
        query,
        filter_ext.as_deref(),
        min_size_bytes,
        max_results,
        min_similarity,
    );

    if results.is_empty() {
        return V2ToolResult::Continue("No files found matching the query.".to_string());
    }

    // Format results
    let mut output = format!("Found {} matching files:\n\n", results.len());
    for (file, score) in &results {
        output.push_str(&format!(
            "- {} (ext: {}, size: {}, similarity: {:.2})\n",
            file.name,
            file.ext.as_deref().unwrap_or("none"),
            format_size(file.size),
            score
        ));
    }

    V2ToolResult::Continue(output)
}

fn execute_apply_rules(input: &serde_json::Value, vfs: &mut ShadowVFS) -> V2ToolResult {
    // Debug: log the full input structure
    eprintln!("[V2Tool] apply_organization_rules input: {}", serde_json::to_string_pretty(input).unwrap_or_default());

    let rules_json = match input.get("rules").and_then(|v| v.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        Some(_) => {
            // Empty array provided - guide AI to provide rules
            return V2ToolResult::Error(
                "The 'rules' array is empty. Please provide at least one rule.\n\n\
                Example rule format:\n\
                {\n  \"rules\": [\n    {\n      \"name\": \"Organize PDFs\",\n      \
                \"if\": \"file.ext == 'pdf'\",\n      \"thenMoveTo\": \"Documents/PDFs\"\n    }\n  ]\n}".to_string()
            );
        }
        None => {
            // Missing rules key entirely
            let keys: Vec<&str> = input.as_object().map(|o| o.keys().map(|s| s.as_str()).collect()).unwrap_or_default();
            return V2ToolResult::Error(format!(
                "Missing 'rules' array. You provided keys: {:?}\n\n\
                Please provide rules in this format:\n\
                {{\n  \"rules\": [\n    {{\n      \"name\": \"Rule Name\",\n      \
                \"if\": \"file.ext == 'pdf'\",\n      \"thenMoveTo\": \"FolderName\"\n    }}\n  ]\n}}",
                keys
            ));
        }
    };

    let mode = input
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("append");

    // Parse rules from JSON
    let rules: Result<Vec<OrganizationRule>, _> = rules_json
        .iter()
        .map(|r| serde_json::from_value(r.clone()))
        .collect();

    let rules = match rules {
        Ok(r) => r,
        Err(e) => return V2ToolResult::Error(format!("Failed to parse rules: {}", e)),
    };

    eprintln!("[V2Tool] apply_organization_rules: {} rules, mode={}", rules.len(), mode);

    match vfs.apply_rules(&rules, mode) {
        Ok(result) => {
            let mut output = format!(
                "Applied {} of {} rules, generated {} operations.\nTotal operations in plan: {}",
                result.rules_applied,
                rules.len(),
                result.operations_created,
                vfs.operations().len()
            );

            // If there were parsing errors, report them so the AI can self-correct
            if !result.parsing_errors.is_empty() {
                output.push_str("\n\n## PARSING ERRORS - Please fix these rules:\n");
                output.push_str("The following rules had invalid syntax and were skipped:\n\n");
                for (rule_name, error) in &result.parsing_errors {
                    output.push_str(&format!("- **{}**: {}\n", rule_name, error));
                }
                output.push_str("\n### How to fix:\n");
                output.push_str("- Fields must come after 'file.' (e.g., `file.ext`, `file.name`)\n");
                output.push_str("- Valid fields: `name`, `ext`, `size`, `path`, `modifiedAt`, `createdAt`, `mimeType`, `isHidden`\n");
                output.push_str("- Use `==` not `=` for comparison\n");
                output.push_str("- String values must be quoted: `file.ext == 'pdf'`\n");
                output.push_str("- Functions only work on `file.name`: `file.name.contains('text')`\n");
                output.push_str("\nPlease retry with corrected rule syntax.");
            }

            V2ToolResult::Continue(output)
        }
        Err(e) => V2ToolResult::Error(format!("Failed to apply rules: {}", e)),
    }
}

fn execute_preview(input: &serde_json::Value, vfs: &ShadowVFS) -> V2ToolResult {
    let group_by = input
        .get("group_by")
        .and_then(|v| v.as_str())
        .unwrap_or("operation_type");

    let include_unchanged = input
        .get("include_unchanged")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    eprintln!("[V2Tool] preview_operations: group_by={}", group_by);

    let preview = vfs.preview_operations(group_by, include_unchanged);

    if preview.total_operations == 0 {
        return V2ToolResult::Continue("No operations planned. Use apply_organization_rules first.".to_string());
    }

    let mut output = format!(
        "Operation Preview (grouped by {})\n",
        group_by
    );
    output.push_str(&format!("Total operations: {}\n", preview.total_operations));

    if include_unchanged {
        output.push_str(&format!("Unchanged files: {}\n", preview.unchanged_files));
    }

    output.push('\n');

    // Sort groups for consistent output
    let mut sorted_groups: Vec<_> = preview.groups.iter().collect();
    sorted_groups.sort_by_key(|(k, _)| k.as_str());

    for (group_name, ops) in sorted_groups {
        output.push_str(&format!("## {} ({} operations)\n", group_name, ops.len()));

        for op in ops.iter().take(10) {
            // Limit preview per group
            match op.op_type {
                OperationType::CreateFolder => {
                    output.push_str(&format!(
                        "  - CREATE FOLDER: {}\n",
                        op.path.as_deref().unwrap_or("?")
                    ));
                }
                OperationType::Move => {
                    output.push_str(&format!(
                        "  - MOVE: {} -> {}\n",
                        op.source.as_deref().unwrap_or("?"),
                        op.destination.as_deref().unwrap_or("?")
                    ));
                }
                OperationType::Rename => {
                    output.push_str(&format!(
                        "  - RENAME: {} -> {}\n",
                        op.path.as_deref().unwrap_or("?"),
                        op.new_name.as_deref().unwrap_or("?")
                    ));
                }
                OperationType::Trash => {
                    output.push_str(&format!(
                        "  - TRASH: {}\n",
                        op.path.as_deref().unwrap_or("?")
                    ));
                }
            }
        }

        if ops.len() > 10 {
            output.push_str(&format!("  ... and {} more\n", ops.len() - 10));
        }

        output.push('\n');
    }

    // Truncate output if too large to prevent context overflow (4KB max to save tokens)
    const MAX_PREVIEW_SIZE: usize = 4000;
    if output.len() > MAX_PREVIEW_SIZE {
        // Count operation types for summary
        let mut creates = 0;
        let mut moves = 0;
        let mut renames = 0;
        let mut trashes = 0;
        for ops in preview.groups.values() {
            for op in ops {
                match op.op_type {
                    OperationType::CreateFolder => creates += 1,
                    OperationType::Move => moves += 1,
                    OperationType::Rename => renames += 1,
                    OperationType::Trash => trashes += 1,
                }
            }
        }
        let truncated = format!(
            "{}...\n\n[Preview truncated]\nSummary: {} total operations ({} creates, {} moves, {} renames, {} deletes) across {} folders\n",
            &output[..MAX_PREVIEW_SIZE.min(output.len())],
            preview.total_operations,
            creates, moves, renames, trashes,
            preview.groups.len()
        );
        return V2ToolResult::Continue(truncated);
    }

    V2ToolResult::Continue(output)
}

fn execute_commit(input: &serde_json::Value, vfs: &ShadowVFS) -> V2ToolResult {
    let description = match input.get("description").and_then(|v| v.as_str()) {
        Some(d) => d,
        None => return V2ToolResult::Error("Missing 'description' parameter".to_string()),
    };

    let confirm = input
        .get("confirm")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let dry_run = input
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !confirm {
        return V2ToolResult::Error(
            "Must set 'confirm: true' to commit the plan".to_string(),
        );
    }

    eprintln!(
        "[V2Tool] commit_plan: description='{}', dry_run={}",
        description, dry_run
    );

    let operations = vfs.operations();

    if operations.is_empty() {
        // Return an empty plan - folder is already organized
        return V2ToolResult::Commit(OrganizePlan {
            plan_id: format!("plan-{}", chrono::Utc::now().timestamp_millis()),
            description: description.to_string(),
            operations: Vec::new(),
            target_folder: vfs.root().to_string_lossy().to_string(),
            simplification_recommended: None,
        });
    }

    // Convert to OrganizeOperation format
    let organize_ops: Vec<crate::jobs::OrganizeOperation> = operations
        .iter()
        .map(|op| crate::jobs::OrganizeOperation {
            op_id: op.op_id.clone(),
            op_type: op.op_type.to_string(),
            source: op.source.clone(),
            destination: op.destination.clone(),
            path: op.path.clone(),
            new_name: op.new_name.clone(),
        })
        .collect();

    let plan = OrganizePlan {
        plan_id: format!("plan-{}", chrono::Utc::now().timestamp_millis()),
        description: description.to_string(),
        operations: organize_ops,
        target_folder: vfs.root().to_string_lossy().to_string(),
        simplification_recommended: None,
    };

    if dry_run {
        // Return as a preview
        let output = format!(
            "Dry run - plan would contain {} operations:\n{}",
            plan.operations.len(),
            serde_json::to_string_pretty(&plan).unwrap_or_default()
        );
        V2ToolResult::Continue(output)
    } else {
        V2ToolResult::Commit(plan)
    }
}

/// V5: Execute inspect_pattern_sample tool
///
/// Returns sample files matching a regex pattern for detailed inspection.
fn execute_inspect_pattern_sample(input: &serde_json::Value, vfs: &ShadowVFS) -> V2ToolResult {
    let pattern = match input.get("pattern_regex").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return V2ToolResult::Error("Missing 'pattern_regex' parameter".to_string()),
    };

    let max_samples = input
        .get("max_samples")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

    eprintln!(
        "[V2Tool] inspect_pattern_sample: pattern='{}', max_samples={}",
        pattern, max_samples
    );

    // Compile the regex
    let regex = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return V2ToolResult::Error(format!("Invalid regex pattern: {}", e)),
    };

    // Find matching files
    let files = vfs.files();
    let matching: Vec<_> = files
        .iter()
        .filter(|f| regex.is_match(&f.name))
        .collect();

    if matching.is_empty() {
        return V2ToolResult::Continue(format!(
            "No files matched pattern '{}'. Check the regex syntax.",
            pattern
        ));
    }

    // Get samples: first, middle, last, and some random
    let mut samples = Vec::new();
    let len = matching.len();

    // First file
    samples.push(matching[0]);

    // Middle file(s)
    if len > 2 {
        samples.push(matching[len / 2]);
    }

    // Last file
    if len > 1 {
        samples.push(matching[len - 1]);
    }

    // Additional quarter points if we have more samples to fill
    if samples.len() < max_samples && len > 4 {
        let q1 = len / 4;
        let q3 = (len * 3) / 4;
        // Check by path to avoid needing PartialEq
        let sample_paths: Vec<&str> = samples.iter().map(|f| f.path.as_str()).collect();
        if q1 > 0 && !sample_paths.contains(&matching[q1].path.as_str()) {
            samples.push(matching[q1]);
        }
        if q3 < len && !sample_paths.contains(&matching[q3].path.as_str()) {
            samples.push(matching[q3]);
        }
    }

    samples.truncate(max_samples);

    // Format output
    let mut output = format!(
        "Pattern '{}' matched {} files. Here are {} samples:\n\n",
        pattern,
        matching.len(),
        samples.len()
    );

    for file in &samples {
        let modified = file.modified_at
            .and_then(|ts| chrono::DateTime::from_timestamp_millis(ts))
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        output.push_str(&format!(
            "- {} (ext: {}, size: {}, modified: {})\n",
            file.name,
            file.ext.as_deref().unwrap_or("none"),
            format_size(file.size),
            modified
        ));
    }

    if matching.len() > samples.len() {
        output.push_str(&format!(
            "\n... and {} more files match this pattern\n",
            matching.len() - samples.len()
        ));
    }

    V2ToolResult::Continue(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions() {
        let tools = get_v2_organize_tools();
        assert_eq!(tools.len(), 5);

        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"query_semantic_index"));
        assert!(names.contains(&"apply_organization_rules"));
        assert!(names.contains(&"preview_operations"));
        assert!(names.contains(&"commit_plan"));
        assert!(names.contains(&"inspect_pattern_sample"));
    }
}
