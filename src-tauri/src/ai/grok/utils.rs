//! Shared utilities for Grok module
//!
//! Common functions used across the multi-model pipeline.

/// Extract JSON object from a response that might contain markdown or other text
///
/// Handles:
/// - ```json code blocks
/// - Plain ``` code blocks
/// - Raw JSON objects
pub fn extract_json_object(text: &str) -> Result<String, String> {
    // Try to find JSON in ```json blocks
    if let Some(start) = text.find("```json") {
        let json_start = start + 7;
        if let Some(end) = text[json_start..].find("```") {
            return Ok(text[json_start..json_start + end].trim().to_string());
        }
    }

    // Try plain code blocks
    if let Some(start) = text.find("```") {
        let block_start = start + 3;
        let content_start = text[block_start..]
            .find('\n')
            .map(|i| block_start + i + 1)
            .unwrap_or(block_start);
        if let Some(end) = text[content_start..].find("```") {
            return Ok(text[content_start..content_start + end].trim().to_string());
        }
    }

    // Try to find raw JSON object
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return Ok(text[start..=end].to_string());
        }
    }

    Err("No JSON object found in response".to_string())
}

/// Extract JSON array from a response that might contain markdown or other text
///
/// Handles:
/// - ```json code blocks
/// - Plain ``` code blocks
/// - Raw JSON arrays
pub fn extract_json_array(text: &str) -> Result<String, String> {
    // Try to find JSON in ```json blocks
    if let Some(start) = text.find("```json") {
        let json_start = start + 7;
        if let Some(end) = text[json_start..].find("```") {
            let content = text[json_start..json_start + end].trim();
            if content.starts_with('[') {
                return Ok(content.to_string());
            }
        }
    }

    // Try plain code blocks
    if let Some(start) = text.find("```") {
        let block_start = start + 3;
        let content_start = text[block_start..]
            .find('\n')
            .map(|i| block_start + i + 1)
            .unwrap_or(block_start);
        if let Some(end) = text[content_start..].find("```") {
            let content = text[content_start..content_start + end].trim();
            if content.starts_with('[') {
                return Ok(content.to_string());
            }
        }
    }

    // Try to find raw JSON array
    if let Some(start) = text.find('[') {
        if let Some(end) = text.rfind(']') {
            return Ok(text[start..=end].to_string());
        }
    }

    Err("No JSON array found in response".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_object_from_code_block() {
        let text = r#"Here's the result:
```json
{"key": "value", "number": 42}
```
That's it."#;
        let result = extract_json_object(text).unwrap();
        assert!(result.contains("\"key\""));
        assert!(result.contains("\"value\""));
    }

    #[test]
    fn test_extract_json_object_raw() {
        let text = r#"Result: {"name": "test"} done"#;
        let result = extract_json_object(text).unwrap();
        assert_eq!(result, r#"{"name": "test"}"#);
    }

    #[test]
    fn test_extract_json_array_from_code_block() {
        let text = r#"Results:
```json
[{"id": 1}, {"id": 2}]
```
End."#;
        let result = extract_json_array(text).unwrap();
        assert!(result.starts_with('['));
        assert!(result.ends_with(']'));
    }

    #[test]
    fn test_extract_json_array_raw() {
        let text = r#"Data: [1, 2, 3] done"#;
        let result = extract_json_array(text).unwrap();
        assert_eq!(result, "[1, 2, 3]");
    }

    #[test]
    fn test_no_json_returns_error() {
        let text = "No JSON here!";
        assert!(extract_json_object(text).is_err());
        assert!(extract_json_array(text).is_err());
    }
}
