//! Tools module - defines tools agents can use

use crate::mcp::client::{McpClient, McpClientConfig};
use crate::mcp::transport::{
    McpTransportConfig, SseTransportConfig, StdioTransportConfig, WebSocketTransportConfig,
};
use crate::memory::{new_entry, Memory, MemoryQuery, SqliteMemory};
use crate::scheduler::{new_job, JobTrigger, Scheduler, SchedulerTrait};
use crate::security::{CommandCheck, SecurityLayer};
use crate::skills::{
    BrowserSkill, CdpClient, GitHubClient, GoogleClient, ImageClient, ImageParams, PluginRegistry,
    PlaywrightClient, QrFormat, QrSkill, SttClient, TtsClient, UrlShortener,
};
use regex::Regex;
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
    pub plugins: Arc<PluginRegistry>,
    pub skills: crate::config::SkillsConfig,
    pub mcp_servers: HashMap<String, crate::config::McpServerConfig>,
    pub security: Arc<SecurityLayer>,
}

impl ToolRuntime {
    pub async fn from_config(
        agent: &crate::config::AgentConfig,
        memory_config: &crate::config::MemoryConfig,
        skills_config: &crate::config::SkillsConfig,
        mcp_config: &crate::config::McpConfig,
        security_config: &crate::config::SecurityConfig,
    ) -> Result<Self, String> {
        let workspace_root = canonical_or_current(&agent.workspace);
        let memory = Arc::new(SqliteMemory::new(memory_config.database_path.clone()));
        memory.init().await.map_err(|e| e.to_string())?;
        let plugins = Arc::new(PluginRegistry::new());
        plugins
            .load_from_dir(&skills_config.skills_path)
            .await
            .map_err(|e| e.to_string())?;

        Ok(Self {
            workspace_root,
            memory,
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            plugins,
            skills: skills_config.clone(),
            mcp_servers: mcp_config.servers.clone(),
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
    let result = match call.name.as_str() {
        "memory_store" => memory_store(&call.arguments, runtime).await,
        "memory_recall" => memory_recall(&call.arguments, runtime).await,
        "execute_command" => execute_command(&call.arguments, runtime).await,
        "read_file" => read_file(&call.arguments, runtime).await,
        "list_directory" => list_directory(&call.arguments, runtime).await,
        "fetch_url" => fetch_url(&call.arguments).await,
        "message" => message(&call.arguments),
        "schedule_task" => schedule_task(&call.arguments, runtime).await,
        "approve" => approve_tool(&call.arguments, runtime).await,
        "web_search" => web_search(&call.arguments).await,
        "plugin_list" => plugin_list(runtime).await,
        "plugin_invoke" => plugin_invoke(&call.arguments, runtime).await,
        "github_list_repos" => github_list_repos(&call.arguments, runtime).await,
        "github_create_pr" => github_create_pr(&call.arguments, runtime).await,
        "github_prepare_delete_branch" => github_prepare_delete_branch(&call.arguments, runtime).await,
        "github_delete_branch" => github_delete_branch(&call.arguments, runtime).await,
        "github_prepare_merge_pr" => github_prepare_merge_pr(&call.arguments, runtime).await,
        "github_merge_pr" => github_merge_pr(&call.arguments, runtime).await,
        "google_list_messages" => google_list_messages(&call.arguments, runtime).await,
        "google_send_email" => google_send_email(&call.arguments, runtime).await,
        "google_search_files" => google_search_files(&call.arguments, runtime).await,
        "google_list_events" => google_list_events(&call.arguments, runtime).await,
        "google_upload_file" => google_upload_file(&call.arguments, runtime).await,
        "google_create_event" => google_create_event(&call.arguments, runtime).await,
        "browser_navigate" => browser_navigate(&call.arguments, runtime).await,
        "browser_click" => browser_click(&call.arguments, runtime).await,
        "browser_fill" => browser_fill(&call.arguments, runtime).await,
        "browser_wait_for" => browser_wait_for(&call.arguments, runtime).await,
        "browser_get_text" => browser_get_text(&call.arguments, runtime).await,
        "browser_get_html" => browser_get_html(&call.arguments, runtime).await,
        "browser_screenshot" => browser_screenshot(&call.arguments, runtime).await,
        "stt_transcribe" => stt_transcribe(&call.arguments, runtime).await,
        "tts_speak" => tts_speak(&call.arguments, runtime).await,
        "image_generate" => image_generate(&call.arguments, runtime).await,
        "qr_encode" => qr_encode(&call.arguments).await,
        "url_shorten" => url_shorten(&call.arguments, runtime).await,
        "url_expand" => url_expand(&call.arguments, runtime).await,
        "mcp_list_tools" => mcp_list_tools(&call.arguments, runtime).await,
        "mcp_call_tool" => mcp_call_tool(&call.arguments, runtime).await,
        other => ToolResult::err(format!("unknown tool: {}", other)),
    };

    sanitize_tool_result(result, runtime)
}

fn sanitize_tool_result(mut result: ToolResult, runtime: &ToolRuntime) -> ToolResult {
    if !runtime.security.configured_for_leak_detection() {
        return result;
    }

    let leaks = runtime.security.check_leak(&result.output);
    if leaks.is_empty() {
        return result;
    }

    result
        .metadata
        .insert("secret_redactions".to_string(), leaks.len().to_string());

    match runtime.security.leak_action() {
        crate::config::LeakAction::Redact => {
            let (redacted, _) = runtime.security.redact_leaks(&result.output);
            result.output = redacted;
            result
                .metadata
                .insert("security_redacted".to_string(), "true".to_string());
        }
        crate::config::LeakAction::Warn => {
            result.metadata.insert(
                "security_warning".to_string(),
                "secret_detected".to_string(),
            );
        }
        crate::config::LeakAction::Block => {
            let mut blocked = ToolResult::err("tool output blocked by secret leak detection");
            blocked.metadata = result.metadata;
            blocked
                .metadata
                .insert("security_blocked".to_string(), "true".to_string());
            return blocked;
        }
    }

    result
}

async fn memory_store(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
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

async fn memory_recall(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
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

async fn execute_command(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let command = match get_required_string(arguments, "command") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let timeout_secs = get_u64(arguments, "timeout").unwrap_or(60);
    let approval_token = arguments
        .get("approval_token")
        .and_then(|value| value.as_str());

    if runtime
        .security
        .needs_approval("execute_command", runtime.security.approval_mode())
    {
        if let Some(token) = approval_token {
            if let Err(err) = runtime
                .security
                .consume_approval("execute_command", token)
                .await
            {
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
    let secret_env = runtime.security.secret_env().await;
    cmd.arg("-lc")
        .arg(&command)
        .current_dir(&runtime.workspace_root)
        .envs(secret_env)
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

async fn read_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
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

async fn list_directory(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
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

async fn schedule_task(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
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

async fn web_search(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    let query = match get_required_string(arguments, "query") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let num_results = get_u64(arguments, "num_results").unwrap_or(5).clamp(1, 10) as usize;

    let client = reqwest::Client::builder()
        .user_agent("BorgClaw/0.1")
        .build();
    let client = match client {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let response = client
        .get("https://duckduckgo.com/html/")
        .query(&[("q", query.as_str())])
        .send()
        .await;
    let response = match response {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => return ToolResult::err(format!("http {}", response.status())),
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let body = match response.text().await {
        Ok(body) => body,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let results = parse_duckduckgo_results(&body, num_results);
    if results.is_empty() {
        return ToolResult::ok("no results");
    }

    let output = results
        .iter()
        .enumerate()
        .map(|(index, result)| {
            let mut line = format!("{}. {} - {}", index + 1, result.title, result.url);
            if let Some(snippet) = &result.snippet {
                line.push_str(&format!("\n   {}", snippet));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n");

    ToolResult::ok(output)
        .with_metadata("source", "duckduckgo")
        .with_metadata("result_count", results.len().to_string())
}

async fn plugin_list(runtime: &ToolRuntime) -> ToolResult {
    let plugins = runtime.plugins.list().await;
    if plugins.is_empty() {
        return ToolResult::ok("no plugins loaded");
    }

    ToolResult::ok(
        plugins
            .into_iter()
            .map(|plugin| {
                format!(
                    "{} {} - {}",
                    plugin.name, plugin.version, plugin.description
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

async fn plugin_invoke(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let plugin = match get_required_string(arguments, "plugin") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let function = arguments
        .get("function")
        .and_then(|value| value.as_str())
        .unwrap_or("invoke")
        .to_string();
    let input = arguments
        .get("input")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let input_json = match serde_json::to_string(&input) {
        Ok(json) => json,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    match runtime
        .plugins
        .invoke(&plugin, &function, &input_json)
        .await
    {
        Ok(output) => ToolResult::ok(output)
            .with_metadata("plugin", plugin)
            .with_metadata("function", function),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_list_repos(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let visibility = arguments.get("visibility").and_then(|value| value.as_str());

    match client.list_repos(visibility).await {
        Ok(repos) if repos.is_empty() => ToolResult::ok("no repositories"),
        Ok(repos) => ToolResult::ok(
            repos.into_iter()
                .map(|repo| format!("{} ({})", repo.full_name, repo.default_branch))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_create_pr(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let title = match get_required_string(arguments, "title") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let head = match get_required_string(arguments, "head") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let base = match get_required_string(arguments, "base") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let body = arguments
        .get("body")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    match client.create_pr(&owner, &repo, &title, body, &head, &base).await {
        Ok(pr) => ToolResult::ok(format!("{} #{}", pr.html_url, pr.number))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_prepare_delete_branch(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let branch = match get_required_string(arguments, "branch") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.prepare_delete_branch(&owner, &repo, &branch).await {
        Ok(confirmation) => ToolResult::ok(confirmation.description)
            .with_metadata("confirmation_token", confirmation.token)
            .with_metadata("expires_at", confirmation.expires_at.to_rfc3339()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_delete_branch(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let branch = match get_required_string(arguments, "branch") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let confirmation_token = arguments
        .get("confirmation_token")
        .and_then(|value| value.as_str());

    match client
        .delete_branch(&owner, &repo, &branch, confirmation_token)
        .await
    {
        Ok(()) => ToolResult::ok(format!("deleted branch {}", branch)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_prepare_merge_pr(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let number = match get_u64(arguments, "number") {
        Some(value) => value as u32,
        None => return ToolResult::err("missing number argument 'number'"),
    };

    match client.prepare_merge_pr(&owner, &repo, number).await {
        Ok(confirmation) => ToolResult::ok(confirmation.description)
            .with_metadata("confirmation_token", confirmation.token)
            .with_metadata("expires_at", confirmation.expires_at.to_rfc3339()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_merge_pr(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let number = match get_u64(arguments, "number") {
        Some(value) => value as u32,
        None => return ToolResult::err("missing number argument 'number'"),
    };
    let confirmation_token = arguments
        .get("confirmation_token")
        .and_then(|value| value.as_str());

    match client.merge_pr(&owner, &repo, number, confirmation_token).await {
        Ok(true) => ToolResult::ok(format!("merged pull request {}", number)),
        Ok(false) => ToolResult::err("merge request was not accepted"),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn google_list_messages(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let query = arguments.get("query").and_then(|value| value.as_str());
    let limit = get_u64(arguments, "limit").unwrap_or(10) as u32;

    match client.list_messages(query, limit).await {
        Ok(messages) if messages.is_empty() => ToolResult::ok("no messages"),
        Ok(messages) => ToolResult::ok(
            messages
                .into_iter()
                .map(|msg| {
                    format!(
                        "{} | {} | {}",
                        msg.id,
                        msg.from.unwrap_or_default(),
                        msg.subject.unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn google_send_email(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let to = match get_required_string(arguments, "to") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let subject = match get_required_string(arguments, "subject") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let body = match get_required_string(arguments, "body") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.send_email(&to, &subject, &body).await {
        Ok(id) => ToolResult::ok(id),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn google_search_files(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let query = match get_required_string(arguments, "query") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.search_files(&query).await {
        Ok(files) if files.is_empty() => ToolResult::ok("no files"),
        Ok(files) => ToolResult::ok(
            files.into_iter()
                .map(|file| format!("{} | {} | {}", file.id, file.name, file.mime_type))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn google_list_events(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let calendar_id = arguments
        .get("calendar_id")
        .and_then(|value| value.as_str())
        .unwrap_or("primary");
    let days = get_u64(arguments, "days").unwrap_or(7) as i64;
    let now = chrono::Utc::now();

    match client
        .list_events(calendar_id, now, now + chrono::Duration::days(days))
        .await
    {
        Ok(events) if events.is_empty() => ToolResult::ok("no events"),
        Ok(events) => ToolResult::ok(
            events.into_iter()
                .map(|event| format!("{} | {}", event.start.to_rfc3339(), event.summary))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn google_upload_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let mime_type = arguments
        .get("mime_type")
        .and_then(|value| value.as_str())
        .unwrap_or("application/octet-stream")
        .to_string();
    let folder_id = arguments.get("folder_id").and_then(|value| value.as_str());
    let resolved = match resolve_workspace_path(&runtime.workspace_root, &path) {
        Ok(path) => path,
        Err(err) => return ToolResult::err(err),
    };
    let bytes = match std::fs::read(&resolved) {
        Ok(bytes) => bytes,
        Err(err) => return ToolResult::err(err.to_string()),
    };
    let name = resolved
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("upload.bin")
        .to_string();

    match client
        .upload_file(&name, bytes, &mime_type, folder_id)
        .await
    {
        Ok(file) => ToolResult::ok(format!("{} | {}", file.id, file.name)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn google_create_event(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let summary = match get_required_string(arguments, "summary") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let start = match get_required_string(arguments, "start") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let end = match get_required_string(arguments, "end") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let start = match chrono::DateTime::parse_from_rfc3339(&start) {
        Ok(value) => value.with_timezone(&chrono::Utc),
        Err(err) => return ToolResult::err(err.to_string()),
    };
    let end = match chrono::DateTime::parse_from_rfc3339(&end) {
        Ok(value) => value.with_timezone(&chrono::Utc),
        Err(err) => return ToolResult::err(err.to_string()),
    };
    let description = arguments
        .get("description")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);

    match client
        .create_event(crate::skills::CalendarEvent {
            summary,
            start,
            end,
            description,
            ..Default::default()
        })
        .await
    {
        Ok(event) => ToolResult::ok(format!("{} | {}", event.id, event.summary)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn browser_navigate(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let url = match get_required_string(arguments, "url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    with_browser(runtime, |browser| async move {
        browser.navigate(&url).await?;
        Ok(ToolResult::ok(url))
    })
    .await
}

async fn browser_click(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let selector = match get_required_string(arguments, "selector") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    with_browser(runtime, |browser| async move {
        browser.click(&selector).await?;
        Ok(ToolResult::ok(format!("clicked {}", selector)))
    })
    .await
}

async fn browser_fill(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let selector = match get_required_string(arguments, "selector") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let value = match get_required_string(arguments, "value") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    with_browser(runtime, |browser| async move {
        browser.fill(&selector, &value).await?;
        Ok(ToolResult::ok(format!("filled {}", selector)))
    })
    .await
}

async fn browser_wait_for(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let timeout_ms = get_u64(arguments, "timeout_ms").unwrap_or(5000);
    if let Some(selector) = arguments.get("selector").and_then(|value| value.as_str()) {
        let selector = selector.to_string();
        return with_browser(runtime, |browser| async move {
            browser.wait_for(&selector, timeout_ms).await?;
            Ok(ToolResult::ok(format!("found {}", selector)))
        })
        .await;
    }
    if let Some(text) = arguments.get("text").and_then(|value| value.as_str()) {
        let text = text.to_string();
        return with_browser(runtime, |browser| async move {
            browser.wait_for_text(&text, timeout_ms).await?;
            Ok(ToolResult::ok(format!("found text {}", text)))
        })
        .await;
    }
    ToolResult::err("browser_wait_for requires 'selector' or 'text'")
}

async fn browser_get_text(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let selector = arguments
        .get("selector")
        .and_then(|value| value.as_str())
        .unwrap_or("body")
        .to_string();

    with_browser(runtime, |browser| async move {
        let text = browser.extract_text(&selector).await?;
        Ok(ToolResult::ok(text))
    })
    .await
}

async fn browser_get_html(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let selector = arguments
        .get("selector")
        .and_then(|value| value.as_str())
        .unwrap_or("body")
        .to_string();

    with_browser(runtime, |browser| async move {
        let html = browser.extract_html(&selector).await?;
        Ok(ToolResult::ok(truncate_output(&html)))
    })
    .await
}

async fn browser_screenshot(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let full_page = arguments
        .get("full_page")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    with_browser(runtime, move |browser| async move {
        let bytes = if full_page {
            browser.screenshot().await?
        } else {
            browser.screenshot().await?
        };
        Ok(ToolResult::ok(format!("captured {} bytes", bytes.len()))
            .with_metadata("bytes", bytes.len().to_string()))
    })
    .await
}

async fn stt_transcribe(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let format = match audio_format(arguments.get("format").and_then(|value| value.as_str())) {
        Ok(format) => format,
        Err(err) => return ToolResult::err(err),
    };
    let resolved = match resolve_workspace_path(&runtime.workspace_root, &path) {
        Ok(path) => path,
        Err(err) => return ToolResult::err(err),
    };
    let audio = match std::fs::read(&resolved) {
        Ok(audio) => audio,
        Err(err) => return ToolResult::err(err.to_string()),
    };
    let client = SttClient::new(runtime.skills.stt.backend_config());

    match client.transcribe(&audio, format).await {
        Ok(text) => ToolResult::ok(text),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn tts_speak(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let text = match get_required_string(arguments, "text") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let client = TtsClient::new(resolve_tts_config(&runtime.skills.tts));

    match client.speak(&text).await {
        Ok(audio) => ToolResult::ok(format!("generated {} bytes", audio.len()))
            .with_metadata("bytes", audio.len().to_string()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn image_generate(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let prompt = match get_required_string(arguments, "prompt") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let mut params = ImageParams::default();
    if let Some(width) = get_u64(arguments, "width") {
        params.width = width as u32;
    }
    if let Some(height) = get_u64(arguments, "height") {
        params.height = height as u32;
    }
    let mut client = ImageClient::new(runtime.skills.image.backend());
    let openai_api_key = resolve_env_reference(&runtime.skills.image.dalle.api_key);
    if !openai_api_key.is_empty() {
        client = client.with_openai_api_key(openai_api_key);
    }

    match client.generate(&prompt, params).await {
        Ok(image) => ToolResult::ok(format!(
            "generated {:?} image ({} bytes)",
            image.format,
            image.bytes.as_ref().map(|bytes| bytes.len()).unwrap_or(0)
        )),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn qr_encode(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    let data = match get_required_string(arguments, "data") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let format = match arguments.get("format").and_then(|value| value.as_str()) {
        Some("svg") => QrFormat::Svg,
        Some("terminal") => QrFormat::Terminal,
        _ => QrFormat::default(),
    };

    match QrSkill::encode(&data, format) {
        Ok(bytes) => ToolResult::ok(format!("generated {} bytes", bytes.len()))
            .with_metadata("bytes", bytes.len().to_string()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn url_shorten(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let url = match get_required_string(arguments, "url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let shortener = UrlShortener::new(resolve_url_shortener_provider(&runtime.skills));
    match shortener.shorten(&url).await {
        Ok(short) => ToolResult::ok(short),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn url_expand(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let url = match get_required_string(arguments, "url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let shortener = UrlShortener::new(resolve_url_shortener_provider(&runtime.skills));
    match shortener.expand(&url).await {
        Ok(expanded) => ToolResult::ok(expanded),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

fn github_client(runtime: &ToolRuntime) -> Result<GitHubClient, String> {
    let mut config = runtime
        .skills
        .github
        .client_config()
        .ok_or_else(|| "skills.github.token is not configured".to_string())?;
    config.token = resolve_env_reference(&config.token);
    Ok(GitHubClient::with_safety(
        config,
        runtime.skills.github.safety_policy(),
    ))
}

fn google_client(runtime: &ToolRuntime) -> GoogleClient {
    let mut config = runtime.skills.google.clone();
    config.client_id = resolve_env_reference(&config.client_id);
    config.client_secret = resolve_env_reference(&config.client_secret);
    GoogleClient::new(config)
}

async fn with_browser<F, Fut>(runtime: &ToolRuntime, f: F) -> ToolResult
where
    F: FnOnce(Box<dyn BrowserSkill>) -> Fut,
    Fut: std::future::Future<Output = Result<ToolResult, crate::skills::browser::BrowserError>>,
{
    let config = runtime.skills.browser.clone();
    let browser: Box<dyn BrowserSkill> = if let Some(cdp_url) = &config.cdp_url {
        let client = CdpClient::new(cdp_url.clone());
        if let Err(err) = client.launch().await {
            return ToolResult::err(err.to_string());
        }
        Box::new(client)
    } else {
        let client = PlaywrightClient::new(config);
        if let Err(err) = client.launch().await {
            return ToolResult::err(err.to_string());
        }
        Box::new(client)
    };

    let result = f(browser).await;
    match result {
        Ok(result) => result,
        Err(err) => ToolResult::err(err.to_string()),
    }
}

fn audio_format(value: Option<&str>) -> Result<crate::skills::AudioFormat, String> {
    match value.unwrap_or("wav") {
        "wav" => Ok(crate::skills::AudioFormat::Wav),
        "mp3" => Ok(crate::skills::AudioFormat::Mp3),
        "webm" => Ok(crate::skills::AudioFormat::Webm),
        "m4a" => Ok(crate::skills::AudioFormat::M4a),
        "ogg" => Ok(crate::skills::AudioFormat::Ogg),
        other => Err(format!("unsupported audio format '{}'", other)),
    }
}

fn resolve_tts_config(config: &crate::config::TtsSkillConfig) -> crate::skills::ElevenLabsConfig {
    let mut resolved = config.elevenlabs.clone();
    resolved.api_key = resolve_env_reference(&resolved.api_key);
    resolved
}

fn resolve_url_shortener_provider(
    skills: &crate::config::SkillsConfig,
) -> crate::skills::UrlShortenerProvider {
    let mut provider = skills.url_shortener.provider_config();
    if let crate::skills::UrlShortenerProvider::Yourls(config) = &mut provider {
        config.api_url = resolve_env_reference(&config.api_url);
        config.signature = resolve_env_reference(&config.signature);
        config.username = config.username.as_ref().map(|value| resolve_env_reference(value));
        config.password = config.password.as_ref().map(|value| resolve_env_reference(value));
    }
    provider
}

fn resolve_env_reference(value: &str) -> String {
    if let Some(var) = value.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
        std::env::var(var).unwrap_or_default()
    } else {
        value.to_string()
    }
}

async fn mcp_list_tools(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let server = match get_required_string(arguments, "server") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let mut client = match mcp_client_for_server(runtime, &server) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    if let Err(err) = client.initialize().await {
        return ToolResult::err(err.to_string());
    }
    let result = client.list_tools().await;
    let _ = client.disconnect().await;

    match result {
        Ok(tools) if tools.is_empty() => ToolResult::ok("no tools"),
        Ok(tools) => ToolResult::ok(
            tools
                .into_iter()
                .map(|tool| format!("{} - {}", tool.name, tool.description.unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn mcp_call_tool(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let server = match get_required_string(arguments, "server") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let tool = match get_required_string(arguments, "tool") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let input = arguments
        .get("input")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let mut client = match mcp_client_for_server(runtime, &server) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    if let Err(err) = client.initialize().await {
        return ToolResult::err(err.to_string());
    }
    let result = client.call_tool(&tool, input).await;
    let _ = client.disconnect().await;

    match result {
        Ok(result) => {
            let output = result
                .content
                .into_iter()
                .map(|content| match content {
                    crate::mcp::types::McpContent::Text { text } => text,
                    crate::mcp::types::McpContent::Image { mime_type, .. } => {
                        format!("[image:{}]", mime_type)
                    }
                    crate::mcp::types::McpContent::Resource { resource } => {
                        format!("[resource:{}]", resource)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if result.is_error.unwrap_or(false) {
                ToolResult::err(output)
            } else {
                ToolResult::ok(output)
                    .with_metadata("server", server)
                    .with_metadata("tool", tool)
            }
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

fn mcp_client_for_server(runtime: &ToolRuntime, server: &str) -> Result<McpClient, String> {
    let config = runtime
        .mcp_servers
        .get(server)
        .ok_or_else(|| format!("unknown mcp server '{}'", server))?;
    let transport_config = match config.transport.as_str() {
        "stdio" => McpTransportConfig::Stdio(StdioTransportConfig {
            command: config
                .command
                .clone()
                .ok_or_else(|| "missing MCP command".to_string())?,
            args: config.args.clone(),
            env: config.env.clone(),
        }),
        "sse" => McpTransportConfig::Sse(SseTransportConfig {
            url: config
                .url
                .clone()
                .ok_or_else(|| "missing MCP url".to_string())?,
            headers: config.headers.clone(),
        }),
        "websocket" => McpTransportConfig::WebSocket(WebSocketTransportConfig {
            url: config
                .url
                .clone()
                .ok_or_else(|| "missing MCP url".to_string())?,
        }),
        other => return Err(format!("unsupported MCP transport '{}'", other)),
    };

    Ok(McpClient::new(McpClientConfig {
        name: "borgclaw".to_string(),
        transport_config,
        protocol_version: "2024-11-05".to_string(),
    }))
}

async fn approve_tool(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
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

fn get_required_string(
    arguments: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Result<String, String> {
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

    let normalized = std::fs::canonicalize(&candidate)
        .or_else(|_| {
            candidate
                .parent()
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "path has no parent")
                })
                .and_then(|parent| {
                    std::fs::canonicalize(parent)
                        .map(|canon| canon.join(candidate.file_name().unwrap_or_default()))
                })
        })
        .map_err(|e| e.to_string())?;

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

#[derive(Debug, PartialEq, Eq)]
struct SearchResult {
    title: String,
    url: String,
    snippet: Option<String>,
}

fn parse_duckduckgo_results(body: &str, limit: usize) -> Vec<SearchResult> {
    let result_re = Regex::new(
        r#"(?s)<a[^>]*class="result__a"[^>]*href="(?P<url>[^"]+)"[^>]*>(?P<title>.*?)</a>.*?(?:<a[^>]*class="result__snippet"[^>]*>|<div[^>]*class="result__snippet"[^>]*>)(?P<snippet>.*?)(?:</a>|</div>)"#,
    )
    .expect("valid regex");
    let tag_re = Regex::new(r"<[^>]+>").expect("valid regex");

    result_re
        .captures_iter(body)
        .take(limit)
        .filter_map(|capture| {
            let title = decode_html_entities(tag_re.replace_all(&capture["title"], "").trim());
            let url = decode_html_entities(capture["url"].trim());
            let snippet = decode_html_entities(tag_re.replace_all(&capture["snippet"], "").trim());
            if title.is_empty() || url.is_empty() {
                return None;
            }

            Some(SearchResult {
                title,
                url,
                snippet: if snippet.is_empty() {
                    None
                } else {
                    Some(snippet)
                },
            })
        })
        .collect()
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn string_property(description: &str) -> PropertySchema {
    PropertySchema {
        prop_type: "string".to_string(),
        description: Some(description.to_string()),
        default: None,
        enum_values: None,
    }
}

fn number_property(description: &str, default: serde_json::Value) -> PropertySchema {
    PropertySchema {
        prop_type: "number".to_string(),
        description: Some(description.to_string()),
        default: Some(default),
        enum_values: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AgentConfig, ApprovalMode, McpConfig, McpServerConfig, MemoryConfig, SecurityConfig,
    };

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
        let workspace =
            std::env::temp_dir().join(format!("borgclaw_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&workspace).unwrap();
        let escaped = resolve_workspace_path(&workspace, "../outside.txt");
        std::fs::remove_dir_all(&workspace).unwrap();
        assert!(escaped.is_err());
    }

    #[tokio::test]
    async fn supervised_command_requires_then_uses_approval() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_runtime_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
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

        assert_eq!(
            first.metadata.get("approval_required").map(String::as_str),
            Some("true")
        );
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

    #[test]
    fn parses_duckduckgo_results() {
        let html = r#"
            <div class="result">
              <a class="result__a" href="https://example.com/one">Example &amp; One</a>
              <div class="result__snippet">First <b>snippet</b></div>
            </div>
            <div class="result">
              <a class="result__a" href="https://example.com/two">Example Two</a>
              <a class="result__snippet">Second snippet</a>
            </div>
        "#;

        let results = parse_duckduckgo_results(html, 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Example & One");
        assert_eq!(results[0].url, "https://example.com/one");
        assert_eq!(results[0].snippet.as_deref(), Some("First snippet"));
    }

    #[tokio::test]
    async fn plugin_list_reports_empty_registry() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_plugin_list_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        let result = execute_tool(&ToolCall::new("plugin_list", HashMap::new()), &runtime).await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(result.output, "no plugins loaded");
    }

    #[tokio::test]
    async fn plugin_invoke_fails_for_missing_plugin() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_plugin_invoke_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        let result = execute_tool(
            &ToolCall::new(
                "plugin_invoke",
                HashMap::from([("plugin".to_string(), serde_json::json!("missing"))]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(!result.success);
        assert!(result.output.contains("Plugin not found"));
    }

    #[tokio::test]
    async fn execute_command_receives_security_secret_env() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_secret_env_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        runtime
            .security
            .store_secret("api_key", "from-security")
            .await
            .unwrap();

        let result = execute_tool(
            &ToolCall::new(
                "execute_command",
                HashMap::from([(
                    "command".to_string(),
                    serde_json::json!("printf %s \"$BC_SECRET_API_KEY\""),
                )]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(result.output, "from-security");
    }

    #[tokio::test]
    async fn execute_tool_redacts_detected_secret_leaks() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_redact_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        let result = execute_tool(
            &ToolCall::new(
                "message",
                HashMap::from([(
                    "text".to_string(),
                    serde_json::json!("sk-abcdefghijklmnopqrstuvwxyz1234"),
                )]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(result.output, "[REDACTED_SECRET]");
        assert_eq!(
            result.metadata.get("security_redacted").map(String::as_str),
            Some("true")
        );
    }

    #[tokio::test]
    async fn execute_tool_warns_on_detected_secret_leaks_when_configured() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_warn_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        let mut security = SecurityConfig::default();
        security.leak_action = crate::config::LeakAction::Warn;

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &security,
        )
        .await
        .unwrap();

        let result = execute_tool(
            &ToolCall::new(
                "message",
                HashMap::from([(
                    "text".to_string(),
                    serde_json::json!("sk-abcdefghijklmnopqrstuvwxyz1234"),
                )]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(result.output, "sk-abcdefghijklmnopqrstuvwxyz1234");
        assert_eq!(
            result.metadata.get("security_warning").map(String::as_str),
            Some("secret_detected")
        );
    }

    #[tokio::test]
    async fn execute_tool_blocks_detected_secret_leaks_when_configured() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_block_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        let mut security = SecurityConfig::default();
        security.leak_action = crate::config::LeakAction::Block;

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &security,
        )
        .await
        .unwrap();

        let result = execute_tool(
            &ToolCall::new(
                "message",
                HashMap::from([(
                    "text".to_string(),
                    serde_json::json!("sk-abcdefghijklmnopqrstuvwxyz1234"),
                )]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(!result.success);
        assert_eq!(
            result.output,
            "tool output blocked by secret leak detection"
        );
        assert_eq!(
            result.metadata.get("security_blocked").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn mcp_client_requires_known_server() {
        let runtime = ToolRuntime {
            workspace_root: PathBuf::from("."),
            memory: Arc::new(SqliteMemory::new(
                std::env::temp_dir().join("borgclaw_mcp_unknown_memory"),
            )),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            plugins: Arc::new(PluginRegistry::new()),
            skills: crate::config::SkillsConfig::default(),
            mcp_servers: HashMap::new(),
            security: Arc::new(SecurityLayer::with_config(SecurityConfig::default())),
        };

        let err = match mcp_client_for_server(&runtime, "missing") {
            Ok(_) => panic!("expected unknown server error"),
            Err(err) => err,
        };
        assert!(err.contains("unknown mcp server"));
    }

    #[test]
    fn mcp_client_rejects_unsupported_transport() {
        let runtime = ToolRuntime {
            workspace_root: PathBuf::from("."),
            memory: Arc::new(SqliteMemory::new(
                std::env::temp_dir().join("borgclaw_mcp_transport_memory"),
            )),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            plugins: Arc::new(PluginRegistry::new()),
            skills: crate::config::SkillsConfig::default(),
            mcp_servers: HashMap::from([(
                "bad".to_string(),
                McpServerConfig {
                    transport: "invalid".to_string(),
                    ..Default::default()
                },
            )]),
            security: Arc::new(SecurityLayer::with_config(SecurityConfig::default())),
        };

        let err = match mcp_client_for_server(&runtime, "bad") {
            Ok(_) => panic!("expected unsupported transport error"),
            Err(err) => err,
        };
        assert!(err.contains("unsupported MCP transport"));
    }

    #[tokio::test]
    async fn runtime_loads_mcp_servers_from_config() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_mcp_runtime_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &McpConfig {
                servers: HashMap::from([(
                    "filesystem".to_string(),
                    McpServerConfig {
                        transport: "stdio".to_string(),
                        command: Some("mcp-filesystem".to_string()),
                        ..Default::default()
                    },
                )]),
            },
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert!(runtime.mcp_servers.contains_key("filesystem"));
    }
}

/// Built-in tools
pub fn builtin_tools() -> Vec<Tool> {
    vec![
        Tool::new("memory_store", "Store information in long-term memory")
            .with_schema(ToolSchema::object(
                [
                    (
                        "key".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Memory key".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "value".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Information to store".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["key".to_string(), "value".to_string()],
            ))
            .with_tags(vec!["memory".to_string()]),
        Tool::new("memory_recall", "Recall information from long-term memory")
            .with_schema(ToolSchema::object(
                [
                    (
                        "query".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Search query".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "limit".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Max results".to_string()),
                            default: Some(serde_json::json!(5)),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["query".to_string()],
            ))
            .with_tags(vec!["memory".to_string()]),
        Tool::new("execute_command", "Execute a shell command")
            .with_schema(ToolSchema::object(
                [
                    (
                        "command".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Command to execute".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "timeout".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Timeout in seconds".to_string()),
                            default: Some(serde_json::json!(60)),
                            enum_values: None,
                        },
                    ),
                    (
                        "approval_token".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Approval token for protected commands".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["command".to_string()],
            ))
            .with_approval(true)
            .with_tags(vec!["system".to_string()]),
        Tool::new("read_file", "Read a file from the filesystem")
            .with_schema(ToolSchema::object(
                [
                    (
                        "path".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("File path".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "offset".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Line offset".to_string()),
                            default: Some(serde_json::json!(0)),
                            enum_values: None,
                        },
                    ),
                    (
                        "limit".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Number of lines".to_string()),
                            default: Some(serde_json::json!(100)),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["path".to_string()],
            ))
            .with_tags(vec!["filesystem".to_string()]),
        Tool::new("list_directory", "List files in a directory")
            .with_schema(ToolSchema::object(
                [(
                    "path".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Directory path".to_string()),
                        default: None,
                        enum_values: None,
                    },
                )]
                .into(),
                vec!["path".to_string()],
            ))
            .with_tags(vec!["filesystem".to_string()]),
        Tool::new("web_search", "Search the web")
            .with_schema(ToolSchema::object(
                [
                    (
                        "query".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Search query".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "num_results".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Number of results".to_string()),
                            default: Some(serde_json::json!(5)),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["query".to_string()],
            ))
            .with_tags(vec!["web".to_string()]),
        Tool::new("plugin_list", "List loaded WASM plugins")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["plugin".to_string()]),
        Tool::new("plugin_invoke", "Invoke a loaded WASM plugin")
            .with_schema(ToolSchema::object(
                [
                    (
                        "plugin".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Plugin name".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "function".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Exported function to call".to_string()),
                            default: Some(serde_json::json!("invoke")),
                            enum_values: None,
                        },
                    ),
                    (
                        "input".to_string(),
                        PropertySchema {
                            prop_type: "object".to_string(),
                            description: Some("JSON input passed to the plugin".to_string()),
                            default: Some(serde_json::json!({})),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["plugin".to_string()],
            ))
            .with_tags(vec!["plugin".to_string()]),
        Tool::new("github_list_repos", "List accessible GitHub repositories")
            .with_schema(ToolSchema::object(
                [(
                    "visibility".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Optional visibility filter".to_string()),
                        default: None,
                        enum_values: None,
                    },
                )]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_create_pr", "Create a GitHub pull request")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("title".to_string(), string_property("Pull request title")),
                    ("head".to_string(), string_property("Head branch")),
                    ("base".to_string(), string_property("Base branch")),
                    ("body".to_string(), string_property("Pull request body")),
                ]
                .into(),
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "title".to_string(),
                    "head".to_string(),
                    "base".to_string(),
                ],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new(
            "github_prepare_delete_branch",
            "Prepare a GitHub branch deletion confirmation",
        )
        .with_schema(ToolSchema::object(
            [
                ("owner".to_string(), string_property("Repository owner")),
                ("repo".to_string(), string_property("Repository name")),
                ("branch".to_string(), string_property("Branch name")),
            ]
            .into(),
            vec!["owner".to_string(), "repo".to_string(), "branch".to_string()],
        ))
        .with_tags(vec!["github".to_string(), "security".to_string()]),
        Tool::new("github_delete_branch", "Delete a GitHub branch")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("branch".to_string(), string_property("Branch name")),
                    (
                        "confirmation_token".to_string(),
                        string_property("Confirmation token from preparation step"),
                    ),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string(), "branch".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new(
            "github_prepare_merge_pr",
            "Prepare a GitHub pull request merge confirmation",
        )
        .with_schema(ToolSchema::object(
            [
                ("owner".to_string(), string_property("Repository owner")),
                ("repo".to_string(), string_property("Repository name")),
                (
                    "number".to_string(),
                    number_property("Pull request number", serde_json::json!(1)),
                ),
            ]
            .into(),
            vec!["owner".to_string(), "repo".to_string(), "number".to_string()],
        ))
        .with_tags(vec!["github".to_string(), "security".to_string()]),
        Tool::new("github_merge_pr", "Merge a GitHub pull request")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    (
                        "number".to_string(),
                        number_property("Pull request number", serde_json::json!(1)),
                    ),
                    (
                        "confirmation_token".to_string(),
                        string_property("Confirmation token from preparation step"),
                    ),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string(), "number".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("google_list_messages", "List Gmail messages")
            .with_schema(ToolSchema::object(
                [
                    ("query".to_string(), string_property("Optional Gmail query")),
                    (
                        "limit".to_string(),
                        number_property("Maximum messages", serde_json::json!(10)),
                    ),
                ]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_send_email", "Send an email via Gmail")
            .with_schema(ToolSchema::object(
                [
                    ("to".to_string(), string_property("Recipient email address")),
                    ("subject".to_string(), string_property("Email subject")),
                    ("body".to_string(), string_property("Email body")),
                ]
                .into(),
                vec!["to".to_string(), "subject".to_string(), "body".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_search_files", "Search Google Drive files")
            .with_schema(ToolSchema::object(
                [("query".to_string(), string_property("Drive search query"))].into(),
                vec!["query".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_list_events", "List Google Calendar events")
            .with_schema(ToolSchema::object(
                [
                    (
                        "calendar_id".to_string(),
                        string_property("Calendar identifier"),
                    ),
                    (
                        "days".to_string(),
                        number_property("Days ahead to query", serde_json::json!(7)),
                    ),
                ]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_upload_file", "Upload a file to Google Drive")
            .with_schema(ToolSchema::object(
                [
                    ("path".to_string(), string_property("Local file path")),
                    ("mime_type".to_string(), string_property("File MIME type")),
                    ("folder_id".to_string(), string_property("Optional Drive folder id")),
                ]
                .into(),
                vec!["path".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_create_event", "Create a Google Calendar event")
            .with_schema(ToolSchema::object(
                [
                    ("summary".to_string(), string_property("Event summary")),
                    (
                        "description".to_string(),
                        string_property("Optional event description"),
                    ),
                    (
                        "start".to_string(),
                        string_property("RFC3339 start time"),
                    ),
                    ("end".to_string(), string_property("RFC3339 end time")),
                ]
                .into(),
                vec!["summary".to_string(), "start".to_string(), "end".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("browser_navigate", "Navigate the browser to a URL")
            .with_schema(ToolSchema::object(
                [("url".to_string(), string_property("Target URL"))].into(),
                vec!["url".to_string()],
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_click", "Click an element in the current page")
            .with_schema(ToolSchema::object(
                [("selector".to_string(), string_property("CSS selector"))].into(),
                vec!["selector".to_string()],
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_fill", "Fill an input element in the current page")
            .with_schema(ToolSchema::object(
                [
                    ("selector".to_string(), string_property("CSS selector")),
                    ("value".to_string(), string_property("Input value")),
                ]
                .into(),
                vec!["selector".to_string(), "value".to_string()],
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_wait_for", "Wait for a selector or text on the current page")
            .with_schema(ToolSchema::object(
                [
                    ("selector".to_string(), string_property("Optional CSS selector")),
                    ("text".to_string(), string_property("Optional page text")),
                    (
                        "timeout_ms".to_string(),
                        number_property("Timeout in milliseconds", serde_json::json!(5000)),
                    ),
                ]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_get_text", "Extract text from the current page")
            .with_schema(ToolSchema::object(
                [("selector".to_string(), string_property("CSS selector"))].into(),
                Vec::new(),
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_get_html", "Extract HTML from the current page")
            .with_schema(ToolSchema::object(
                [("selector".to_string(), string_property("CSS selector"))].into(),
                Vec::new(),
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_screenshot", "Capture a screenshot from the current page")
            .with_schema(ToolSchema::object(
                [(
                    "full_page".to_string(),
                    PropertySchema {
                        prop_type: "boolean".to_string(),
                        description: Some("Capture the full page".to_string()),
                        default: Some(serde_json::json!(true)),
                        enum_values: None,
                    },
                )]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("stt_transcribe", "Transcribe audio to text")
            .with_schema(ToolSchema::object(
                [
                    ("path".to_string(), string_property("Audio file path")),
                    ("format".to_string(), string_property("Audio format")),
                ]
                .into(),
                vec!["path".to_string()],
            ))
            .with_tags(vec!["stt".to_string(), "integration".to_string()]),
        Tool::new("tts_speak", "Synthesize speech from text")
            .with_schema(ToolSchema::object(
                [("text".to_string(), string_property("Text to synthesize"))].into(),
                vec!["text".to_string()],
            ))
            .with_tags(vec!["tts".to_string(), "integration".to_string()]),
        Tool::new("image_generate", "Generate an image from a prompt")
            .with_schema(ToolSchema::object(
                [
                    ("prompt".to_string(), string_property("Image prompt")),
                    (
                        "width".to_string(),
                        number_property("Image width", serde_json::json!(1024)),
                    ),
                    (
                        "height".to_string(),
                        number_property("Image height", serde_json::json!(1024)),
                    ),
                ]
                .into(),
                vec!["prompt".to_string()],
            ))
            .with_tags(vec!["image".to_string(), "integration".to_string()]),
        Tool::new("qr_encode", "Generate a QR code")
            .with_schema(ToolSchema::object(
                [
                    ("data".to_string(), string_property("Data to encode")),
                    ("format".to_string(), string_property("png, svg, or terminal")),
                ]
                .into(),
                vec!["data".to_string()],
            ))
            .with_tags(vec!["qr".to_string(), "integration".to_string()]),
        Tool::new("url_shorten", "Shorten a URL")
            .with_schema(ToolSchema::object(
                [("url".to_string(), string_property("URL to shorten"))].into(),
                vec!["url".to_string()],
            ))
            .with_tags(vec!["url".to_string(), "integration".to_string()]),
        Tool::new("url_expand", "Expand a shortened URL")
            .with_schema(ToolSchema::object(
                [("url".to_string(), string_property("Shortened URL"))].into(),
                vec!["url".to_string()],
            ))
            .with_tags(vec!["url".to_string(), "integration".to_string()]),
        Tool::new(
            "mcp_list_tools",
            "List tools exposed by a configured MCP server",
        )
        .with_schema(ToolSchema::object(
            [(
                "server".to_string(),
                PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Configured MCP server name".to_string()),
                    default: None,
                    enum_values: None,
                },
            )]
            .into(),
            vec!["server".to_string()],
        ))
        .with_tags(vec!["mcp".to_string(), "integration".to_string()]),
        Tool::new(
            "mcp_call_tool",
            "Call a tool exposed by a configured MCP server",
        )
        .with_schema(ToolSchema::object(
            [
                (
                    "server".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Configured MCP server name".to_string()),
                        default: None,
                        enum_values: None,
                    },
                ),
                (
                    "tool".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Remote MCP tool name".to_string()),
                        default: None,
                        enum_values: None,
                    },
                ),
                (
                    "input".to_string(),
                    PropertySchema {
                        prop_type: "object".to_string(),
                        description: Some(
                            "JSON input object passed to the remote tool".to_string(),
                        ),
                        default: Some(serde_json::json!({})),
                        enum_values: None,
                    },
                ),
            ]
            .into(),
            vec!["server".to_string(), "tool".to_string()],
        ))
        .with_tags(vec!["mcp".to_string(), "integration".to_string()]),
        Tool::new("fetch_url", "Fetch content from a URL")
            .with_schema(ToolSchema::object(
                [(
                    "url".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("URL to fetch".to_string()),
                        default: None,
                        enum_values: None,
                    },
                )]
                .into(),
                vec!["url".to_string()],
            ))
            .with_tags(vec!["web".to_string()]),
        Tool::new("message", "Send a message to the user")
            .with_schema(ToolSchema::object(
                [(
                    "text".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Message text".to_string()),
                        default: None,
                        enum_values: None,
                    },
                )]
                .into(),
                vec!["text".to_string()],
            ))
            .with_tags(vec!["communication".to_string()]),
        Tool::new("schedule_task", "Schedule a task to run later")
            .with_schema(ToolSchema::object(
                [
                    (
                        "message".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Task description".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "cron".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Cron expression".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["message".to_string()],
            ))
            .with_tags(vec!["scheduling".to_string()]),
        Tool::new("approve", "Approve a pending protected tool operation")
            .with_schema(ToolSchema::object(
                [
                    (
                        "tool".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Tool name being approved".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "token".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Approval token".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["tool".to_string(), "token".to_string()],
            ))
            .with_tags(vec!["security".to_string()]),
    ]
}
