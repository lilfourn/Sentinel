//! Chat module tests
//!
//! Tests for chat agent functionality including:
//! - Model routing (Claude vs OpenAI)
//! - Tool conversion
//! - Message formatting

#[cfg(test)]
mod tests {
    use crate::ai::chat::tool_conversion::*;
    use crate::ai::chat::tools::*;
    use serde_json::json;

    #[test]
    fn test_tools_to_openai_format() {
        let claude_tools = get_chat_tools();
        let openai_tools = tools_to_openai_format(&claude_tools);

        assert!(!openai_tools.is_empty(), "Should have converted tools");

        // Check that each tool has the expected OpenAI format
        for tool in &openai_tools {
            assert!(tool.get("type").is_some(), "Tool should have type");
            assert_eq!(
                tool.get("type").unwrap().as_str().unwrap(),
                "function",
                "Tool type should be 'function'"
            );
            assert!(tool.get("function").is_some(), "Tool should have function");
        }
    }

    #[test]
    fn test_search_hybrid_tool_exists() {
        let tools = get_chat_tools();
        let search_tool = tools
            .iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("search_hybrid"));

        assert!(search_tool.is_some(), "search_hybrid tool should exist");
        let tool = search_tool.unwrap();
        let description = tool.get("description").and_then(|d| d.as_str()).unwrap_or("");
        assert!(!description.is_empty(), "Tool should have description");
    }

    #[test]
    fn test_read_file_tool_exists() {
        let tools = get_chat_tools();
        let read_tool = tools
            .iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("read_file"));

        assert!(read_tool.is_some(), "read_file tool should exist");
    }

    #[test]
    fn test_list_directory_tool_exists() {
        let tools = get_chat_tools();
        let list_tool = tools
            .iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("list_directory"));

        assert!(list_tool.is_some(), "list_directory tool should exist");
    }

    #[test]
    fn test_shell_tool_exists() {
        let tools = get_chat_tools();
        let shell_tool = tools
            .iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("shell"));

        assert!(shell_tool.is_some(), "shell tool should exist");
    }

    #[test]
    fn test_grep_tool_exists() {
        let tools = get_chat_tools();
        let grep_tool = tools
            .iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("grep"));

        assert!(grep_tool.is_some(), "grep tool should exist");
    }

    #[test]
    fn test_openai_tool_conversion_preserves_params() {
        let claude_tools = get_chat_tools();
        let openai_tools = tools_to_openai_format(&claude_tools);

        // Find search_hybrid in OpenAI format
        let openai_search = openai_tools
            .iter()
            .find(|t| {
                t.get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    == Some("search_hybrid")
            })
            .expect("search_hybrid should exist in OpenAI format");

        // Check parameters are preserved
        let openai_params = openai_search
            .get("function")
            .and_then(|f| f.get("parameters"))
            .and_then(|p| p.get("properties"));

        assert!(openai_params.is_some(), "Should have parameters");
    }

    #[test]
    fn test_tool_result_to_openai_message() {
        let result = tool_result_to_openai_message("test-id", "test result", false);

        assert_eq!(result.get("role").unwrap().as_str().unwrap(), "tool");
        assert_eq!(
            result.get("tool_call_id").unwrap().as_str().unwrap(),
            "test-id"
        );
        assert!(result.get("content").is_some());
    }

    #[test]
    fn test_tool_result_error_format() {
        let result = tool_result_to_openai_message("error-id", "error message", true);

        assert_eq!(result.get("role").unwrap().as_str().unwrap(), "tool");
        let content = result.get("content").unwrap().as_str().unwrap();
        assert!(
            content.contains("error") || content.contains("Error"),
            "Error result should indicate error"
        );
    }

    #[test]
    fn test_all_required_tools_present() {
        let tools = get_chat_tools();
        let required_tools = ["search_hybrid", "read_file", "list_directory", "shell", "grep"];

        for name in required_tools {
            let found = tools
                .iter()
                .any(|t| t.get("name").and_then(|n| n.as_str()) == Some(name));
            assert!(found, "Required tool '{}' should exist", name);
        }
    }
}
