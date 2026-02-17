//! Tools module - defines tools agents can use

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Tool name
    pub name: String,
    /// Description
    pub description: String,
    /// Input schema
    pub input_schema: ToolSchema,
    /// Whether tool requires approval
    pub requires_approval: bool,
    /// Categories/tags
    pub tags: Vec<String>,
}

impl Tool {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema: ToolSchema::default(),
            requires_approval: false,
            tags: Vec::new(),
        }
    }
    
    pub fn with_schema(mut self, schema: ToolSchema) -> Self {
        self.input_schema = schema;
        self
    }
    
    pub fn with_approval(mut self, requires: bool) -> Self {
        self.requires_approval = requires;
        self
    }
    
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// JSON Schema for tool input
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolSchema {
    /// type
    #[serde(rename = " Schematype")]
    pub schema_type: String,
    /// Required properties
    pub required: Vec<String>,
    /// Properties
    pub properties: HashMap<String, PropertySchema>,
    /// Description
    pub description: Option<String>,
}

/// Property schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySchema {
    /// Property type
    #[serde(rename = "type")]
    pub prop_type: String,
    /// Description
    pub description: Option<String>,
    /// Default value
    pub default: Option<serde_json::Value>,
    /// Enum values
    pub enum_values: Option<Vec<serde_json::Value>>,
}

impl ToolSchema {
    pub fn object(properties: HashMap<String, PropertySchema>, required: Vec<String>) -> Self {
        Self {
            schema_type: "object".to_string(),
            required,
            properties,
            description: None,
        }
    }
    
    pub fn string() -> Self {
        Self {
            schema_type: "string".to_string(),
            required: Vec::new(),
            properties: HashMap::new(),
            description: None,
        }
    }
}

/// Tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Call ID
    pub id: String,
    /// Tool name
    pub name: String,
    /// Arguments
    pub arguments: HashMap<String, serde_json::Value>,
    /// Result
    pub result: Option<ToolResult>,
    /// Error message
    pub error: Option<String>,
}

impl ToolCall {
    pub fn new(name: impl Into<String>, arguments: HashMap<String, serde_json::Value>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            arguments,
            result: None,
            error: None,
        }
    }
    
    pub fn with_result(mut self, result: ToolResult) -> Self {
        self.result = Some(result);
        self
    }
    
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Success
    pub success: bool,
    /// Output
    pub output: String,
    /// Error type
    pub error_type: Option<String>,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error_type: None,
            metadata: HashMap::new(),
        }
    }
    
    pub fn err(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: String::new(),
            error_type: Some("ExecutionError".to_string()),
            metadata: HashMap::new(),
        }
    }
    
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Built-in tools
pub fn builtin_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "memory_store",
            "Store information in long-term memory",
        )
        .with_schema(ToolSchema::object(
            [
                ("key".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Memory key".to_string()),
                    default: None,
                    enum_values: None,
                }),
                ("value".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Information to store".to_string()),
                    default: None,
                    enum_values: None,
                }),
            ].into(),
            vec!["key".to_string(), "value".to_string()],
        ))
        .with_tags(vec!["memory".to_string()]),
        
        Tool::new(
            "memory_recall",
            "Recall information from long-term memory",
        )
        .with_schema(ToolSchema::object(
            [
                ("query".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Search query".to_string()),
                    default: None,
                    enum_values: None,
                }),
                ("limit".to_string(), PropertySchema {
                    prop_type: "number".to_string(),
                    description: Some("Max results".to_string()),
                    default: Some(serde_json::json!(5)),
                    enum_values: None,
                }),
            ].into(),
            vec!["query".to_string()],
        ))
        .with_tags(vec!["memory".to_string()]),
        
        Tool::new(
            "execute_command",
            "Execute a shell command",
        )
        .with_schema(ToolSchema::object(
            [
                ("command".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Command to execute".to_string()),
                    default: None,
                    enum_values: None,
                }),
                ("timeout".to_string(), PropertySchema {
                    prop_type: "number".to_string(),
                    description: Some("Timeout in seconds".to_string()),
                    default: Some(serde_json::json!(60)),
                    enum_values: None,
                }),
            ].into(),
            vec!["command".to_string()],
        ))
        .with_approval(true)
        .with_tags(vec!["system".to_string()]),
        
        Tool::new(
            "read_file",
            "Read a file from the filesystem",
        )
        .with_schema(ToolSchema::object(
            [
                ("path".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("File path".to_string()),
                    default: None,
                    enum_values: None,
                }),
                ("offset".to_string(), PropertySchema {
                    prop_type: "number".to_string(),
                    description: Some("Line offset".to_string()),
                    default: Some(serde_json::json!(0)),
                    enum_values: None,
                }),
                ("limit".to_string(), PropertySchema {
                    prop_type: "number".to_string(),
                    description: Some("Number of lines".to_string()),
                    default: Some(serde_json::json!(100)),
                    enum_values: None,
                }),
            ].into(),
            vec!["path".to_string()],
        ))
        .with_tags(vec!["filesystem".to_string()]),
        
        Tool::new(
            "list_directory",
            "List files in a directory",
        )
        .with_schema(ToolSchema::object(
            [
                ("path".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Directory path".to_string()),
                    default: None,
                    enum_values: None,
                }),
            ].into(),
            vec!["path".to_string()],
        ))
        .with_tags(vec!["filesystem".to_string()]),
        
        Tool::new(
            "web_search",
            "Search the web",
        )
        .with_schema(ToolSchema::object(
            [
                ("query".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Search query".to_string()),
                    default: None,
                    enum_values: None,
                }),
                ("num_results".to_string(), PropertySchema {
                    prop_type: "number".to_string(),
                    description: Some("Number of results".to_string()),
                    default: Some(serde_json::json!(5)),
                    enum_values: None,
                }),
            ].into(),
            vec!["query".to_string()],
        ))
        .with_tags(vec!["web".to_string()]),
        
        Tool::new(
            "fetch_url",
            "Fetch content from a URL",
        )
        .with_schema(ToolSchema::object(
            [
                ("url".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("URL to fetch".to_string()),
                    default: None,
                    enum_values: None,
                }),
            ].into(),
            vec!["url".to_string()],
        ))
        .with_tags(vec!["web".to_string()]),
        
        Tool::new(
            "message",
            "Send a message to the user",
        )
        .with_schema(ToolSchema::object(
            [
                ("text".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Message text".to_string()),
                    default: None,
                    enum_values: None,
                }),
            ].into(),
            vec!["text".to_string()],
        ))
        .with_tags(vec!["communication".to_string()]),
        
        Tool::new(
            "schedule_task",
            "Schedule a task to run later",
        )
        .with_schema(ToolSchema::object(
            [
                ("message".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Task description".to_string()),
                    default: None,
                    enum_values: None,
                }),
                ("cron".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Cron expression".to_string()),
                    default: None,
                    enum_values: None,
                }),
            ].into(),
            vec!["message".to_string()],
        ))
        .with_tags(vec!["scheduling".to_string()]),
    ]
}
