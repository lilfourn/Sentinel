//! Orchestrator Agent
//!
//! Takes summaries from all explore agents and creates the organization plan.
//! Leverages Grok's 2M token context window to hold all file summaries at once.
//!
//! ## Input Format (from Explore Agents)
//! ```text
//! filename | content_summary | document_type | suggested_name
//! ```
//!
//! ## Output
//! - Folder structure with semantic descriptions
//! - File assignments (file → folder mapping)
//! - Suggested renames

use super::client::GrokClient;
use super::types::*;
use super::utils::extract_json_object;
use serde::Deserialize;
use std::sync::Arc;

/// Orchestrator agent that plans the organization
#[allow(dead_code)]
pub struct OrchestratorAgent {
    client: Arc<GrokClient>,
    config: OrchestratorConfig,
}

/// Configuration for the orchestrator
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct OrchestratorConfig {
    /// Maximum folders to create
    pub max_folders: usize,
    /// Maximum nesting depth
    pub max_depth: usize,
    /// Whether to suggest renames
    pub suggest_renames: bool,
    /// User's organization instruction
    pub user_instruction: String,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_folders: 200,  // Allow many specific folders - one per entity/project
            max_depth: 6,      // Deep nesting for proper hierarchy
            suggest_renames: true,
            user_instruction: "Organize these files intelligently".to_string(),
        }
    }
}

impl OrchestratorAgent {
    /// Create a new orchestrator
    pub fn new(client: Arc<GrokClient>, config: OrchestratorConfig) -> Self {
        Self { client, config }
    }

    /// Create organization plan from explore results
    pub async fn create_plan(
        &self,
        explore_results: Vec<ExploreResult>,
    ) -> Result<OrganizationPlan, String> {
        // Aggregate all analyses
        let all_analyses: Vec<&DocumentAnalysis> = explore_results
            .iter()
            .flat_map(|r| r.analyses.iter())
            .collect();

        if all_analyses.is_empty() {
            return Err("No files analyzed".to_string());
        }

        tracing::info!(
            "[Orchestrator] Creating plan for {} files",
            all_analyses.len()
        );

        // Build the mega-prompt with all summaries
        let summaries = self.build_summary_context(&all_analyses);

        // Call Grok with the full context
        let plan = self.call_grok_for_plan(&summaries).await?;

        tracing::info!(
            "[Orchestrator] Plan created: {} folders, {} assignments",
            plan.folder_structure.len(),
            plan.assignments.len()
        );

        Ok(plan)
    }

    /// Build the context string with all file summaries
    /// CRITICAL: Include full content summaries and entities for proper folder naming
    fn build_summary_context(&self, analyses: &[&DocumentAnalysis]) -> String {
        let mut context = String::new();

        // Group by document type for better organization
        let mut by_type: std::collections::HashMap<&str, Vec<&DocumentAnalysis>> =
            std::collections::HashMap::new();

        for analysis in analyses {
            by_type
                .entry(analysis.document_type.as_str())
                .or_default()
                .push(analysis);
        }

        // Format summaries grouped by type - INCLUDE FULL DETAILS for domain-specific naming
        for (doc_type, files) in by_type {
            context.push_str(&format!("\n## {} ({} files)\n", doc_type, files.len()));

            for analysis in files {
                // Start with filename
                context.push_str(&format!("### {}\n", analysis.file_name));

                // KEY ENTITIES FIRST - These drive folder naming decisions!
                if !analysis.key_entities.is_empty() {
                    context.push_str(&format!(
                        "**Key Entities**: {}\n",
                        analysis.key_entities.join(", ")
                    ));
                }

                // FULL content summary - don't truncate! This is essential for understanding
                context.push_str(&format!(
                    "**Summary**: {}\n",
                    analysis.content_summary
                ));

                // Suggested name if different from original
                if let Some(ref suggested) = analysis.suggested_name {
                    if suggested != &analysis.file_name {
                        context.push_str(&format!("**Suggested Name**: {}\n", suggested));
                    }
                }

                // Confidence indicator for quality filtering
                if analysis.confidence < 0.7 {
                    context.push_str("**Note**: Low confidence analysis\n");
                }

                context.push('\n');
            }
        }

        // Add statistics for context
        let total_files = analyses.len();
        let high_confidence = analyses.iter().filter(|a| a.confidence >= 0.7).count();
        context.push_str(&format!(
            "\n---\nTotal: {} files analyzed ({} high confidence)\n",
            total_files, high_confidence
        ));

        context
    }

    /// Call Grok to create the organization plan
    async fn call_grok_for_plan(&self, summaries: &str) -> Result<OrganizationPlan, String> {
        let prompt = self.build_orchestrator_prompt(summaries);

        tracing::debug!(
            "[Orchestrator] Prompt size: {} chars, ~{} tokens",
            prompt.len(),
            prompt.len() / 4
        );

        // Use the client's base request mechanism
        // This is a text-only request (no images)
        let response = self.send_text_request(&prompt).await?;

        // Parse the response
        self.parse_plan_response(&response)
    }

    /// Build the orchestrator prompt
    fn build_orchestrator_prompt(&self, summaries: &str) -> String {
        format!(
            r#"You are an expert file organization specialist. Create a HIGHLY SPECIFIC folder structure based on the ACTUAL ENTITIES found in these files.

## User Request
{}

## File Analysis Data
{}

## CRITICAL: ENTITY-FIRST ORGANIZATION

### THE GOLDEN RULE: ONE FOLDER PER ENTITY
Every unique company, client, project, or person mentioned in the files MUST get their own folder. Do NOT combine unrelated entities into generic buckets.

### Step 1: Extract ALL Entities from Key Entities Fields
Scan through EVERY file's key_entities and create folders for:
- **Each company**: "Acme-Corp/", "Smith-Law-Firm/", "TechStart-Inc/", "Global-Investments-LLC/"
- **Each client**: "Client-Johnson/", "Client-Martinez/", "Client-Tech-Solutions/"
- **Each project**: "Project-Phoenix/", "Website-Redesign-2024/", "Kitchen-Renovation/"
- **Each person**: "John-Smith/", "Dr-Chen/", "Sarah-Williams/"
- **Each property/address**: "123-Main-St/", "Suite-500-Building/", "Rental-Property-Oak-Ave/"
- **Each time period with context**: "2024-Tax-Filing/", "Q1-2024-Reports/", "FY2023-Audit/"

### Step 2: Create Deep Hierarchies
Use DEEP NESTING to show relationships. More specific = better!

EXCELLENT STRUCTURE (what we want):
```
Clients/
  Acme-Corporation/
    2024/
      Q1-Invoices/
      Q2-Invoices/
      Contracts/
        Master-Service-Agreement/
        NDA/
      Project-Alpha/
        Deliverables/
        Communications/
    2023/
      Annual-Report/
  TechStart-Inc/
    Onboarding/
    Monthly-Reports/
Properties/
  123-Main-Street/
    Lease-Agreements/
    Maintenance-Records/
    Tenant-Communications/
  456-Oak-Avenue/
    Purchase-Documents/
    Renovation-2024/
```

TERRIBLE STRUCTURE (what we're avoiding):
```
Financial-Records/      ← FORBIDDEN: Generic category
Business-Documents/     ← FORBIDDEN: Generic category
Property-Documentation/ ← FORBIDDEN: Generic category
General-PDFs/          ← FORBIDDEN: Catch-all bucket
Email-Attachments/     ← FORBIDDEN: Source-based, not content-based
Spreadsheets/          ← FORBIDDEN: Format-based, not content-based
```

### Step 3: FORBIDDEN Generic Names
These words are BANNED from folder names:
❌ General, Generic, Various, Mixed, Assorted
❌ Documents, Files, Data, Content, Resources, Records
❌ Financial, Legal, Administrative, Technical, Business
❌ Miscellaneous, Other, Unsorted, Uncategorized, Misc
❌ PDFs, Spreadsheets, Images, Attachments (format-based names)
❌ Corporate, Professional, Personal (vague categories)

### Step 4: When Entities Are Unclear
If a file has no clear entity in key_entities:
1. Look at the content_summary for company/person names
2. Look at the suggested_name for clues
3. Group by the MOST SPECIFIC common attribute (date + type is better than just type)
4. Use the actual filename pattern if it contains useful info

Example: If files mention "rent roll" but no property name, create:
- "Rent-Rolls-2024/" NOT "Financial-Records/"

## Output Requirements

Create as many folders as needed. Aim for:
- **3-15 files per leaf folder** (most specific level)
- **Deep nesting** (4-6 levels) for complex hierarchies
- **One folder per unique entity** found in the data

## MANDATORY FILE RENAMING RULES

EVERY file MUST be renamed using this format:
**[Entity]-[Description]-[Date]-[Type].[ext]**

### Rules:
1. **Use HYPHENS (-)** instead of spaces - NEVER use spaces or underscores
2. **Include DATE** when the document has an important date (YYYY-MM-DD or YYYY-MM)
3. **Start with primary entity** (company name, person name, property address)
4. **End with document type** (Invoice, Contract, Report, Statement, etc.)
5. **Include identifying numbers** when present (invoice #, amount, reference)
6. **Keep original extension** (.pdf, .xlsx, .docx, etc.)
7. **Max 60 characters** before extension

### Naming Examples:
| Original | Renamed |
|----------|---------|
| scan001.pdf | Acme-Corp-Invoice-2024-03-15-5432.pdf |
| document.pdf | Smith-John-Employment-Contract-2024-01.pdf |
| IMG_4521.jpg | 123-Main-St-Property-Photo-Exterior.jpg |
| report.xlsx | Q1-2024-Financial-Summary-TechStart.xlsx |
| email attachment.pdf | ABC-Properties-Lease-Agreement-Unit-4B.pdf |
| Copy of invoice (1).pdf | Global-Inc-Invoice-2024-02-28-1250.pdf |
| SCAN0042.PDF | Martinez-Family-Trust-Document-2023.pdf |

### FORBIDDEN filename patterns:
❌ Spaces: "Acme Corp Invoice.pdf"
❌ Generic: "document.pdf", "scan.pdf", "file.pdf"
❌ Camera defaults: "IMG_xxxx", "DSC_xxxx", "SCAN00xx"
❌ Copy indicators: "Copy of", "(1)", "- Copy"
❌ Underscores: "acme_corp_invoice.pdf"
❌ No context: "invoice.pdf", "contract.pdf"

Return ONLY this JSON structure:
{{
  "detected_domain": "Specific description like 'Real estate property management for Smith Properties LLC' or 'Software consulting business with clients Acme, TechStart, and GlobalCorp'",
  "key_entities_found": ["Acme-Corp", "TechStart-Inc", "John-Smith", "123-Main-St", "Project-Phoenix", "Q1-2024"],
  "strategy_name": "Entity-based hierarchical organization",
  "description": "Files organized by [primary entity type], then by [secondary grouping], with [time-based subdivisions] where applicable",
  "folder_structure": [
    {{
      "path": "Clients/Acme-Corporation",
      "description": "All documents related to Acme Corporation client",
      "expected_file_count": 15
    }},
    {{
      "path": "Clients/Acme-Corporation/2024/Invoices",
      "description": "Acme Corp invoices from 2024",
      "expected_file_count": 8
    }},
    {{
      "path": "Clients/Acme-Corporation/2024/Contracts",
      "description": "Acme Corp contracts from 2024",
      "expected_file_count": 3
    }},
    {{
      "path": "Properties/123-Main-Street/Lease-Documents",
      "description": "Lease agreements for 123 Main St property",
      "expected_file_count": 4
    }}
  ],
  "assignments": [
    {{
      "file_path": "original/path/to/file.pdf",
      "original_name": "scan001.pdf",
      "destination_folder": "Clients/Acme-Corporation/2024/Invoices",
      "new_name": "Acme-Corp-Invoice-2024-03-15-5432.pdf",
      "confidence": 0.9
    }}
  ],
  "unassigned_files": []
}}

## Constraints
- Create up to {} folders - USE THEM ALL if entities warrant it
- Nest up to {} levels deep - deeper is better for clarity
- EVERY file MUST be assigned AND renamed - no "unassigned" unless truly unidentifiable
- EVERY new_name MUST follow the naming rules above - NO spaces, NO generic names
- Generic folder or file names will be REJECTED

Output ONLY valid JSON. No markdown, no explanation, no code blocks."#,
            self.config.user_instruction,
            summaries,
            self.config.max_folders,
            self.config.max_depth
        )
    }

    /// Send a text-only request to Grok
    async fn send_text_request(&self, prompt: &str) -> Result<String, String> {
        use reqwest::Client;
        use serde_json::json;

        let client = Client::new();

        let request_body = json!({
            "model": "grok-4-1-fast",
            "messages": [{
                "role": "user",
                "content": prompt
            }],
            "max_tokens": 16000,  // Large output for complex hierarchical structures
            "temperature": 0.3   // Slightly higher for more creative folder naming
        });

        // Get API key from environment (dotenvy loads .env at startup)
        let api_key = std::env::var("XAI_API_KEY")
            .or_else(|_| std::env::var("GROK_API_KEY"))
            .or_else(|_| std::env::var("VITE_XAI_API_KEY"))
            .map_err(|_| "No Grok API key found (XAI_API_KEY, GROK_API_KEY, or VITE_XAI_API_KEY)")?;

        let response = client
            .post("https://api.x.ai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("API error ({}): {}", status, text));
        }

        #[derive(Deserialize)]
        struct Response {
            choices: Vec<Choice>,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: Message,
        }
        #[derive(Deserialize)]
        struct Message {
            content: String,
        }

        let resp: Response = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        resp.choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| "No response content".to_string())
    }

    /// Parse the plan response from Grok
    fn parse_plan_response(&self, response: &str) -> Result<OrganizationPlan, String> {
        // Extract JSON from response
        let json_str = extract_json_object(response)?;

        // Parse into our structure
        #[derive(Deserialize)]
        struct RawPlan {
            #[serde(default)]
            detected_domain: Option<String>,
            #[serde(default)]
            key_entities_found: Vec<String>,
            strategy_name: String,
            description: String,
            folder_structure: Vec<RawFolder>,
            assignments: Vec<RawAssignment>,
            #[serde(default)]
            unassigned_files: Vec<String>,
        }

        #[derive(Deserialize)]
        struct RawFolder {
            path: String,
            description: String,
            #[serde(default)]
            expected_file_count: usize,
        }

        #[derive(Deserialize)]
        struct RawAssignment {
            file_path: String,
            original_name: String,
            destination_folder: String,
            new_name: Option<String>,
            #[serde(default = "default_confidence")]
            confidence: f32,
        }

        fn default_confidence() -> f32 {
            0.8
        }

        let raw: RawPlan = serde_json::from_str(&json_str)
            .map_err(|e| format!("Failed to parse plan JSON: {}. Response: {}", e, response))?;

        // Log detected domain for debugging
        if let Some(ref domain) = raw.detected_domain {
            tracing::info!("[Orchestrator] Detected domain: {}", domain);
        }
        if !raw.key_entities_found.is_empty() {
            tracing::info!(
                "[Orchestrator] Key entities: {}",
                raw.key_entities_found.join(", ")
            );
        }

        Ok(OrganizationPlan {
            detected_domain: raw.detected_domain,
            key_entities_found: raw.key_entities_found,
            strategy_name: raw.strategy_name,
            description: raw.description,
            folder_structure: raw
                .folder_structure
                .into_iter()
                .map(|f| PlannedFolder {
                    path: f.path,
                    description: f.description,
                    expected_file_count: f.expected_file_count,
                })
                .collect(),
            assignments: raw
                .assignments
                .into_iter()
                .map(|a| FolderAssignment {
                    file_path: a.file_path,
                    original_name: a.original_name,
                    destination_folder: a.destination_folder,
                    new_name: a.new_name,
                    confidence: a.confidence,
                })
                .collect(),
            unassigned_files: raw.unassigned_files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json() {
        let text = r#"Here's the plan:
```json
{"strategy_name": "Test", "description": "Test plan", "folder_structure": [], "assignments": []}
```
"#;
        let json = extract_json_object(text).unwrap();
        assert!(json.contains("strategy_name"));
    }

    #[test]
    fn test_extract_raw_json() {
        let text = r#"{"strategy_name": "Test"}"#;
        let json = extract_json_object(text).unwrap();
        assert!(json.contains("strategy_name"));
    }
}
