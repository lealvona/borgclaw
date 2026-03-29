use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool retry policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRetryPolicy {
    /// Maximum number of retry attempts (0 = no retry)
    pub max_retries: u32,
    /// Initial delay between retries in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum delay between retries in milliseconds
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
    /// Add random jitter to delay (0.0-1.0)
    pub jitter_factor: f64,
}

impl Default for ToolRetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 0,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

impl ToolRetryPolicy {
    /// Create a standard retry policy with exponential backoff
    pub fn exponential(max_retries: u32) -> Self {
        Self {
            max_retries,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }

    /// Create a policy with custom initial delay
    pub fn with_initial_delay(mut self, delay_ms: u64) -> Self {
        self.initial_delay_ms = delay_ms;
        self
    }

    /// Create a policy with custom max delay
    pub fn with_max_delay(mut self, delay_ms: u64) -> Self {
        self.max_delay_ms = delay_ms;
        self
    }

    /// Calculate delay for a specific retry attempt
    pub fn calculate_delay(&self, attempt: u32) -> u64 {
        if attempt == 0 {
            return 0;
        }

        let base_delay =
            self.initial_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32 - 1);
        let capped_delay = base_delay.min(self.max_delay_ms as f64);

        // Add jitter
        if self.jitter_factor > 0.0 {
            let jitter_range = capped_delay * self.jitter_factor;
            let jitter = rand::random::<f64>() * jitter_range - (jitter_range / 2.0);
            (capped_delay + jitter) as u64
        } else {
            capped_delay as u64
        }
    }
}

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
    /// Retry policy for this tool
    pub retry_policy: ToolRetryPolicy,
}

impl Tool {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema: ToolSchema::default(),
            requires_approval: false,
            tags: Vec::new(),
            retry_policy: ToolRetryPolicy::default(),
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

    pub fn with_retry_policy(mut self, policy: ToolRetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Execute a function with retry logic according to this tool's policy
    pub async fn execute_with_retry<F, Fut>(&self, operation: F) -> ToolResult
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = ToolResult>,
    {
        execute_with_retry(&self.retry_policy, &self.name, operation).await
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
            output: error.into(),
            error_type: Some("ExecutionError".to_string()),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Check if the result indicates a retryable error
    pub fn is_retryable(&self) -> bool {
        if self.success {
            return false;
        }

        // Check error type or message for transient errors
        if let Some(ref error_type) = self.error_type {
            match error_type.as_str() {
                "NetworkError" | "TimeoutError" | "RateLimitError" | "TransientError" => {
                    return true
                }
                _ => {}
            }
        }

        // Check error message for common retryable patterns
        let retryable_patterns = [
            "timeout",
            "timed out",
            "connection refused",
            "connection reset",
            "temporary",
            "rate limit",
            "too many requests",
            "503",
            "502",
            "504",
            "network error",
        ];

        let output_lower = self.output.to_lowercase();
        retryable_patterns
            .iter()
            .any(|pattern| output_lower.contains(pattern))
    }
}

/// Execute an operation with retry logic according to a retry policy
pub async fn execute_with_retry<F, Fut>(
    policy: &ToolRetryPolicy,
    tool_name: &str,
    mut operation: F,
) -> ToolResult
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ToolResult>,
{
    let mut last_result = operation().await;

    // If success or no retries configured, return immediately
    if last_result.success || policy.max_retries == 0 {
        return last_result;
    }

    // Check if the error is retryable
    if !last_result.is_retryable() {
        return last_result.with_metadata("retry_skipped", "error_not_retryable");
    }

    // Attempt retries
    for attempt in 1..=policy.max_retries {
        let delay_ms = policy.calculate_delay(attempt);

        tracing::info!(
            "Tool '{}' failed, retrying in {}ms (attempt {}/{})",
            tool_name,
            delay_ms,
            attempt,
            policy.max_retries
        );

        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;

        last_result = operation().await;

        if last_result.success {
            return last_result.with_metadata("retry_attempts", attempt.to_string());
        }

        // If error is no longer retryable, stop
        if !last_result.is_retryable() {
            break;
        }
    }

    last_result.with_metadata(
        "retry_exhausted",
        format!("{}/{} attempts", policy.max_retries, policy.max_retries),
    )
}
