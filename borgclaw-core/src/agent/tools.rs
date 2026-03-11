//! Tools module - defines tools agents can use

use crate::mcp::client::{McpClient, McpClientConfig};
use crate::mcp::transport::{
    McpTransportConfig, SseTransportConfig, StdioTransportConfig, WebSocketTransportConfig,
};
use crate::memory::HeartbeatEngine;
use crate::memory::{new_entry, new_entry_for_group, Memory, MemoryQuery, SqliteMemory};
use crate::scheduler::{new_job, JobTrigger, Scheduler, SchedulerError, SchedulerTrait};
use crate::security::{CommandCheck, SecurityLayer};
use crate::skills::{
    BrowserSkill, CdpClient, GitHubClient, GoogleClient, ImageClient, ImageParams,
    PlaywrightClient, PluginRegistry, QrFormat, QrSkill, SttClient, TtsClient, UrlShortener,
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
    pub workspace_policy: crate::config::WorkspacePolicyConfig,
    pub memory: Arc<SqliteMemory>,
    pub heartbeat: Arc<HeartbeatEngine>,
    pub scheduler: Arc<Mutex<Scheduler>>,
    pub heartbeat_config: crate::config::HeartbeatConfig,
    pub scheduler_config: crate::config::SchedulerConfig,
    pub plugins: Arc<PluginRegistry>,
    pub skills: crate::config::SkillsConfig,
    pub mcp_servers: HashMap<String, crate::config::McpServerConfig>,
    pub security: Arc<SecurityLayer>,
    pub invocation: Option<Arc<ToolInvocationContext>>,
}

#[derive(Debug, Clone)]
pub struct ToolInvocationContext {
    pub session_id: super::SessionId,
    pub sender: super::SenderInfo,
    pub metadata: HashMap<String, String>,
}

impl ToolRuntime {
    pub async fn from_config(
        agent: &crate::config::AgentConfig,
        memory_config: &crate::config::MemoryConfig,
        heartbeat_config: &crate::config::HeartbeatConfig,
        scheduler_config: &crate::config::SchedulerConfig,
        skills_config: &crate::config::SkillsConfig,
        mcp_config: &crate::config::McpConfig,
        security_config: &crate::config::SecurityConfig,
    ) -> Result<Self, String> {
        let workspace_root = canonical_or_current(&agent.workspace);
        let memory = Arc::new(SqliteMemory::new(memory_config.database_path.clone()));
        memory.init().await.map_err(|e| e.to_string())?;
        let heartbeat = Arc::new(
            HeartbeatEngine::new()
                .with_state_path(agent.workspace.join("heartbeat.json"))
                .with_poll_interval(std::time::Duration::from_secs(
                    heartbeat_config.check_interval_seconds.max(1),
                )),
        );
        let plugins = Arc::new(
            PluginRegistry::new()
                .with_workspace_policy(workspace_root.clone(), security_config.workspace.clone()),
        );
        plugins
            .load_from_dir(&skills_config.skills_path)
            .await
            .map_err(|e| e.to_string())?;

        let runtime = Self {
            workspace_root,
            workspace_policy: security_config.workspace.clone(),
            memory,
            heartbeat,
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            heartbeat_config: heartbeat_config.clone(),
            scheduler_config: scheduler_config.clone(),
            plugins,
            skills: skills_config.clone(),
            mcp_servers: mcp_config.servers.clone(),
            security: Arc::new(SecurityLayer::with_config(security_config.clone())),
            invocation: None,
        };

        if heartbeat_config.enabled {
            runtime.start_heartbeat_loop().await?;
        }

        if scheduler_config.enabled {
            runtime.start_scheduler_loop().await?;
        }

        Ok(runtime)
    }

    pub fn with_context(&self, ctx: &super::AgentContext) -> Self {
        let mut runtime = self.clone();
        runtime.invocation = Some(Arc::new(ToolInvocationContext {
            session_id: ctx.session_id.clone(),
            sender: ctx.sender.clone(),
            metadata: ctx.metadata.clone(),
        }));
        runtime
    }

    fn with_scheduled_job_context(&self, job: &crate::scheduler::Job) -> Self {
        let session_id = job
            .metadata
            .get("scheduled_session_id")
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("scheduled:{}", job.id));
        let sender = super::SenderInfo {
            id: job
                .metadata
                .get("scheduled_sender_id")
                .cloned()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| format!("scheduler:{}", job.id)),
            name: job.metadata.get("scheduled_sender_name").cloned(),
            channel: job
                .metadata
                .get("scheduled_sender_channel")
                .cloned()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "scheduler".to_string()),
        };
        let metadata = scheduled_metadata(job);

        let mut runtime = self.clone();
        runtime.invocation = Some(Arc::new(ToolInvocationContext {
            session_id: super::SessionId(session_id),
            sender,
            metadata,
        }));
        runtime
    }

    async fn start_heartbeat_loop(&self) -> Result<(), String> {
        self.heartbeat
            .start_with_interval(std::time::Duration::from_secs(
                self.heartbeat_config.check_interval_seconds.max(1),
            ))
            .await
    }

    async fn start_scheduler_loop(&self) -> Result<(), String> {
        let scheduler = self.scheduler.lock().await;
        scheduler
            .start_with_policy(
                std::time::Duration::from_secs(1),
                self.scheduler_config.max_concurrent_jobs,
                Some(std::time::Duration::from_secs(
                    self.scheduler_config.job_timeout.max(1),
                )),
                Arc::new({
                    let runtime = self.clone();
                    move |job| {
                        let runtime = runtime.clone();
                        Box::pin(async move { execute_scheduled_job(&job, &runtime).await })
                    }
                }),
            )
            .await
            .map_err(|err| err.to_string())
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
        "run_scheduled_tasks" => run_scheduled_tasks(runtime).await,
        "approve" => approve_tool(&call.arguments, runtime).await,
        "web_search" => web_search(&call.arguments).await,
        "plugin_list" => plugin_list(runtime).await,
        "plugin_invoke" => plugin_invoke(&call.arguments, runtime).await,
        "github_list_repos" => github_list_repos(&call.arguments, runtime).await,
        "github_get_repo" => github_get_repo(&call.arguments, runtime).await,
        "github_list_branches" => github_list_branches(&call.arguments, runtime).await,
        "github_create_branch" => github_create_branch(&call.arguments, runtime).await,
        "github_list_prs" => github_list_prs(&call.arguments, runtime).await,
        "github_create_pr" => github_create_pr(&call.arguments, runtime).await,
        "github_prepare_delete_branch" => {
            github_prepare_delete_branch(&call.arguments, runtime).await
        }
        "github_delete_branch" => github_delete_branch(&call.arguments, runtime).await,
        "github_prepare_merge_pr" => github_prepare_merge_pr(&call.arguments, runtime).await,
        "github_merge_pr" => github_merge_pr(&call.arguments, runtime).await,
        "github_list_issues" => github_list_issues(&call.arguments, runtime).await,
        "github_create_issue" => github_create_issue(&call.arguments, runtime).await,
        "github_list_releases" => github_list_releases(&call.arguments, runtime).await,
        "github_get_file" => github_get_file(&call.arguments, runtime).await,
        "github_create_file" => github_create_file(&call.arguments, runtime).await,
        "google_list_messages" => google_list_messages(&call.arguments, runtime).await,
        "google_get_message" => google_get_message(&call.arguments, runtime).await,
        "google_send_email" => google_send_email(&call.arguments, runtime).await,
        "google_search_files" => google_search_files(&call.arguments, runtime).await,
        "google_download_file" => google_download_file(&call.arguments, runtime).await,
        "google_list_events" => google_list_events(&call.arguments, runtime).await,
        "google_upload_file" => google_upload_file(&call.arguments, runtime).await,
        "google_create_event" => google_create_event(&call.arguments, runtime).await,
        "browser_navigate" => browser_navigate(&call.arguments, runtime).await,
        "browser_click" => browser_click(&call.arguments, runtime).await,
        "browser_fill" => browser_fill(&call.arguments, runtime).await,
        "browser_wait_for" => browser_wait_for(&call.arguments, runtime).await,
        "browser_get_text" => browser_get_text(&call.arguments, runtime).await,
        "browser_get_html" => browser_get_html(&call.arguments, runtime).await,
        "browser_get_url" => browser_get_url(runtime).await,
        "browser_eval_js" => browser_eval_js(&call.arguments, runtime).await,
        "browser_screenshot" => browser_screenshot(&call.arguments, runtime).await,
        "stt_transcribe" => stt_transcribe(&call.arguments, runtime).await,
        "tts_list_voices" => tts_list_voices(runtime).await,
        "tts_speak" => tts_speak(&call.arguments, runtime).await,
        "image_generate" => image_generate(&call.arguments, runtime).await,
        "qr_encode" => qr_encode(&call.arguments).await,
        "qr_encode_url" => qr_encode_url(&call.arguments).await,
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

async fn require_tool_approval(
    tool_name: &str,
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> Option<ToolResult> {
    if !runtime
        .security
        .needs_approval(tool_name, runtime.security.approval_mode())
    {
        return None;
    }

    let approval_token = arguments
        .get("approval_token")
        .and_then(|value| value.as_str());

    if let Some(token) = approval_token {
        if let Err(err) = runtime.security.consume_approval(tool_name, token).await {
            return Some(ToolResult::err(err.to_string()));
        }
        return None;
    }

    let token = runtime.security.request_approval(tool_name).await;
    Some(
        ToolResult::ok(format!(
            "approval required; rerun with /approve {{\"tool\":\"{}\",\"token\":\"{}\"}}",
            tool_name, token
        ))
        .with_metadata("approval_required", "true")
        .with_metadata("approval_tool", tool_name)
        .with_metadata("approval_token", token),
    )
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

    let group_id = optional_group_id(arguments, runtime);
    let entry = match group_id.clone() {
        Some(group_id) => new_entry_for_group(key, value, group_id),
        None => new_entry(key, value),
    };
    match runtime.memory.store(entry).await {
        Ok(()) => {
            let mut result = ToolResult::ok("stored");
            if let Some(group_id) = group_id {
                result = result.with_metadata("group_id", group_id);
            }
            result
        }
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
    let group_id = optional_group_id(arguments, runtime);

    match runtime
        .memory
        .recall(&MemoryQuery {
            query,
            limit,
            min_score: 0.0,
            group_id: group_id.clone(),
        })
        .await
    {
        Ok(results) if results.is_empty() => {
            let mut result = ToolResult::ok("no matching memories");
            if let Some(group_id) = group_id {
                result = result.with_metadata("group_id", group_id);
            }
            result
        }
        Ok(results) => {
            let mut result = ToolResult::ok(
                results
                    .into_iter()
                    .map(|result| format!("{}: {}", result.entry.key, result.entry.content))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            if let Some(group_id) = group_id {
                result = result.with_metadata("group_id", group_id);
            }
            result
        }
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
    if let Some(result) = require_tool_approval("execute_command", arguments, runtime).await {
        return result;
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

    let resolved =
        match resolve_workspace_path(&runtime.workspace_root, &runtime.workspace_policy, &path) {
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

    let resolved =
        match resolve_workspace_path(&runtime.workspace_root, &runtime.workspace_policy, path) {
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
    let scheduled = match scheduled_action(arguments) {
        Ok(action) => action,
        Err(err) => return ToolResult::err(err),
    };

    let trigger = match arguments.get("cron").and_then(|value| value.as_str()) {
        Some(cron) => JobTrigger::Cron(cron.to_string()),
        None => JobTrigger::OneShot(chrono::Utc::now() + chrono::Duration::seconds(1)),
    };

    let mut job = new_job(scheduled.job_name(), trigger, scheduled.job_action());
    scheduled.apply_metadata(&mut job.metadata);
    apply_invocation_metadata(runtime, &mut job.metadata);
    let id = {
        let scheduler = runtime.scheduler.lock().await;
        match scheduler.schedule(job).await {
            Ok(id) => id,
            Err(err) => return ToolResult::err(err.to_string()),
        }
    };

    ToolResult::ok(format!("scheduled {}", id))
}

async fn run_scheduled_tasks(runtime: &ToolRuntime) -> ToolResult {
    let scheduler = runtime.scheduler.lock().await;
    let results = scheduler
        .run_due(|job| {
            let runtime = runtime.clone();
            async move { execute_scheduled_job(&job, &runtime).await }
        })
        .await;

    if results.is_empty() {
        return ToolResult::ok("no due jobs");
    }

    let success_count = results.iter().filter(|result| result.is_ok()).count();
    let failure_count = results.len() - success_count;
    ToolResult::ok(format!(
        "executed {} scheduled jobs ({} ok, {} failed)",
        results.len(),
        success_count,
        failure_count
    ))
    .with_metadata("executed", results.len().to_string())
    .with_metadata("succeeded", success_count.to_string())
    .with_metadata("failed", failure_count.to_string())
}

async fn execute_scheduled_job(
    job: &crate::scheduler::Job,
    runtime: &ToolRuntime,
) -> Result<(), SchedulerError> {
    match job.metadata.get("action_tool").map(String::as_str) {
        Some("message") => {
            let result = message(&HashMap::from([(
                "text".to_string(),
                serde_json::json!(job
                    .metadata
                    .get("text")
                    .cloned()
                    .unwrap_or_else(|| job.action.clone())),
            )]));
            if result.success {
                Ok(())
            } else {
                Err(SchedulerError::JobFailed(result.output))
            }
        }
        Some("tool_call") => {
            let tool_name = job.metadata.get("tool_name").cloned().ok_or_else(|| {
                SchedulerError::Error("scheduled job missing tool_name".to_string())
            })?;
            if matches!(tool_name.as_str(), "schedule_task" | "run_scheduled_tasks") {
                return Err(SchedulerError::Error(format!(
                    "scheduled tool not allowed: {}",
                    tool_name
                )));
            }
            if runtime
                .security
                .needs_approval(&tool_name, runtime.security.approval_mode())
            {
                return Err(SchedulerError::Error(format!(
                    "scheduled background execution cannot satisfy approval for {}",
                    tool_name
                )));
            }
            let arguments = job
                .metadata
                .get("tool_arguments")
                .map(|raw| serde_json::from_str::<HashMap<String, serde_json::Value>>(raw))
                .transpose()
                .map_err(|err| SchedulerError::Error(err.to_string()))?
                .unwrap_or_default();
            let scheduled_runtime = runtime.with_scheduled_job_context(job);
            let result = execute_tool(
                &ToolCall::new(tool_name.clone(), arguments),
                &scheduled_runtime,
            )
            .await;
            if result.success {
                Ok(())
            } else {
                Err(SchedulerError::JobFailed(result.output))
            }
        }
        Some(other) => Err(SchedulerError::Error(format!(
            "unsupported scheduled action tool: {}",
            other
        ))),
        None => Err(SchedulerError::Error(
            "scheduled job missing action_tool metadata".to_string(),
        )),
    }
}

enum ScheduledAction {
    Message(String),
    ToolCall {
        tool_name: String,
        arguments: HashMap<String, serde_json::Value>,
    },
}

impl ScheduledAction {
    fn job_name(&self) -> String {
        match self {
            Self::Message(message) => message.clone(),
            Self::ToolCall { tool_name, .. } => format!("tool:{}", tool_name),
        }
    }

    fn job_action(&self) -> String {
        match self {
            Self::Message(message) => message.clone(),
            Self::ToolCall { tool_name, .. } => format!("tool:{}", tool_name),
        }
    }

    fn apply_metadata(&self, metadata: &mut HashMap<String, String>) {
        match self {
            Self::Message(message) => {
                metadata.insert("action_tool".to_string(), "message".to_string());
                metadata.insert("text".to_string(), message.clone());
            }
            Self::ToolCall {
                tool_name,
                arguments,
            } => {
                metadata.insert("action_tool".to_string(), "tool_call".to_string());
                metadata.insert("tool_name".to_string(), tool_name.clone());
                let raw_arguments =
                    serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
                metadata.insert("tool_arguments".to_string(), raw_arguments);
            }
        }
    }
}

fn scheduled_action(
    arguments: &HashMap<String, serde_json::Value>,
) -> Result<ScheduledAction, String> {
    match (
        arguments.get("message").and_then(|value| value.as_str()),
        arguments.get("tool").and_then(|value| value.as_str()),
    ) {
        (Some(message), None) => Ok(ScheduledAction::Message(message.to_string())),
        (None, Some(tool_name)) => {
            let tool_arguments = arguments
                .get("arguments")
                .and_then(|value| value.as_object())
                .map(|object| {
                    object
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default();
            Ok(ScheduledAction::ToolCall {
                tool_name: tool_name.to_string(),
                arguments: tool_arguments,
            })
        }
        (Some(_), Some(_)) => Err("provide either message or tool, not both".to_string()),
        (None, None) => Err("missing required argument: message or tool".to_string()),
    }
}

fn optional_group_id(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> Option<String> {
    arguments
        .get("group_id")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            runtime
                .invocation
                .as_ref()
                .and_then(|ctx| ctx.metadata.get("group_id").cloned())
        })
}

fn apply_invocation_metadata(runtime: &ToolRuntime, metadata: &mut HashMap<String, String>) {
    let Some(invocation) = runtime.invocation.as_ref() else {
        return;
    };

    metadata.insert(
        "scheduled_session_id".to_string(),
        invocation.session_id.0.clone(),
    );
    metadata.insert(
        "scheduled_sender_id".to_string(),
        invocation.sender.id.clone(),
    );
    metadata.insert(
        "scheduled_sender_channel".to_string(),
        invocation.sender.channel.clone(),
    );
    if let Some(name) = &invocation.sender.name {
        metadata.insert("scheduled_sender_name".to_string(), name.clone());
    }
    for (key, value) in &invocation.metadata {
        metadata.insert(format!("scheduled_meta_{}", key), value.clone());
    }
}

fn scheduled_metadata(job: &crate::scheduler::Job) -> HashMap<String, String> {
    job.metadata
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix("scheduled_meta_")
                .map(|key| (key.to_string(), value.clone()))
        })
        .collect()
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
    if let Some(result) = require_tool_approval("plugin_invoke", arguments, runtime).await {
        return result;
    }

    let plugin = match get_required_string(arguments, "plugin") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let function =
        if let Some(function) = arguments.get("function").and_then(|value| value.as_str()) {
            function.to_string()
        } else {
            runtime
                .plugins
                .get(&plugin)
                .await
                .map(|manifest| manifest.entry_point)
                .unwrap_or_else(|| "invoke".to_string())
        };
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
            repos
                .into_iter()
                .map(|repo| format!("{} ({})", repo.full_name, repo.default_branch))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_get_repo(
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

    match client.get_repo(&owner, &repo).await {
        Ok(repo) => ToolResult::ok(format!(
            "{} | default={} | private={}",
            repo.full_name, repo.default_branch, repo.private
        )),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_list_branches(
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

    match client.list_branches(&owner, &repo).await {
        Ok(branches) if branches.is_empty() => ToolResult::ok("no branches"),
        Ok(branches) => ToolResult::ok(
            branches
                .into_iter()
                .map(|branch| {
                    format!(
                        "{} | {} | protected={}",
                        branch.name, branch.sha, branch.protected
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_create_branch(
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
    let from_sha = match get_required_string(arguments, "from_sha") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client
        .create_branch(&owner, &repo, &branch, &from_sha)
        .await
    {
        Ok(created) => ToolResult::ok(format!("{} | {}", created.name, created.sha))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_list_prs(
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
    let state = arguments.get("state").and_then(|value| value.as_str());

    match client.list_prs(&owner, &repo, state).await {
        Ok(prs) if prs.is_empty() => ToolResult::ok("no pull requests"),
        Ok(prs) => ToolResult::ok(
            prs.into_iter()
                .map(|pr| format!("{} #{} [{}]", pr.title, pr.number, pr.state))
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

    match client
        .create_pr(&owner, &repo, &title, body, &head, &base)
        .await
    {
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

    match client
        .merge_pr(&owner, &repo, number, confirmation_token)
        .await
    {
        Ok(true) => ToolResult::ok(format!("merged pull request {}", number)),
        Ok(false) => ToolResult::err("merge request was not accepted"),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_list_issues(
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
    let state = arguments.get("state").and_then(|value| value.as_str());

    match client.list_issues(&owner, &repo, state).await {
        Ok(issues) if issues.is_empty() => ToolResult::ok("no issues"),
        Ok(issues) => ToolResult::ok(
            issues
                .into_iter()
                .map(|issue| format!("{} #{} [{}]", issue.title, issue.number, issue.state))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_create_issue(
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
    let body = arguments.get("body").and_then(|value| value.as_str());

    match client.create_issue(&owner, &repo, &title, body).await {
        Ok(issue) => ToolResult::ok(format!("{} #{}", issue.html_url, issue.number))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_list_releases(
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

    match client.list_releases(&owner, &repo).await {
        Ok(releases) if releases.is_empty() => ToolResult::ok("no releases"),
        Ok(releases) => ToolResult::ok(
            releases
                .into_iter()
                .map(|release| {
                    format!(
                        "{} | {} | draft={}",
                        release.tag_name,
                        release.name.unwrap_or_default(),
                        release.draft
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_get_file(
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
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let git_ref = arguments
        .get("ref")
        .and_then(|value| value.as_str())
        .unwrap_or("HEAD");

    match client.get_file(&owner, &repo, &path, git_ref).await {
        Ok(content) => ToolResult::ok(truncate_output(&content)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn github_create_file(
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
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let content = match get_required_string(arguments, "content") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let message = match get_required_string(arguments, "message") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let branch = arguments
        .get("branch")
        .and_then(|value| value.as_str())
        .unwrap_or("main");

    match client
        .create_file(&owner, &repo, &path, &content, &message, branch)
        .await
    {
        Ok(()) => ToolResult::ok(format!("created {}", path))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo)
            .with_metadata("branch", branch.to_string()),
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

async fn google_get_message(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let id = match get_required_string(arguments, "id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let gmail = crate::skills::GmailClient::new(client.auth());
    match gmail.get_message(&id).await {
        Ok(message) => ToolResult::ok(format!(
            "{} | {} | {}",
            message.id,
            message.from.unwrap_or_default(),
            message.subject.unwrap_or_default()
        )),
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
            files
                .into_iter()
                .map(|file| format!("{} | {} | {}", file.id, file.name, file.mime_type))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

async fn google_download_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let id = match get_required_string(arguments, "id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive.download_file(&id).await {
        Ok(bytes) => {
            ToolResult::ok(format!("downloaded {} bytes", bytes.len())).with_metadata("file_id", id)
        }
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
            events
                .into_iter()
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
    let resolved =
        match resolve_workspace_path(&runtime.workspace_root, &runtime.workspace_policy, &path) {
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

async fn browser_get_url(runtime: &ToolRuntime) -> ToolResult {
    with_browser(runtime, |browser| async move {
        let url = browser.get_url().await?;
        Ok(ToolResult::ok(url))
    })
    .await
}

async fn browser_eval_js(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let script = match get_required_string(arguments, "script") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    with_browser(runtime, |browser| async move {
        let value = browser.eval_js(&script).await?;
        Ok(ToolResult::ok(value.to_string()))
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
    let resolved =
        match resolve_workspace_path(&runtime.workspace_root, &runtime.workspace_policy, &path) {
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

async fn tts_list_voices(runtime: &ToolRuntime) -> ToolResult {
    let client = TtsClient::new(resolve_tts_config(&runtime.skills.tts));

    match client.list_voices().await {
        Ok(voices) if voices.is_empty() => ToolResult::ok("no voices"),
        Ok(voices) => ToolResult::ok(
            voices
                .into_iter()
                .map(|voice| format!("{} | {}", voice.voice_id, voice.name))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
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
        Ok(image) => {
            let byte_len = image.bytes.as_ref().map(|bytes| bytes.len()).unwrap_or(0);
            let mut result = ToolResult::ok(format!(
                "generated {:?} image ({}) bytes",
                image.format, byte_len
            ))
            .with_metadata("format", format!("{:?}", image.format).to_lowercase())
            .with_metadata("bytes", byte_len.to_string());
            if let Some(url) = image.url {
                result = result.with_metadata("url", url);
            }
            if let Some(revised_prompt) = image.revised_prompt {
                result = result.with_metadata("revised_prompt", revised_prompt);
            }
            result
        }
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

async fn qr_encode_url(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    let url = match get_required_string(arguments, "url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let format = match arguments.get("format").and_then(|value| value.as_str()) {
        Some("svg") => QrFormat::Svg,
        Some("terminal") => QrFormat::Terminal,
        _ => QrFormat::default(),
    };

    match QrSkill::encode_url(&url, format) {
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
        config.username = config
            .username
            .as_ref()
            .map(|value| resolve_env_reference(value));
        config.password = config
            .password
            .as_ref()
            .map(|value| resolve_env_reference(value));
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
    let mut client = match mcp_client_for_server(runtime, &server).await {
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
    if let Some(result) = require_tool_approval("mcp_call_tool", arguments, runtime).await {
        return result;
    }

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

    let mut client = match mcp_client_for_server(runtime, &server).await {
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

async fn mcp_client_for_server(runtime: &ToolRuntime, server: &str) -> Result<McpClient, String> {
    let transport_config = mcp_transport_config_for_server(runtime, server).await?;

    Ok(McpClient::new(McpClientConfig {
        name: "borgclaw".to_string(),
        transport_config,
        protocol_version: "2024-11-05".to_string(),
    }))
}

async fn mcp_transport_config_for_server(
    runtime: &ToolRuntime,
    server: &str,
) -> Result<McpTransportConfig, String> {
    let config = runtime
        .mcp_servers
        .get(server)
        .ok_or_else(|| format!("unknown mcp server '{}'", server))?;
    Ok(match config.transport.as_str() {
        "stdio" => {
            let command = config
                .command
                .clone()
                .ok_or_else(|| "missing MCP command".to_string())?;
            match runtime.security.check_command(&command) {
                CommandCheck::Blocked(pattern) => {
                    return Err(format!("blocked MCP command by policy: {}", pattern));
                }
                CommandCheck::Allowed => {}
            }
            let mut env = runtime.security.secret_env().await;
            for (key, value) in &config.env {
                env.insert(
                    key.clone(),
                    resolve_secret_or_env_reference(value, &runtime.security).await,
                );
            }
            McpTransportConfig::Stdio(StdioTransportConfig {
                command,
                args: config.args.clone(),
                env,
            })
        }
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
    })
}

async fn resolve_secret_or_env_reference(value: &str, security: &SecurityLayer) -> String {
    if let Some(var) = value.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
        if let Some(secret) = security.get_secret(var).await {
            return secret;
        }
        return std::env::var(var).unwrap_or_default();
    }
    value.to_string()
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

fn resolve_workspace_path(
    workspace_root: &Path,
    policy: &crate::config::WorkspacePolicyConfig,
    requested: &str,
) -> Result<PathBuf, String> {
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

    let mut allowed_roots = vec![workspace_root.to_path_buf()];
    if !policy.workspace_only {
        for root in &policy.allowed_roots {
            if let Ok(resolved) = resolve_policy_path(workspace_root, root) {
                allowed_roots.push(resolved);
            }
        }
    }

    if !allowed_roots
        .iter()
        .any(|root| normalized.starts_with(root))
    {
        return Err("path escapes allowed workspace roots".to_string());
    }

    for forbidden in &policy.forbidden_paths {
        let resolved = resolve_policy_path(workspace_root, forbidden)?;
        if normalized.starts_with(&resolved) {
            return Err(format!(
                "path blocked by workspace policy: {}",
                forbidden.display()
            ));
        }
    }

    Ok(normalized)
}

fn resolve_policy_path(workspace_root: &Path, path: &Path) -> Result<PathBuf, String> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };

    std::fs::canonicalize(&candidate)
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
        .map_err(|e| e.to_string())
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
    use crate::agent::{AgentContext, SenderInfo, SessionId};
    use crate::config::{
        AgentConfig, ApprovalMode, HeartbeatConfig, McpConfig, McpServerConfig, MemoryConfig,
        SchedulerConfig, SecurityConfig,
    };
    use crate::scheduler::JobStatus;

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
        let escaped = resolve_workspace_path(
            &workspace,
            &crate::config::WorkspacePolicyConfig::default(),
            "../outside.txt",
        );
        std::fs::remove_dir_all(&workspace).unwrap();
        assert!(escaped.is_err());
    }

    #[test]
    fn blocks_forbidden_workspace_path() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_tools_forbidden_test_{}",
            uuid::Uuid::new_v4()
        ));
        let forbidden_dir = workspace.join("secrets");
        std::fs::create_dir_all(&forbidden_dir).unwrap();
        std::fs::write(forbidden_dir.join("token.txt"), "secret").unwrap();

        let policy = crate::config::WorkspacePolicyConfig {
            forbidden_paths: vec![PathBuf::from("secrets")],
            ..Default::default()
        };
        let blocked = resolve_workspace_path(&workspace, &policy, "secrets/token.txt");

        std::fs::remove_dir_all(&workspace).unwrap();
        assert_eq!(
            blocked.unwrap_err(),
            "path blocked by workspace policy: secrets"
        );
    }

    #[test]
    fn allows_additional_root_when_workspace_only_disabled() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_tools_allowed_root_test_{}",
            uuid::Uuid::new_v4()
        ));
        let extra_root = workspace.join("shared-root");
        std::fs::create_dir_all(&extra_root).unwrap();
        std::fs::write(extra_root.join("shared.txt"), "shared").unwrap();

        let policy = crate::config::WorkspacePolicyConfig {
            workspace_only: false,
            allowed_roots: vec![extra_root.clone()],
            ..Default::default()
        };
        let allowed = resolve_workspace_path(
            &workspace,
            &policy,
            extra_root.join("shared.txt").to_string_lossy().as_ref(),
        );

        std::fs::remove_dir_all(&workspace).unwrap();
        assert!(allowed.is_ok());
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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

    #[tokio::test]
    async fn command_allowlist_blocks_execute_command_when_unmatched() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_command_allowlist_test_{}",
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig {
                allowed_commands: vec!["^git status$".to_string()],
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let result = execute_tool(
            &ToolCall::new(
                "execute_command",
                HashMap::from([("command".to_string(), serde_json::json!("printf nope"))]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(!result.success);
        assert_eq!(
            result.output,
            "blocked command by policy: not allowed by command allowlist"
        );
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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
    async fn supervised_plugin_invoke_requires_approval() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_plugin_approval_test_{}",
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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

        let result = execute_tool(
            &ToolCall::new(
                "plugin_invoke",
                HashMap::from([("plugin".to_string(), serde_json::json!("missing"))]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(
            result.metadata.get("approval_required").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            result.metadata.get("approval_tool").map(String::as_str),
            Some("plugin_invoke")
        );
    }

    #[tokio::test]
    async fn plugin_invoke_executes_loaded_wasm_plugin() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_plugin_runtime_test_{}",
            uuid::Uuid::new_v4()
        ));
        let skills_dir = root.join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_test_plugin(
            &skills_dir,
            "echo_plugin",
            r#"
name = "echo_plugin"
version = "1.0.0"
description = "Test plugin"
entry_point = "invoke"

[permissions]
file_read = ["."]
"#,
        );

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
            &crate::config::SkillsConfig {
                skills_path: skills_dir.clone(),
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
                HashMap::from([("plugin".to_string(), serde_json::json!("echo_plugin"))]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(root).unwrap();
        assert!(result.success);
        assert_eq!(result.output, "plugin ok");
        assert_eq!(
            result.metadata.get("plugin").map(String::as_str),
            Some("echo_plugin")
        );
        assert_eq!(
            result.metadata.get("function").map(String::as_str),
            Some("invoke")
        );
    }

    #[tokio::test]
    async fn plugin_invoke_enforces_workspace_permissions_for_loaded_plugin() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_plugin_runtime_policy_test_{}",
            uuid::Uuid::new_v4()
        ));
        let skills_dir = root.join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_test_plugin(
            &skills_dir,
            "blocked_plugin",
            r#"
name = "blocked_plugin"
version = "1.0.0"
description = "Blocked plugin"
entry_point = "invoke"

[permissions]
file_write = ["/etc"]
"#,
        );

        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
            &crate::config::SkillsConfig {
                skills_path: skills_dir.clone(),
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
                HashMap::from([("plugin".to_string(), serde_json::json!("blocked_plugin"))]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(root).unwrap();
        assert!(!result.success);
        assert!(result.output.contains("escapes allowed roots"));
    }

    #[tokio::test]
    async fn supervised_mcp_call_requires_approval() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_mcp_approval_test_{}",
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig {
                servers: HashMap::from([(
                    "demo".to_string(),
                    crate::config::McpServerConfig {
                        transport: "stdio".to_string(),
                        command: Some("echo".to_string()),
                        args: Vec::new(),
                        env: HashMap::new(),
                        url: None,
                        headers: HashMap::new(),
                    },
                )]),
            },
            &SecurityConfig {
                approval_mode: ApprovalMode::Supervised,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let result = execute_tool(
            &ToolCall::new(
                "mcp_call_tool",
                HashMap::from([
                    ("server".to_string(), serde_json::json!("demo")),
                    ("tool".to_string(), serde_json::json!("noop")),
                ]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(
            result.metadata.get("approval_required").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            result.metadata.get("approval_tool").map(String::as_str),
            Some("mcp_call_tool")
        );
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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

    #[tokio::test]
    async fn mcp_client_requires_known_server() {
        let runtime = ToolRuntime {
            workspace_root: PathBuf::from("."),
            workspace_policy: crate::config::WorkspacePolicyConfig::default(),
            memory: Arc::new(SqliteMemory::new(
                std::env::temp_dir().join("borgclaw_mcp_unknown_memory"),
            )),
            heartbeat: Arc::new(HeartbeatEngine::new()),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            heartbeat_config: HeartbeatConfig::default(),
            scheduler_config: SchedulerConfig::default(),
            plugins: Arc::new(PluginRegistry::new()),
            skills: crate::config::SkillsConfig::default(),
            mcp_servers: HashMap::new(),
            security: Arc::new(SecurityLayer::with_config(SecurityConfig::default())),
            invocation: None,
        };

        let err = match mcp_client_for_server(&runtime, "missing").await {
            Ok(_) => panic!("expected unknown server error"),
            Err(err) => err,
        };
        assert!(err.contains("unknown mcp server"));
    }

    #[tokio::test]
    async fn mcp_client_rejects_unsupported_transport() {
        let runtime = ToolRuntime {
            workspace_root: PathBuf::from("."),
            workspace_policy: crate::config::WorkspacePolicyConfig::default(),
            memory: Arc::new(SqliteMemory::new(
                std::env::temp_dir().join("borgclaw_mcp_transport_memory"),
            )),
            heartbeat: Arc::new(HeartbeatEngine::new()),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            heartbeat_config: HeartbeatConfig::default(),
            scheduler_config: SchedulerConfig::default(),
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
            invocation: None,
        };

        let err = match mcp_client_for_server(&runtime, "bad").await {
            Ok(_) => panic!("expected unsupported transport error"),
            Err(err) => err,
        };
        assert!(err.contains("unsupported MCP transport"));
    }

    #[tokio::test]
    async fn mcp_client_blocks_stdio_command_by_policy() {
        let runtime = ToolRuntime {
            workspace_root: PathBuf::from("."),
            workspace_policy: crate::config::WorkspacePolicyConfig::default(),
            memory: Arc::new(SqliteMemory::new(
                std::env::temp_dir().join("borgclaw_mcp_blocked_memory"),
            )),
            heartbeat: Arc::new(HeartbeatEngine::new()),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            heartbeat_config: HeartbeatConfig::default(),
            scheduler_config: SchedulerConfig::default(),
            plugins: Arc::new(PluginRegistry::new()),
            skills: crate::config::SkillsConfig::default(),
            mcp_servers: HashMap::from([(
                "blocked".to_string(),
                McpServerConfig {
                    transport: "stdio".to_string(),
                    command: Some("rm -rf /".to_string()),
                    ..Default::default()
                },
            )]),
            security: Arc::new(SecurityLayer::with_config(SecurityConfig::default())),
            invocation: None,
        };

        let err = match mcp_client_for_server(&runtime, "blocked").await {
            Ok(_) => panic!("expected blocked MCP command error"),
            Err(err) => err,
        };
        assert!(err.contains("blocked MCP command by policy"));
    }

    #[tokio::test]
    async fn mcp_client_respects_command_allowlist() {
        let runtime = ToolRuntime {
            workspace_root: PathBuf::from("."),
            workspace_policy: crate::config::WorkspacePolicyConfig::default(),
            memory: Arc::new(SqliteMemory::new(
                std::env::temp_dir().join("borgclaw_mcp_allowlist_memory"),
            )),
            heartbeat: Arc::new(HeartbeatEngine::new()),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            heartbeat_config: HeartbeatConfig::default(),
            scheduler_config: SchedulerConfig::default(),
            plugins: Arc::new(PluginRegistry::new()),
            skills: crate::config::SkillsConfig::default(),
            mcp_servers: HashMap::from([(
                "blocked".to_string(),
                McpServerConfig {
                    transport: "stdio".to_string(),
                    command: Some("python tool.py".to_string()),
                    ..Default::default()
                },
            )]),
            security: Arc::new(SecurityLayer::with_config(SecurityConfig {
                allowed_commands: vec!["^git status$".to_string()],
                ..Default::default()
            })),
            invocation: None,
        };

        let err = match mcp_client_for_server(&runtime, "blocked").await {
            Ok(_) => panic!("expected allowlist rejection"),
            Err(err) => err,
        };
        assert!(err.contains("not allowed by command allowlist"));
    }

    #[tokio::test]
    async fn mcp_client_stdio_env_includes_security_secrets_and_resolved_placeholders() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_mcp_env_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        let security = Arc::new(SecurityLayer::with_config(SecurityConfig::default()));
        security
            .store_secret("MCP_API_KEY", "secret-value")
            .await
            .unwrap();

        let runtime = ToolRuntime {
            workspace_root: PathBuf::from("."),
            workspace_policy: crate::config::WorkspacePolicyConfig::default(),
            memory: Arc::new(SqliteMemory::new(root.join("memory"))),
            heartbeat: Arc::new(HeartbeatEngine::new()),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            heartbeat_config: HeartbeatConfig::default(),
            scheduler_config: SchedulerConfig::default(),
            plugins: Arc::new(PluginRegistry::new()),
            skills: crate::config::SkillsConfig::default(),
            mcp_servers: HashMap::from([(
                "server".to_string(),
                McpServerConfig {
                    transport: "stdio".to_string(),
                    command: Some("echo".to_string()),
                    env: HashMap::from([("API_KEY".to_string(), "${MCP_API_KEY}".to_string())]),
                    ..Default::default()
                },
            )]),
            security,
            invocation: None,
        };

        let transport = match mcp_transport_config_for_server(&runtime, "server")
            .await
            .unwrap()
        {
            McpTransportConfig::Stdio(config) => config,
            _ => panic!("expected stdio transport"),
        };

        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            transport.env.get("API_KEY").map(String::as_str),
            Some("secret-value")
        );
        assert_eq!(
            transport
                .env
                .get("BC_SECRET_MCP_API_KEY")
                .map(String::as_str),
            Some("secret-value")
        );
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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

    #[tokio::test]
    async fn runtime_starts_scheduler_loop_when_enabled() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_scheduler_runtime_test_{}",
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        {
            let scheduler = runtime.scheduler.lock().await;
            assert!(scheduler.is_running().await);
            assert!(scheduler.stop().await);
        }

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn runtime_leaves_scheduler_stopped_when_disabled() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_scheduler_disabled_runtime_test_{}",
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
            &HeartbeatConfig::default(),
            &SchedulerConfig {
                enabled: false,
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

        {
            let scheduler = runtime.scheduler.lock().await;
            assert!(!scheduler.is_running().await);
        }

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn runtime_starts_heartbeat_loop_when_enabled() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_heartbeat_runtime_test_{}",
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
            &HeartbeatConfig::default(),
            &SchedulerConfig {
                enabled: false,
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

        assert!(runtime.heartbeat.is_running().await);
        assert!(runtime.heartbeat.stop().await.is_ok());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn runtime_leaves_heartbeat_stopped_when_disabled() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_heartbeat_disabled_runtime_test_{}",
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
            &HeartbeatConfig {
                enabled: false,
                ..Default::default()
            },
            &SchedulerConfig {
                enabled: false,
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

        assert!(!runtime.heartbeat.is_running().await);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn builtin_tools_include_documented_github_issue_release_and_file_ops() {
        let names = builtin_tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name == "github_get_repo"));
        assert!(names.iter().any(|name| name == "github_list_branches"));
        assert!(names.iter().any(|name| name == "github_create_branch"));
        assert!(names.iter().any(|name| name == "github_list_prs"));
        assert!(names.iter().any(|name| name == "github_list_issues"));
        assert!(names.iter().any(|name| name == "github_create_issue"));
        assert!(names.iter().any(|name| name == "github_list_releases"));
        assert!(names.iter().any(|name| name == "github_get_file"));
        assert!(names.iter().any(|name| name == "github_create_file"));
        assert!(names.iter().any(|name| name == "google_get_message"));
        assert!(names.iter().any(|name| name == "google_download_file"));
        assert!(names.iter().any(|name| name == "browser_get_url"));
        assert!(names.iter().any(|name| name == "browser_eval_js"));
        assert!(names.iter().any(|name| name == "tts_list_voices"));
        assert!(names.iter().any(|name| name == "qr_encode_url"));
    }

    #[tokio::test]
    async fn schedule_task_defaults_to_future_one_shot() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_schedule_task_test_{}",
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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
                "schedule_task",
                HashMap::from([("message".to_string(), serde_json::json!("scheduled task"))]),
            ),
            &runtime,
        )
        .await;

        let jobs = runtime.scheduler.lock().await.list().await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(jobs.len(), 1);
        assert!(matches!(jobs[0].trigger, JobTrigger::OneShot(_)));
        assert!(jobs[0].next_run.is_some());
        assert_eq!(
            jobs[0].metadata.get("action_tool").map(String::as_str),
            Some("message")
        );
    }

    #[tokio::test]
    async fn memory_tools_inherit_group_context() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_memory_group_context_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let scheduler = SchedulerConfig {
            enabled: false,
            ..Default::default()
        };
        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &HeartbeatConfig::default(),
            &scheduler,
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap()
        .with_context(&AgentContext {
            session_id: SessionId("session-a".to_string()),
            message: "remember this".to_string(),
            sender: SenderInfo {
                id: "user-a".to_string(),
                name: Some("User A".to_string()),
                channel: "cli".to_string(),
            },
            metadata: HashMap::from([("group_id".to_string(), "group-a".to_string())]),
        });

        let store = execute_tool(
            &ToolCall::new(
                "memory_store",
                HashMap::from([
                    ("key".to_string(), serde_json::json!("groupkey")),
                    ("value".to_string(), serde_json::json!("group value")),
                ]),
            ),
            &runtime,
        )
        .await;
        let recall = execute_tool(
            &ToolCall::new(
                "memory_recall",
                HashMap::from([("query".to_string(), serde_json::json!("groupkey"))]),
            ),
            &runtime,
        )
        .await;
        let global = runtime
            .memory
            .recall(&MemoryQuery {
                query: "groupkey".to_string(),
                limit: 5,
                min_score: 0.0,
                group_id: None,
            })
            .await
            .unwrap();
        let grouped = runtime
            .memory
            .recall(&MemoryQuery {
                query: "groupkey".to_string(),
                limit: 5,
                min_score: 0.0,
                group_id: Some("group-a".to_string()),
            })
            .await
            .unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert!(store.success);
        assert_eq!(
            store.metadata.get("group_id").map(String::as_str),
            Some("group-a")
        );
        assert!(recall.success);
        assert_eq!(
            recall.metadata.get("group_id").map(String::as_str),
            Some("group-a")
        );
        assert!(recall.output.contains("groupkey: group value"));
        assert!(global.is_empty());
        assert_eq!(grouped.len(), 1);
    }

    #[tokio::test]
    async fn run_scheduled_tasks_executes_due_scheduled_messages() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_run_scheduled_task_test_{}",
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        let schedule = execute_tool(
            &ToolCall::new(
                "schedule_task",
                HashMap::from([("message".to_string(), serde_json::json!("scheduled task"))]),
            ),
            &runtime,
        )
        .await;
        assert!(schedule.success);

        {
            let scheduler = runtime.scheduler.lock().await;
            let jobs = scheduler.list().await;
            let id = jobs[0].id.clone();
            drop(jobs);
            let mut stored = scheduler.get(&id).await.unwrap();
            stored.next_run = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
            scheduler.unschedule(&id).await.unwrap();
            scheduler.schedule(stored).await.unwrap();
        }

        let result = execute_tool(
            &ToolCall::new("run_scheduled_tasks", HashMap::new()),
            &runtime,
        )
        .await;
        let jobs = runtime.scheduler.lock().await.list().await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(
            result.metadata.get("executed").map(String::as_str),
            Some("1")
        );
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, JobStatus::Completed);
        assert_eq!(jobs[0].run_count, 1);
    }

    #[tokio::test]
    async fn run_scheduled_tasks_executes_due_scheduled_tool_calls() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_run_scheduled_tool_test_{}",
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        let schedule = execute_tool(
            &ToolCall::new(
                "schedule_task",
                HashMap::from([
                    ("tool".to_string(), serde_json::json!("memory_store")),
                    (
                        "arguments".to_string(),
                        serde_json::json!({
                            "key": "scheduledkey",
                            "value": "scheduled content"
                        }),
                    ),
                ]),
            ),
            &runtime,
        )
        .await;
        assert!(schedule.success);

        {
            let scheduler = runtime.scheduler.lock().await;
            let jobs = scheduler.list().await;
            let id = jobs[0].id.clone();
            drop(jobs);
            let mut stored = scheduler.get(&id).await.unwrap();
            stored.next_run = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
            scheduler.unschedule(&id).await.unwrap();
            scheduler.schedule(stored).await.unwrap();
        }

        let result = execute_tool(
            &ToolCall::new("run_scheduled_tasks", HashMap::new()),
            &runtime,
        )
        .await;
        let jobs = runtime.scheduler.lock().await.list().await;
        let recalled = runtime
            .memory
            .recall(&MemoryQuery {
                query: "scheduledkey".to_string(),
                limit: 5,
                min_score: 0.0,
                group_id: None,
            })
            .await
            .unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(
            result.metadata.get("executed").map(String::as_str),
            Some("1")
        );
        assert_eq!(jobs[0].status, JobStatus::Completed);
        assert!(recalled
            .iter()
            .any(|entry| entry.entry.key == "scheduledkey"
                && entry.entry.content == "scheduled content"));
    }

    #[tokio::test]
    async fn scheduled_tool_calls_inherit_group_context() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_scheduled_group_context_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let scheduler = SchedulerConfig {
            enabled: false,
            ..Default::default()
        };
        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &HeartbeatConfig::default(),
            &scheduler,
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap()
        .with_context(&AgentContext {
            session_id: SessionId("session-scheduled".to_string()),
            message: "schedule this".to_string(),
            sender: SenderInfo {
                id: "user-scheduled".to_string(),
                name: Some("Scheduled User".to_string()),
                channel: "telegram".to_string(),
            },
            metadata: HashMap::from([("group_id".to_string(), "group-scheduled".to_string())]),
        });

        let schedule = execute_tool(
            &ToolCall::new(
                "schedule_task",
                HashMap::from([
                    ("tool".to_string(), serde_json::json!("memory_store")),
                    (
                        "arguments".to_string(),
                        serde_json::json!({
                            "key": "scheduledgroupkey",
                            "value": "scheduled group content"
                        }),
                    ),
                ]),
            ),
            &runtime,
        )
        .await;
        assert!(schedule.success);

        {
            let scheduler = runtime.scheduler.lock().await;
            let jobs = scheduler.list().await;
            let id = jobs[0].id.clone();
            drop(jobs);
            let mut stored = scheduler.get(&id).await.unwrap();
            assert_eq!(
                stored
                    .metadata
                    .get("scheduled_meta_group_id")
                    .map(String::as_str),
                Some("group-scheduled")
            );
            stored.next_run = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
            scheduler.unschedule(&id).await.unwrap();
            scheduler.schedule(stored).await.unwrap();
        }

        let result = execute_tool(
            &ToolCall::new("run_scheduled_tasks", HashMap::new()),
            &runtime,
        )
        .await;
        let grouped = runtime
            .memory
            .recall(&MemoryQuery {
                query: "scheduledgroupkey".to_string(),
                limit: 5,
                min_score: 0.0,
                group_id: Some("group-scheduled".to_string()),
            })
            .await
            .unwrap();
        let global = runtime
            .memory
            .recall(&MemoryQuery {
                query: "scheduledgroupkey".to_string(),
                limit: 5,
                min_score: 0.0,
                group_id: None,
            })
            .await
            .unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(grouped.len(), 1);
        assert!(global.is_empty());
    }

    #[tokio::test]
    async fn scheduled_tool_calls_inherit_workspace_policy() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_scheduled_workspace_policy_test_{}",
            uuid::Uuid::new_v4()
        ));
        let blocked_dir = root.join("blocked");
        std::fs::create_dir_all(&blocked_dir).unwrap();
        std::fs::write(blocked_dir.join("secret.txt"), "top secret").unwrap();

        let scheduler = SchedulerConfig {
            enabled: false,
            ..Default::default()
        };
        let runtime = ToolRuntime::from_config(
            &AgentConfig {
                workspace: root.clone(),
                ..Default::default()
            },
            &MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            &HeartbeatConfig::default(),
            &scheduler,
            &crate::config::SkillsConfig {
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig {
                workspace: crate::config::WorkspacePolicyConfig {
                    forbidden_paths: vec![PathBuf::from("blocked")],
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let schedule = execute_tool(
            &ToolCall::new(
                "schedule_task",
                HashMap::from([
                    ("tool".to_string(), serde_json::json!("read_file")),
                    (
                        "arguments".to_string(),
                        serde_json::json!({
                            "path": "blocked/secret.txt"
                        }),
                    ),
                ]),
            ),
            &runtime,
        )
        .await;
        assert!(schedule.success);

        {
            let scheduler = runtime.scheduler.lock().await;
            let jobs = scheduler.list().await;
            let id = jobs[0].id.clone();
            drop(jobs);
            let mut stored = scheduler.get(&id).await.unwrap();
            stored.next_run = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
            scheduler.unschedule(&id).await.unwrap();
            scheduler.schedule(stored).await.unwrap();
        }

        let result = execute_tool(
            &ToolCall::new("run_scheduled_tasks", HashMap::new()),
            &runtime,
        )
        .await;
        let jobs = runtime.scheduler.lock().await.list().await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(jobs[0].status, JobStatus::Failed);
        assert!(jobs[0]
            .run_history
            .last()
            .and_then(|run| run.error.as_deref())
            .unwrap_or_default()
            .contains("path blocked by workspace policy: blocked"));
    }

    #[tokio::test]
    async fn run_scheduled_tasks_rejects_background_tools_that_need_approval() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_run_scheduled_approval_test_{}",
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
            &HeartbeatConfig::default(),
            &SchedulerConfig::default(),
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

        let schedule = execute_tool(
            &ToolCall::new(
                "schedule_task",
                HashMap::from([
                    ("tool".to_string(), serde_json::json!("execute_command")),
                    (
                        "arguments".to_string(),
                        serde_json::json!({
                            "command": "echo should-not-run"
                        }),
                    ),
                ]),
            ),
            &runtime,
        )
        .await;
        assert!(schedule.success);

        {
            let scheduler = runtime.scheduler.lock().await;
            let jobs = scheduler.list().await;
            let id = jobs[0].id.clone();
            drop(jobs);
            let mut stored = scheduler.get(&id).await.unwrap();
            stored.next_run = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
            scheduler.unschedule(&id).await.unwrap();
            scheduler.schedule(stored).await.unwrap();
        }

        let result = execute_tool(
            &ToolCall::new("run_scheduled_tasks", HashMap::new()),
            &runtime,
        )
        .await;
        let jobs = runtime.scheduler.lock().await.list().await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(result.metadata.get("failed").map(String::as_str), Some("1"));
        assert_eq!(jobs[0].status, JobStatus::Failed);
    }

    fn write_test_plugin(skills_dir: &std::path::Path, name: &str, manifest: &str) {
        let wasm = wat::parse_str(
            r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 16) "plugin ok\00")
              (func (export "alloc") (param i32) (result i32)
                (i32.const 0))
              (func (export "invoke") (param i32 i32) (result i32)
                (i32.const 16)))
        "#,
        )
        .unwrap();

        std::fs::write(skills_dir.join(format!("{name}.wasm")), wasm).unwrap();
        std::fs::write(skills_dir.join(format!("{name}.toml")), manifest.trim()).unwrap();
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
            .with_approval(true)
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
        Tool::new("github_get_repo", "Get GitHub repository details")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_list_branches", "List GitHub repository branches")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_create_branch", "Create a GitHub branch")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("branch".to_string(), string_property("Branch name")),
                    ("from_sha".to_string(), string_property("Source commit SHA")),
                ]
                .into(),
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "branch".to_string(),
                    "from_sha".to_string(),
                ],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_list_prs", "List GitHub pull requests")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    (
                        "state".to_string(),
                        string_property("Optional pull request state filter"),
                    ),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string()],
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
            vec![
                "owner".to_string(),
                "repo".to_string(),
                "branch".to_string(),
            ],
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
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "branch".to_string(),
                ],
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
            vec![
                "owner".to_string(),
                "repo".to_string(),
                "number".to_string(),
            ],
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
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "number".to_string(),
                ],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_list_issues", "List GitHub issues")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    (
                        "state".to_string(),
                        string_property("Optional issue state filter"),
                    ),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_create_issue", "Create a GitHub issue")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("title".to_string(), string_property("Issue title")),
                    ("body".to_string(), string_property("Issue body")),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string(), "title".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_list_releases", "List GitHub releases")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_get_file", "Read a file from a GitHub repository")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("path".to_string(), string_property("Repository file path")),
                    ("ref".to_string(), string_property("Git ref or branch name")),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string(), "path".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_create_file", "Create a file in a GitHub repository")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("path".to_string(), string_property("Repository file path")),
                    ("content".to_string(), string_property("File content")),
                    ("message".to_string(), string_property("Commit message")),
                    ("branch".to_string(), string_property("Target branch")),
                ]
                .into(),
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "path".to_string(),
                    "content".to_string(),
                    "message".to_string(),
                ],
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
        Tool::new("google_get_message", "Get a Gmail message by id")
            .with_schema(ToolSchema::object(
                [("id".to_string(), string_property("Gmail message id"))].into(),
                vec!["id".to_string()],
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
        Tool::new("google_download_file", "Download a Google Drive file")
            .with_schema(ToolSchema::object(
                [("id".to_string(), string_property("Drive file id"))].into(),
                vec!["id".to_string()],
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
                    (
                        "folder_id".to_string(),
                        string_property("Optional Drive folder id"),
                    ),
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
                    ("start".to_string(), string_property("RFC3339 start time")),
                    ("end".to_string(), string_property("RFC3339 end time")),
                ]
                .into(),
                vec![
                    "summary".to_string(),
                    "start".to_string(),
                    "end".to_string(),
                ],
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
        Tool::new(
            "browser_wait_for",
            "Wait for a selector or text on the current page",
        )
        .with_schema(ToolSchema::object(
            [
                (
                    "selector".to_string(),
                    string_property("Optional CSS selector"),
                ),
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
        Tool::new("browser_get_url", "Get the current browser URL")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_eval_js", "Evaluate JavaScript in the current page")
            .with_schema(ToolSchema::object(
                [("script".to_string(), string_property("JavaScript source"))].into(),
                vec!["script".to_string()],
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new(
            "browser_screenshot",
            "Capture a screenshot from the current page",
        )
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
        Tool::new("tts_list_voices", "List available TTS voices")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["tts".to_string(), "integration".to_string()]),
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
                    (
                        "format".to_string(),
                        string_property("png, svg, or terminal"),
                    ),
                ]
                .into(),
                vec!["data".to_string()],
            ))
            .with_tags(vec!["qr".to_string(), "integration".to_string()]),
        Tool::new("qr_encode_url", "Generate a QR code from a URL")
            .with_schema(ToolSchema::object(
                [
                    ("url".to_string(), string_property("URL to encode")),
                    (
                        "format".to_string(),
                        string_property("png, svg, or terminal"),
                    ),
                ]
                .into(),
                vec!["url".to_string()],
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
        .with_approval(true)
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
                        "tool".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Built-in tool name to execute later".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "arguments".to_string(),
                        PropertySchema {
                            prop_type: "object".to_string(),
                            description: Some("Arguments for the scheduled tool call".to_string()),
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
                Vec::new(),
            ))
            .with_tags(vec!["scheduling".to_string()]),
        Tool::new("run_scheduled_tasks", "Execute due scheduled tasks")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
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
