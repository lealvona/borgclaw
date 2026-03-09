//! Tools module - defines tools agents can use

use crate::memory::{new_entry, Memory, MemoryQuery, SqliteMemory};
use crate::scheduler::{new_job, JobTrigger, Scheduler, SchedulerTrait};
use crate::security::{CommandCheck, SecurityLayer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

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
            output: error.into(),
            error_type: Some("ExecutionError".to_string()),
            metadata: HashMap::new(),
        }
    }
    
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

#[derive(Clone)]
pub struct ToolRuntime {
    pub workspace_root: PathBuf,
    pub memory: Arc<SqliteMemory>,
    pub scheduler: Arc<Mutex<Scheduler>>,
    pub security: Arc<SecurityLayer>,
}

impl ToolRuntime {
    pub async fn from_config(
        agent: &crate::config::AgentConfig,
        memory_config: &crate::config::MemoryConfig,
        security_config: &crate::config::SecurityConfig,
    ) -> Result<Self, String> {
        let workspace_root = canonical_or_current(&agent.workspace);
        let memory = Arc::new(SqliteMemory::new(memory_config.memory_path.clone()));
        memory.init().await.map_err(|e| e.to_string())?;

        Ok(Self {
            workspace_root,
            memory,
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            security: Arc::new(SecurityLayer::with_config(security_config.clone())),
        })
    }
}

fn canonical_or_current(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

pub fn parse_tool_command(input: &str, tools: &[Tool]) -> Option<ToolCall> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let body = &trimmed[1..];
    let (tool_name, args_str) = match body.split_once(' ') {
        Some((name, rest)) => (name, rest.trim()),
        None => (body, ""),
    };

    if !tools.iter().any(|tool| tool.name == tool_name) {
        return None;
    }

    let arguments = if args_str.is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str::<HashMap<String, serde_json::Value>>(args_str).ok()?
    };

    Some(ToolCall::new(tool_name, arguments))
}

pub async fn execute_tool(call: &ToolCall, runtime: &ToolRuntime) -> ToolResult {
    match call.name.as_str() {
        "memory_store" => memory_store(&call.arguments, runtime).await,
        "memory_recall" => memory_recall(&call.arguments, runtime).await,
        "execute_command" => execute_command(&call.arguments, runtime).await,
        "read_file" => read_file(&call.arguments, runtime).await,
        "list_directory" => list_directory(&call.arguments, runtime).await,
        "fetch_url" => fetch_url(&call.arguments).await,
        "message" => message(&call.arguments),
        "schedule_task" => schedule_task(&call.arguments, runtime).await,
        "approve" => approve_tool(&call.arguments, runtime).await,
        "web_search" => ToolResult::err("web_search is not implemented yet"),
        other => ToolResult::err(format!("unknown tool: {}", other)),
    }
}

async fn memory_store(arguments: &HashMap<String, serde_json::Value>, runtime: &ToolRuntime) -> ToolResult {
    let key = match get_required_string(arguments, "key") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let value = match get_required_string(arguments, "value") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let entry = new_entry(key, value);
    match runtime.memory.store(entry).await {
        Ok(()) => ToolResult::ok("stored"),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn memory_recall(arguments: &HashMap<String, serde_json::Value>, runtime: &ToolRuntime) -> ToolResult {
    let query = match get_required_string(arguments, "query") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let limit = get_u64(arguments, "limit").unwrap_or(5) as usize;

    match runtime
        .memory
        .recall(&MemoryQuery {
            query,
            limit,
            min_score: 0.0,
            group_id: None,
        })
        .await
    {
        Ok(results) if results.is_empty() => ToolResult::ok("no matching memories"),
        Ok(results) => ToolResult::ok(
            results
                .into_iter()
                .map(|result| format!("{}: {}", result.entry.key, result.entry.content))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn execute_command(arguments: &HashMap<String, serde_json::Value>, runtime: &ToolRuntime) -> ToolResult {
    let command = match get_required_string(arguments, "command") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let timeout_secs = get_u64(arguments, "timeout").unwrap_or(60);
    let approval_token = arguments.get("approval_token").and_then(|value| value.as_str());

    if runtime
        .security
        .needs_approval("execute_command", runtime.security.approval_mode())
    {
        if let Some(token) = approval_token {
            if let Err(err) = runtime.security.consume_approval("execute_command", token).await {
                return ToolResult::err(err.to_string());
            }
        } else {
            let token = runtime.security.request_approval("execute_command").await;
            return ToolResult::ok(format!(
                "approval required; rerun with /approve {{\"tool\":\"execute_command\",\"token\":\"{}\"}}",
                token
            ))
            .with_metadata("approval_required", "true")
            .with_metadata("approval_tool", "execute_command")
            .with_metadata("approval_token", token);
        }
    }

    match runtime.security.check_command(&command) {
        CommandCheck::Blocked(pattern) => {
            return ToolResult::err(format!("blocked command by policy: {}", pattern));
        }
        CommandCheck::Allowed => {}
    }

    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-lc")
        .arg(&command)
        .current_dir(&runtime.workspace_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        cmd.output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => return ToolResult::err(err.to_string()),
        Err(_) => return ToolResult::err("command timed out"),
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let combined = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    if output.status.success() {
        ToolResult::ok(truncate_output(&combined))
    } else {
        ToolResult::err(truncate_output(&combined))
    }
}

async fn read_file(arguments: &HashMap<String, serde_json::Value>, runtime: &ToolRuntime) -> ToolResult {
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let offset = get_u64(arguments, "offset").unwrap_or(0) as usize;
    let limit = get_u64(arguments, "limit").unwrap_or(100) as usize;

    let resolved = match resolve_workspace_path(&runtime.workspace_root, &path) {
        Ok(path) => path,
        Err(err) => return ToolResult::err(err),
    };

    let content = match std::fs::read_to_string(&resolved) {
        Ok(content) => content,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let lines = content
        .lines()
        .skip(offset)
        .take(limit)
        .enumerate()
        .map(|(idx, line)| format!("{:>4}: {}", offset + idx + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    ToolResult::ok(lines)
}

async fn list_directory(arguments: &HashMap<String, serde_json::Value>, runtime: &ToolRuntime) -> ToolResult {
    let path = arguments
        .get("path")
        .and_then(|value| value.as_str())
        .unwrap_or(".");

    let resolved = match resolve_workspace_path(&runtime.workspace_root, path) {
        Ok(path) => path,
        Err(err) => return ToolResult::err(err),
    };

    let entries = match std::fs::read_dir(&resolved) {
        Ok(entries) => entries,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let mut names = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let file_type = entry.file_type().ok();
            let suffix = if file_type.as_ref().is_some_and(|ft| ft.is_dir()) {
                "/"
            } else {
                ""
            };
            format!("{}{}", entry.file_name().to_string_lossy(), suffix)
        })
        .collect::<Vec<_>>();
    names.sort();

    ToolResult::ok(names.join("\n"))
}

async fn fetch_url(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    let url = match get_required_string(arguments, "url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match reqwest::get(&url).await {
        Ok(response) if response.status().is_success() => match response.text().await {
            Ok(body) => ToolResult::ok(truncate_output(&body)),
            Err(err) => ToolResult::err(err.to_string()),
        },
        Ok(response) => ToolResult::err(format!("http {}", response.status())),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

fn message(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    match get_required_string(arguments, "text") {
        Ok(text) => ToolResult::ok(text),
        Err(err) => ToolResult::err(err),
    }
}

async fn schedule_task(arguments: &HashMap<String, serde_json::Value>, runtime: &ToolRuntime) -> ToolResult {
    let message = match get_required_string(arguments, "message") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let trigger = match arguments.get("cron").and_then(|value| value.as_str()) {
        Some(cron) => JobTrigger::Cron(cron.to_string()),
        None => JobTrigger::OneShot(chrono::Utc::now()),
    };

    let job = new_job(message.clone(), trigger, message);
    let id = {
        let scheduler = runtime.scheduler.lock().await;
        match scheduler.schedule(job).await {
            Ok(id) => id,
            Err(err) => return ToolResult::err(err.to_string()),
        }
    };

    ToolResult::ok(format!("scheduled {}", id))
}

async fn approve_tool(arguments: &HashMap<String, serde_json::Value>, runtime: &ToolRuntime) -> ToolResult {
    let tool = match get_required_string(arguments, "tool") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let token = match get_required_string(arguments, "token") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match runtime.security.approve_pending(&tool, &token).await {
        Ok(()) => ToolResult::ok(format!(
            "approval recorded for {}; rerun the original command with \"approval_token\":\"{}\"",
            tool, token
        )),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

fn get_required_string(arguments: &HashMap<String, serde_json::Value>, key: &str) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing string argument '{}'", key))
}

fn get_u64(arguments: &HashMap<String, serde_json::Value>, key: &str) -> Option<u64> {
    arguments.get(key).and_then(|value| value.as_u64())
}

fn resolve_workspace_path(workspace_root: &Path, requested: &str) -> Result<PathBuf, String> {
    let candidate = if Path::new(requested).is_absolute() {
        PathBuf::from(requested)
    } else {
        workspace_root.join(requested)
    };

    let normalized = std::fs::canonicalize(&candidate).or_else(|_| {
        candidate
            .parent()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "path has no parent"))
            .and_then(|parent| std::fs::canonicalize(parent).map(|canon| canon.join(candidate.file_name().unwrap_or_default())))
    }).map_err(|e| e.to_string())?;

    if normalized.starts_with(workspace_root) {
        Ok(normalized)
    } else {
        Err("path escapes workspace root".to_string())
    }
}

fn truncate_output(output: &str) -> String {
    const MAX_LEN: usize = 4000;
    if output.len() <= MAX_LEN {
        output.to_string()
    } else {
        format!("{}...", &output[..MAX_LEN])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentConfig, ApprovalMode, MemoryConfig, SecurityConfig};

    #[test]
    fn parses_json_tool_command() {
        let tools = builtin_tools();
        let call = parse_tool_command(r#"/message {"text":"hello"}"#, &tools).unwrap();
        assert_eq!(call.name, "message");
        assert_eq!(call.arguments["text"], "hello");
    }

    #[test]
    fn rejects_unknown_tool_command() {
        let tools = builtin_tools();
        assert!(parse_tool_command("/unknown {}", &tools).is_none());
    }

    #[test]
    fn blocks_workspace_escape() {
        let workspace = std::env::temp_dir().join(format!("borgclaw_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&workspace).unwrap();
        let escaped = resolve_workspace_path(&workspace, "../outside.txt");
        std::fs::remove_dir_all(&workspace).unwrap();
        assert!(escaped.is_err());
    }

    #[tokio::test]
    async fn supervised_command_requires_then_uses_approval() {
        let root = std::env::temp_dir().join(format!("borgclaw_runtime_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                memory_path: root.join("memory"),
                ..Default::default()
            },
            &SecurityConfig {
                approval_mode: ApprovalMode::Supervised,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let first = execute_tool(
            &ToolCall::new(
                "execute_command",
                HashMap::from([("command".to_string(), serde_json::json!("printf ok"))]),
            ),
            &runtime,
        )
        .await;

        assert_eq!(first.metadata.get("approval_required").map(String::as_str), Some("true"));
        let token = first.metadata.get("approval_token").cloned().unwrap();

        let approval = execute_tool(
            &ToolCall::new(
                "approve",
                HashMap::from([
                    ("tool".to_string(), serde_json::json!("execute_command")),
                    ("token".to_string(), serde_json::json!(token.clone())),
                ]),
            ),
            &runtime,
        )
        .await;
        assert!(approval.success);

        let second = execute_tool(
            &ToolCall::new(
                "execute_command",
                HashMap::from([
                    ("command".to_string(), serde_json::json!("printf ok")),
                    ("approval_token".to_string(), serde_json::json!(token)),
                ]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(second.success);
        assert_eq!(second.output, "ok");
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
                ("approval_token".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Approval token for protected commands".to_string()),
                    default: None,
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

        Tool::new(
            "approve",
            "Approve a pending protected tool operation",
        )
        .with_schema(ToolSchema::object(
            [
                ("tool".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Tool name being approved".to_string()),
                    default: None,
                    enum_values: None,
                }),
                ("token".to_string(), PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Approval token".to_string()),
                    default: None,
                    enum_values: None,
                }),
            ].into(),
            vec!["tool".to_string(), "token".to_string()],
        ))
        .with_tags(vec!["security".to_string()]),
    ]
}
