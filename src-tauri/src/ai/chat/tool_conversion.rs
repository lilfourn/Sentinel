//! Tool format conversion between Claude and OpenAI
//!
//! Claude and OpenAI have different formats for tool definitions and calls.
//! This module provides conversion utilities to make tools work with both APIs.

use serde_json::{json, Value};

/// Convert Claude tool definitions to OpenAI function format
///
/// Claude format:
/// ```json
/// {
///     "name": "search_hybrid",
///     "description": "Search files",
///     "input_schema": { "type": "object", "properties": {...} }
/// }
/// ```
///
/// OpenAI format:
/// ```json
/// {
///     "type": "function",
///     "function": {
///         "name": "search_hybrid",
///         "description": "Search files",
///         "parameters": { "type": "object", "properties": {...} }
///     }
/// }
/// ```
pub fn tools_to_openai_format(claude_tools: &[Value]) -> Vec<Value> {
    claude_tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.get("name").cloned().unwrap_or(json!("")),
                    "description": tool.get("description").cloned().unwrap_or(json!("")),
                    "parameters": tool.get("input_schema").cloned().unwrap_or(json!({"type": "object", "properties": {}})),
                }
            })
        })
        .collect()
}

/// Parse OpenAI tool call to internal format for execution
///
/// OpenAI format:
/// ```json
/// {
///     "id": "call_abc123",
///     "type": "function",
///     "function": {
///         "name": "search_hybrid",
///         "arguments": "{\"query\": \"invoices\"}"
///     }
/// }
/// ```
///
/// Returns (tool_call_id, tool_name, parsed_arguments)
pub fn parse_openai_tool_call(tool_call: &Value) -> (String, String, Value) {
    let id = tool_call
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let empty_obj = json!({});
    let function = tool_call.get("function").unwrap_or(&empty_obj);

    let name = function
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let arguments_str = function
        .get("arguments")
        .and_then(|v| v.as_str())
        .unwrap_or("{}");

    let arguments: Value = serde_json::from_str(arguments_str).unwrap_or(json!({}));

    (id, name, arguments)
}

/// Format tool result for OpenAI API
///
/// OpenAI expects tool results as messages with role "tool":
/// ```json
/// {
///     "role": "tool",
///     "tool_call_id": "call_abc123",
///     "content": "Found 5 files: ..."
/// }
/// ```
pub fn tool_result_to_openai_message(tool_call_id: &str, result: &str, is_error: bool) -> Value {
    json!({
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": if is_error {
            format!("Error: {}", result)
        } else {
            result.to_string()
        }
    })
}

/// Build assistant message with tool calls for OpenAI format
///
/// When the assistant wants to call tools, OpenAI expects:
/// ```json
/// {
///     "role": "assistant",
///     "tool_calls": [
///         { "id": "call_abc", "type": "function", "function": { "name": "...", "arguments": "..." } }
///     ]
/// }
/// ```
#[allow(dead_code)]
pub fn build_assistant_tool_call_message(tool_calls: &[Value]) -> Value {
    json!({
        "role": "assistant",
        "tool_calls": tool_calls
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tools_to_openai_format() {
        let claude_tools = vec![json!({
            "name": "search_hybrid",
            "description": "Search files",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }
        })];

        let openai_tools = tools_to_openai_format(&claude_tools);

        assert_eq!(openai_tools.len(), 1);
        assert_eq!(openai_tools[0]["type"], "function");
        assert_eq!(openai_tools[0]["function"]["name"], "search_hybrid");
        assert_eq!(openai_tools[0]["function"]["description"], "Search files");
        assert_eq!(
            openai_tools[0]["function"]["parameters"]["type"],
            "object"
        );
    }

    #[test]
    fn test_parse_openai_tool_call() {
        let tool_call = json!({
            "id": "call_abc123",
            "type": "function",
            "function": {
                "name": "search_hybrid",
                "arguments": "{\"query\": \"invoices\"}"
            }
        });

        let (id, name, args) = parse_openai_tool_call(&tool_call);

        assert_eq!(id, "call_abc123");
        assert_eq!(name, "search_hybrid");
        assert_eq!(args["query"], "invoices");
    }

    #[test]
    fn test_tool_result_to_openai_message() {
        let result = tool_result_to_openai_message("call_abc123", "Found 5 files", false);

        assert_eq!(result["role"], "tool");
        assert_eq!(result["tool_call_id"], "call_abc123");
        assert_eq!(result["content"], "Found 5 files");
    }

    #[test]
    fn test_tool_result_error() {
        let result = tool_result_to_openai_message("call_abc123", "File not found", true);

        assert_eq!(result["content"], "Error: File not found");
    }
}
