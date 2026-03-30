//! Tools module - defines tools agents can use

mod browser;
mod file;
mod github;
mod google;
mod mcp;
mod media;
mod memory;
mod plugin;
mod schedule;
mod shell;
mod types;
mod web;

pub use types::*;

use crate::memory::{create_memory_backend, HeartbeatEngine, Memory};
use crate::scheduler::Scheduler;
use crate::security::SecurityLayer;
use crate::skills::{
    BrowserSkill, CdpClient, GitHubClient, GoogleClient, PlaywrightClient, PluginRegistry,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ToolRuntime {
    pub workspace_root: PathBuf,
    pub workspace_policy: crate::config::WorkspacePolicyConfig,
    pub memory_config: crate::config::MemoryConfig,
    pub memory: Arc<dyn Memory>,
    pub heartbeat: Arc<HeartbeatEngine>,
    pub scheduler: Arc<Mutex<Scheduler>>,
    pub heartbeat_config: crate::config::HeartbeatConfig,
    pub scheduler_config: crate::config::SchedulerConfig,
    pub plugins: Arc<PluginRegistry>,
    pub skills: crate::config::SkillsConfig,
    pub mcp_servers: HashMap<String, crate::config::McpServerConfig>,
    pub security: Arc<SecurityLayer>,
    pub audit: Arc<crate::security::AuditLogger>,
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
        let memory = create_memory_backend(memory_config)
            .await
            .map_err(|e| e.to_string())?;
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
            memory_config: memory_config.clone(),
            memory,
            heartbeat,
            scheduler: Arc::new(Mutex::new(
                Scheduler::new().with_state_path(agent.workspace.join("scheduler.json")),
            )),
            heartbeat_config: heartbeat_config.clone(),
            scheduler_config: scheduler_config.clone(),
            plugins,
            skills: skills_config.clone(),
            mcp_servers: mcp_config.servers.clone(),
            security: Arc::new(SecurityLayer::with_config(security_config.clone())),
            audit: Arc::new(crate::security::AuditLogger::disabled()),
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
                        Box::pin(
                            async move { schedule::execute_scheduled_job(&job, &runtime).await },
                        )
                    }
                }),
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn shutdown(&self) {
        let _ = self.heartbeat.stop().await;
        let scheduler = self.scheduler.lock().await;
        let _ = scheduler.stop().await;
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
        "memory_store" => memory::memory_store(&call.arguments, runtime).await,
        "memory_store_procedural" => {
            memory::memory_store_procedural(&call.arguments, runtime).await
        }
        "memory_recall" => memory::memory_recall(&call.arguments, runtime).await,
        "memory_history" => memory::memory_history(&call.arguments, runtime).await,
        "memory_delete" => memory::memory_delete(&call.arguments, runtime).await,
        "memory_keys" => memory::memory_keys(runtime).await,
        "memory_groups" => memory::memory_groups(runtime).await,
        "memory_clear_group" => memory::memory_clear_group(&call.arguments, runtime).await,
        "solution_store" => memory::solution_store(&call.arguments, runtime).await,
        "solution_find" => memory::solution_find(&call.arguments, runtime).await,
        "execute_command" => shell::execute_command(&call.arguments, runtime).await,
        "read_file" => file::read_file(&call.arguments, runtime).await,
        "list_directory" => file::list_directory(&call.arguments, runtime).await,
        "fetch_url" => web::fetch_url(&call.arguments, runtime).await,
        "message" => message(&call.arguments),
        "schedule_task" => schedule::schedule_task(&call.arguments, runtime).await,
        "run_scheduled_tasks" => schedule::run_scheduled_tasks(runtime).await,
        "approve" => schedule::approve_tool(&call.arguments, runtime).await,
        "web_search" => web::web_search(&call.arguments).await,
        "plugin_list" => plugin::plugin_list(runtime).await,
        "plugin_invoke" => plugin::plugin_invoke(&call.arguments, runtime).await,
        "github_list_repos" => github::github_list_repos(&call.arguments, runtime).await,
        "github_get_repo" => github::github_get_repo(&call.arguments, runtime).await,
        "github_list_branches" => github::github_list_branches(&call.arguments, runtime).await,
        "github_create_branch" => github::github_create_branch(&call.arguments, runtime).await,
        "github_list_prs" => github::github_list_prs(&call.arguments, runtime).await,
        "github_create_pr" => github::github_create_pr(&call.arguments, runtime).await,
        "github_prepare_delete_branch" => {
            github::github_prepare_delete_branch(&call.arguments, runtime).await
        }
        "github_delete_branch" => github::github_delete_branch(&call.arguments, runtime).await,
        "github_prepare_merge_pr" => {
            github::github_prepare_merge_pr(&call.arguments, runtime).await
        }
        "github_merge_pr" => github::github_merge_pr(&call.arguments, runtime).await,
        "github_list_issues" => github::github_list_issues(&call.arguments, runtime).await,
        "github_create_issue" => github::github_create_issue(&call.arguments, runtime).await,
        "github_list_releases" => github::github_list_releases(&call.arguments, runtime).await,
        "github_get_file" => github::github_get_file(&call.arguments, runtime).await,
        "github_create_file" => github::github_create_file(&call.arguments, runtime).await,
        "github_update_file" => github::github_update_file(&call.arguments, runtime).await,
        "github_delete_file" => github::github_delete_file(&call.arguments, runtime).await,
        "github_close_issue" => github::github_close_issue(&call.arguments, runtime).await,
        "google_list_messages" => google::google_list_messages(&call.arguments, runtime).await,
        "google_get_message" => google::google_get_message(&call.arguments, runtime).await,
        "google_send_email" => google::google_send_email(&call.arguments, runtime).await,
        "google_search_files" => google::google_search_files(&call.arguments, runtime).await,
        "google_download_file" => google::google_download_file(&call.arguments, runtime).await,
        "google_list_events" => google::google_list_events(&call.arguments, runtime).await,
        "google_upload_file" => google::google_upload_file(&call.arguments, runtime).await,
        "google_create_event" => google::google_create_event(&call.arguments, runtime).await,
        "google_delete_email" => google::google_delete_email(&call.arguments, runtime).await,
        "google_trash_email" => google::google_trash_email(&call.arguments, runtime).await,
        "google_update_event" => google::google_update_event(&call.arguments, runtime).await,
        "google_delete_event" => google::google_delete_event(&call.arguments, runtime).await,
        "google_create_folder" => google::google_create_folder(&call.arguments, runtime).await,
        "google_list_folders" => google::google_list_folders(&call.arguments, runtime).await,
        "google_share_file" => google::google_share_file(&call.arguments, runtime).await,
        "google_list_permissions" => {
            google::google_list_permissions(&call.arguments, runtime).await
        }
        "google_remove_permission" => {
            google::google_remove_permission(&call.arguments, runtime).await
        }
        "google_move_file" => google::google_move_file(&call.arguments, runtime).await,
        "google_copy_file" => google::google_copy_file(&call.arguments, runtime).await,
        "google_delete_file" => google::google_delete_file(&call.arguments, runtime).await,
        "google_get_file_details" => {
            google::google_get_file_details(&call.arguments, runtime).await
        }
        "browser_navigate" => browser::browser_navigate(&call.arguments, runtime).await,
        "browser_click" => browser::browser_click(&call.arguments, runtime).await,
        "browser_fill" => browser::browser_fill(&call.arguments, runtime).await,
        "browser_wait_for" => browser::browser_wait_for(&call.arguments, runtime).await,
        "browser_get_text" => browser::browser_get_text(&call.arguments, runtime).await,
        "browser_get_html" => browser::browser_get_html(&call.arguments, runtime).await,
        "browser_get_url" => browser::browser_get_url(runtime).await,
        "browser_eval_js" => browser::browser_eval_js(&call.arguments, runtime).await,
        "browser_screenshot" => browser::browser_screenshot(&call.arguments, runtime).await,
        "browser_go_back" => browser::browser_go_back(runtime).await,
        "browser_go_forward" => browser::browser_go_forward(runtime).await,
        "browser_reload" => browser::browser_reload(runtime).await,
        "stt_transcribe" => media::stt_transcribe(&call.arguments, runtime).await,
        "stt_transcribe_url" => media::stt_transcribe_url(&call.arguments, runtime).await,
        "tts_list_voices" => media::tts_list_voices(runtime).await,
        "tts_speak_stream" => media::tts_speak_stream(&call.arguments, runtime).await,
        "tts_speak" => media::tts_speak(&call.arguments, runtime).await,
        "image_generate" => media::image_generate(&call.arguments, runtime).await,
        "image_analyze" => media::image_analyze(&call.arguments, runtime).await,
        "image_analyze_file" => media::image_analyze_file(&call.arguments, runtime).await,
        "qr_encode" => media::qr_encode(&call.arguments).await,
        "qr_encode_url" => media::qr_encode_url(&call.arguments).await,
        "url_shorten" => media::url_shorten(&call.arguments, runtime).await,
        "url_expand" => media::url_expand(&call.arguments, runtime).await,
        "mcp_list_tools" => mcp::mcp_list_tools(&call.arguments, runtime).await,
        "mcp_call_tool" => mcp::mcp_call_tool(&call.arguments, runtime).await,
        other => ToolResult::err(format!("unknown tool: {}", other)),
    };

    // Log tool execution to audit log
    let actor = runtime
        .invocation
        .as_ref()
        .map(|i| i.sender.id.clone())
        .unwrap_or_else(|| "system".to_string());
    runtime
        .audit
        .log_tool_execution(&actor, &call.name, result.success, Some(&result.output))
        .await;

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

pub(super) async fn require_tool_approval(
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

pub(super) fn message(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    match get_required_string(arguments, "text") {
        Ok(text) => ToolResult::ok(text),
        Err(err) => ToolResult::err(err),
    }
}

pub(super) fn optional_group_id(
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

fn scheduled_metadata(job: &crate::scheduler::Job) -> HashMap<String, String> {
    schedule::scheduled_metadata(job)
}

pub(super) fn github_client(runtime: &ToolRuntime) -> Result<GitHubClient, String> {
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

pub(super) fn google_client(runtime: &ToolRuntime) -> GoogleClient {
    let mut config = runtime.skills.google.clone();
    config.client_id = resolve_env_reference(&config.client_id);
    config.client_secret = resolve_env_reference(&config.client_secret);
    GoogleClient::new(config)
}

pub(super) async fn with_browser<F, Fut>(runtime: &ToolRuntime, f: F) -> ToolResult
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

pub(super) fn audio_format(value: Option<&str>) -> Result<crate::skills::AudioFormat, String> {
    match value.unwrap_or("wav") {
        "wav" => Ok(crate::skills::AudioFormat::Wav),
        "mp3" => Ok(crate::skills::AudioFormat::Mp3),
        "webm" => Ok(crate::skills::AudioFormat::Webm),
        "m4a" => Ok(crate::skills::AudioFormat::M4a),
        "ogg" => Ok(crate::skills::AudioFormat::Ogg),
        other => Err(format!("unsupported audio format '{}'", other)),
    }
}

pub(super) fn resolve_tts_config(
    config: &crate::config::TtsSkillConfig,
) -> crate::skills::ElevenLabsConfig {
    let mut resolved = config.elevenlabs.clone();
    resolved.api_key = resolve_env_reference(&resolved.api_key);
    resolved
}

pub(super) fn resolve_url_shortener_provider(
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

pub(super) fn resolve_env_reference(value: &str) -> String {
    if let Some(var) = value.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
        std::env::var(var).unwrap_or_default()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
async fn mcp_client_for_server(
    runtime: &ToolRuntime,
    server: &str,
) -> Result<crate::mcp::client::McpClient, String> {
    mcp::mcp_client_for_server(runtime, server).await
}

#[cfg(test)]
async fn mcp_transport_config_for_server(
    runtime: &ToolRuntime,
    server: &str,
) -> Result<crate::mcp::transport::McpTransportConfig, String> {
    mcp::mcp_transport_config_for_server(runtime, server).await
}

pub(super) fn get_required_string(
    arguments: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing string argument '{}'", key))
}

pub(super) fn get_u64(arguments: &HashMap<String, serde_json::Value>, key: &str) -> Option<u64> {
    arguments.get(key).and_then(|value| value.as_u64())
}

pub(super) fn get_string(
    arguments: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

pub(super) fn get_bool(arguments: &HashMap<String, serde_json::Value>, key: &str) -> Option<bool> {
    arguments.get(key).and_then(|value| value.as_bool())
}

pub(super) fn resolve_workspace_path(
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

pub(super) fn truncate_output(output: &str) -> String {
    const MAX_LEN: usize = 4000;
    if output.len() <= MAX_LEN {
        output.to_string()
    } else {
        format!("{}...", &output[..MAX_LEN])
    }
}

pub(super) fn string_property(description: &str) -> PropertySchema {
    PropertySchema {
        prop_type: "string".to_string(),
        description: Some(description.to_string()),
        default: None,
        enum_values: None,
    }
}

pub(super) fn number_property(description: &str, default: serde_json::Value) -> PropertySchema {
    PropertySchema {
        prop_type: "number".to_string(),
        description: Some(description.to_string()),
        default: Some(default),
        enum_values: None,
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::agent::{AgentContext, SenderInfo, SessionId};
    use crate::config::{
        AgentConfig, ApprovalMode, HeartbeatConfig, McpConfig, McpServerConfig, MemoryConfig,
        SchedulerConfig, SecurityConfig,
    };
    use crate::constants::DEFAULT_TOOL_VERSION;
    use crate::mcp::transport::McpTransportConfig;
    use crate::memory::{new_entry, MemoryQuery, MemorySensitivity, SqliteMemory};
    use crate::scheduler::{JobStatus, JobTrigger, SchedulerTrait};
    fn write_fixture(path: &std::path::Path, body: impl AsRef<[u8]>) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

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
            &format!(
                r#"
name = "echo_plugin"
version = "{}"
description = "Test plugin"
entry_point = "invoke"

[permissions]
file_read = ["."]
"#,
                DEFAULT_TOOL_VERSION
            ),
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
            &format!(
                r#"
name = "blocked_plugin"
version = "{}"
description = "Blocked plugin"
entry_point = "invoke"

[permissions]
file_write = ["/etc"]
"#,
                DEFAULT_TOOL_VERSION
            ),
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
    async fn qr_encode_url_generates_bytes() {
        let result = execute_tool(
            &ToolCall::new(
                "qr_encode_url",
                HashMap::from([(
                    "url".to_string(),
                    serde_json::json!("https://example.com/docs"),
                )]),
            ),
            &test_runtime().await,
        )
        .await;

        assert!(result.success);
        assert!(result.output.contains("generated"));
        assert!(
            result
                .metadata
                .get("bytes")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or_default()
                > 0
        );
    }

    #[tokio::test]
    async fn url_shorten_uses_configured_yourls_provider() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_url_shorten_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let fixture = root.join("yourls-response.json");
        std::fs::write(&fixture, r#"{"shorturl":"https://sho.rt/abc"}"#).unwrap();

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
                url_shortener: crate::config::UrlShortenerSkillConfig {
                    provider: "yourls".to_string(),
                    yourls: crate::config::DocumentedYourlsConfig {
                        base_url: format!("file://{}", fixture.display()),
                        signature: "testsig".to_string(),
                        username: String::new(),
                        password: String::new(),
                    },
                },
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        let result = execute_tool(
            &ToolCall::new(
                "url_shorten",
                HashMap::from([(
                    "url".to_string(),
                    serde_json::json!("https://example.com/very/long/path"),
                )]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(root).unwrap();
        assert!(result.success);
        assert_eq!(result.output, "https://sho.rt/abc");
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

        let security = SecurityConfig {
            leak_action: crate::config::LeakAction::Warn,
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

        let security = SecurityConfig {
            leak_action: crate::config::LeakAction::Block,
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
            memory_config: crate::config::MemoryConfig::default(),
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
            audit: Arc::new(crate::security::AuditLogger::disabled()),
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
            memory_config: crate::config::MemoryConfig::default(),
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
            audit: Arc::new(crate::security::AuditLogger::disabled()),
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
            memory_config: crate::config::MemoryConfig::default(),
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
            audit: Arc::new(crate::security::AuditLogger::disabled()),
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
            memory_config: crate::config::MemoryConfig::default(),
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
            audit: Arc::new(crate::security::AuditLogger::disabled()),
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
            memory_config: crate::config::MemoryConfig::default(),
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
            audit: Arc::new(crate::security::AuditLogger::disabled()),
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
                ..Default::default()
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
                ..Default::default()
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
    async fn subagent_memory_recall_hides_private_entries() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_memory_privacy_subagent_test_{}",
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
        .unwrap()
        .with_context(&AgentContext {
            session_id: SessionId("subagent-session".to_string()),
            message: "recall".to_string(),
            sender: SenderInfo {
                id: "subagent-1".to_string(),
                name: Some("SubAgent".to_string()),
                channel: "subagent".to_string(),
            },
            metadata: HashMap::new(),
        });

        let mut private_entry = new_entry("deploy", "private deploy secret");
        private_entry.set_sensitivity(MemorySensitivity::Private);
        runtime.memory.store(private_entry).await.unwrap();

        let workspace_entry = new_entry("deploy", "workspace deploy note");
        runtime.memory.store(workspace_entry).await.unwrap();

        let recall = execute_tool(
            &ToolCall::new(
                "memory_recall",
                HashMap::from([("query".to_string(), serde_json::json!("deploy"))]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(recall.success);
        assert!(recall.output.contains("workspace deploy note"));
        assert!(!recall.output.contains("private deploy secret"));
    }

    #[tokio::test]
    async fn scheduler_memory_recall_hides_private_entries() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_memory_privacy_scheduler_test_{}",
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

        let mut private_entry = new_entry("deploy", "private scheduler secret");
        private_entry.set_sensitivity(MemorySensitivity::Private);
        runtime.memory.store(private_entry).await.unwrap();

        let workspace_entry = new_entry("deploy", "workspace scheduler note");
        runtime.memory.store(workspace_entry).await.unwrap();

        let job = crate::scheduler::new_job(
            "scheduled recall".to_string(),
            crate::scheduler::JobTrigger::OneShot(chrono::Utc::now()),
            "memory_recall".to_string(),
        );
        let scheduled_runtime = runtime.with_scheduled_job_context(&job);
        let recall = execute_tool(
            &ToolCall::new(
                "memory_recall",
                HashMap::from([("query".to_string(), serde_json::json!("deploy"))]),
            ),
            &scheduled_runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(recall.success);
        assert!(recall.output.contains("workspace scheduler note"));
        assert!(!recall.output.contains("private scheduler secret"));
    }

    #[tokio::test]
    async fn heartbeat_memory_recall_hides_private_entries() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_memory_privacy_heartbeat_test_{}",
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

        let mut private_entry = new_entry("deploy", "private heartbeat secret");
        private_entry.set_sensitivity(MemorySensitivity::Private);
        runtime.memory.store(private_entry).await.unwrap();

        let workspace_entry = new_entry("deploy", "workspace heartbeat note");
        runtime.memory.store(workspace_entry).await.unwrap();

        let mut heartbeat_runtime = runtime.clone();
        heartbeat_runtime.invocation = Some(Arc::new(ToolInvocationContext {
            session_id: SessionId("heartbeat-session".to_string()),
            sender: SenderInfo {
                id: "heartbeat-1".to_string(),
                name: Some("Heartbeat".to_string()),
                channel: "scheduler".to_string(),
            },
            metadata: HashMap::from([("heartbeat_task_id".to_string(), "task-1".to_string())]),
        }));

        let recall = execute_tool(
            &ToolCall::new(
                "memory_recall",
                HashMap::from([("query".to_string(), serde_json::json!("deploy"))]),
            ),
            &heartbeat_runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();
        assert!(recall.success);
        assert!(recall.output.contains("workspace heartbeat note"));
        assert!(!recall.output.contains("private heartbeat secret"));
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
                ..Default::default()
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
    async fn scheduler_restart_recovery_executes_persisted_due_tool_jobs() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_scheduler_restart_recovery_{}",
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
                            "key": "restartrecovery",
                            "value": "restored run"
                        }),
                    ),
                ]),
            ),
            &runtime,
        )
        .await;
        assert!(schedule.success);

        let job_id = {
            let scheduler = runtime.scheduler.lock().await;
            let jobs = scheduler.list().await;
            let id = jobs[0].id.clone();
            drop(jobs);
            let mut stored = scheduler.get(&id).await.unwrap();
            stored.next_run = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
            scheduler.unschedule(&id).await.unwrap();
            scheduler.schedule(stored).await.unwrap();
            scheduler
                .update_status(&id, JobStatus::Running)
                .await
                .unwrap();
            id
        };

        drop(runtime);

        let restarted = ToolRuntime::from_config(
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
                enabled: true,
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

        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let scheduler = restarted.scheduler.lock().await;
                if let Some(job) = scheduler.get(&job_id).await {
                    if job.status == JobStatus::Completed {
                        break;
                    }
                }
                drop(scheduler);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();

        let recalled = restarted
            .memory
            .recall(&MemoryQuery {
                query: "restartrecovery".to_string(),
                limit: 5,
                min_score: 0.0,
                group_id: None,
                ..Default::default()
            })
            .await
            .unwrap();
        let scheduler = restarted.scheduler.lock().await;
        let job = scheduler.get(&job_id).await.unwrap();
        scheduler.stop().await;

        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert_eq!(job.run_count, 1);
        assert!(recalled
            .iter()
            .any(|entry| entry.entry.key == "restartrecovery"
                && entry.entry.content == "restored run"));
    }

    #[tokio::test]
    async fn github_tools_use_configured_runtime_client_against_local_stub() {
        use base64::Engine;

        let root = std::env::temp_dir().join(format!(
            "borgclaw_github_tool_stub_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let readme = base64::engine::general_purpose::STANDARD.encode("hello from github");
        let base_url = format!("file://{}", root.display());
        let repo_url = format!("{}/repos/owner/repo", base_url);
        let branches_url = format!("{}/repos/owner/repo/branches", base_url);
        let readme_url = format!("{}/repos/owner/repo/contents/README.md?ref=main", base_url);
        write_fixture(
            &crate::skills::github::fixture_path_for_request(&base_url, "GET", &repo_url)
                .unwrap()
                .unwrap(),
            serde_json::json!({
                "name": "repo",
                "full_name": "owner/repo",
                "owner": {"login": "owner"},
                "description": "stub repo",
                "private": false,
                "html_url": "https://github.com/owner/repo",
                "default_branch": "main"
            })
            .to_string(),
        );
        write_fixture(
            &crate::skills::github::fixture_path_for_request(&base_url, "GET", &branches_url)
                .unwrap()
                .unwrap(),
            serde_json::json!([
                {
                    "name": "main",
                    "commit": {"sha": "abc123"},
                    "protected": true
                }
            ])
            .to_string(),
        );
        write_fixture(
            &crate::skills::github::fixture_path_for_request(&base_url, "GET", &readme_url)
                .unwrap()
                .unwrap(),
            serde_json::json!({
                "content": readme,
                "encoding": "base64"
            })
            .to_string(),
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
                github: crate::config::GitHubSkillConfig {
                    token: "test-token".to_string(),
                    user_agent: "BorgClawTest/1.0".to_string(),
                    base_url,
                    safety: crate::config::GitHubSafetyConfig {
                        repo_access: "all".to_string(),
                        require_confirmation: false,
                        allowlist: Vec::new(),
                    },
                },
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        let repo = execute_tool(
            &ToolCall::new(
                "github_get_repo",
                HashMap::from([
                    ("owner".to_string(), serde_json::json!("owner")),
                    ("repo".to_string(), serde_json::json!("repo")),
                ]),
            ),
            &runtime,
        )
        .await;
        let branches = execute_tool(
            &ToolCall::new(
                "github_list_branches",
                HashMap::from([
                    ("owner".to_string(), serde_json::json!("owner")),
                    ("repo".to_string(), serde_json::json!("repo")),
                ]),
            ),
            &runtime,
        )
        .await;
        let readme = execute_tool(
            &ToolCall::new(
                "github_get_file",
                HashMap::from([
                    ("owner".to_string(), serde_json::json!("owner")),
                    ("repo".to_string(), serde_json::json!("repo")),
                    ("path".to_string(), serde_json::json!("README.md")),
                    ("ref".to_string(), serde_json::json!("main")),
                ]),
            ),
            &runtime,
        )
        .await;
        std::fs::remove_dir_all(&root).unwrap();

        assert!(repo.success);
        assert!(repo
            .output
            .contains("owner/repo | default=main | private=false"));
        assert!(branches.success);
        assert!(branches.output.contains("main | abc123 | protected=true"));
        assert!(readme.success);
        assert_eq!(readme.output, "hello from github");
    }

    #[tokio::test]
    async fn browser_tools_use_configured_bridge_runtime() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_browser_tool_stub_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let bridge = root.join("fake_bridge.py");
        std::fs::write(
            &bridge,
            r#"#!/usr/bin/env python3
import json
import sys

for line in sys.stdin:
    req = json.loads(line)
    action = req.get("action")
    resp = {"id": req["id"], "success": True}
    if action == "new_page":
        resp["result"] = {"ok": True}
    elif action == "get_url":
        resp["result"] = {"url": "https://example.com/dashboard"}
    elif action == "evaluate":
        resp["result"] = {"value": 42, "source": "fake-bridge"}
    elif action == "close":
        resp["result"] = {"closed": True}
    else:
        resp["success"] = False
        resp["error"] = f"unexpected action: {action}"
    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bridge).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bridge, perms).unwrap();
        }

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
                browser: crate::skills::BrowserConfig {
                    node_path: std::path::PathBuf::from("python3"),
                    bridge_path: bridge,
                    ..Default::default()
                },
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        let get_url =
            execute_tool(&ToolCall::new("browser_get_url", HashMap::new()), &runtime).await;
        let eval = execute_tool(
            &ToolCall::new(
                "browser_eval_js",
                HashMap::from([(
                    "script".to_string(),
                    serde_json::json!("window.location.href"),
                )]),
            ),
            &runtime,
        )
        .await;

        std::fs::remove_dir_all(&root).unwrap();

        assert!(get_url.success);
        assert_eq!(get_url.output, "https://example.com/dashboard");
        assert!(eval.success);
        assert!(eval.output.contains("\"value\":42"));
        assert!(eval.output.contains("\"source\":\"fake-bridge\""));
    }

    #[tokio::test]
    async fn google_tools_use_configured_runtime_endpoints_against_local_stub() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_google_tool_stub_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let token_path = root.join("google-token.json");
        std::fs::write(
            &token_path,
            serde_json::json!({
                "access_token": "test-token",
                "refresh_token": null,
                "expires_at": chrono::Utc::now().timestamp() + 3600,
                "scopes": []
            })
            .to_string(),
        )
        .unwrap();

        let base_url = format!("file://{}", root.display());
        let message_url = format!("{}/gmail/v1/users/me/messages/msg-1", base_url);
        let download_url = format!("{}/drive/v3/files/file-1?alt=media", base_url);
        let event_url = format!("{}/calendar/v3/calendars/primary/events", base_url);
        write_fixture(
            &crate::skills::google::fixture_path_for_request(&base_url, "GET", &message_url)
                .unwrap()
                .unwrap(),
            serde_json::json!({
                "id": "msg-1",
                "thread_id": "thread-1",
                "snippet": "hello",
                "payload": {
                    "headers": [
                        {"name": "From", "value": "sender@example.com"},
                        {"name": "Subject", "value": "Stub message"}
                    ]
                }
            })
            .to_string(),
        );
        write_fixture(
            &crate::skills::google::fixture_path_for_request(&base_url, "GET", &download_url)
                .unwrap()
                .unwrap(),
            b"drive-bytes",
        );
        write_fixture(
            &crate::skills::google::fixture_path_for_request(&base_url, "POST", &event_url)
                .unwrap()
                .unwrap(),
            serde_json::json!({
                "id": "event-1",
                "summary": "Stub event",
                "description": "calendar body",
                "start": { "dateTime": "2026-03-19T15:00:00Z" },
                "end": { "dateTime": "2026-03-19T16:00:00Z" }
            })
            .to_string(),
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
                google: crate::skills::GoogleOAuthConfig {
                    client_id: "client-id".to_string(),
                    client_secret: "client-secret".to_string(),
                    token_path,
                    gmail_base_url: base_url.clone(),
                    drive_base_url: base_url.clone(),
                    calendar_base_url: base_url.clone(),
                    auth_base_url: format!("{}/oauth2", base_url),
                    ..Default::default()
                },
                skills_path: root.join("skills"),
                ..Default::default()
            },
            &crate::config::McpConfig::default(),
            &SecurityConfig::default(),
        )
        .await
        .unwrap();

        let message = execute_tool(
            &ToolCall::new(
                "google_get_message",
                HashMap::from([("id".to_string(), serde_json::json!("msg-1"))]),
            ),
            &runtime,
        )
        .await;
        let download = execute_tool(
            &ToolCall::new(
                "google_download_file",
                HashMap::from([("id".to_string(), serde_json::json!("file-1"))]),
            ),
            &runtime,
        )
        .await;
        let event = execute_tool(
            &ToolCall::new(
                "google_create_event",
                HashMap::from([
                    ("summary".to_string(), serde_json::json!("Stub event")),
                    (
                        "start".to_string(),
                        serde_json::json!("2026-03-19T15:00:00Z"),
                    ),
                    ("end".to_string(), serde_json::json!("2026-03-19T16:00:00Z")),
                    (
                        "description".to_string(),
                        serde_json::json!("calendar body"),
                    ),
                ]),
            ),
            &runtime,
        )
        .await;
        std::fs::remove_dir_all(&root).unwrap();

        assert!(message.success);
        assert!(message
            .output
            .contains("msg-1 | sender@example.com | Stub message"));
        assert!(download.success);
        assert_eq!(
            download.metadata.get("file_id").map(String::as_str),
            Some("file-1")
        );
        assert!(download.output.contains("downloaded 11 bytes"));
        assert!(event.success);
        assert!(event.output.contains("event-1 | Stub event"));
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
                ..Default::default()
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
                ..Default::default()
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

    async fn test_runtime() -> ToolRuntime {
        let root =
            std::env::temp_dir().join(format!("borgclaw_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        ToolRuntime::from_config(
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
        .unwrap()
    }

    #[test]
    fn tool_retry_policy_calculates_exponential_backoff() {
        let policy = ToolRetryPolicy::exponential(3);

        // First retry should be around initial_delay
        let delay1 = policy.calculate_delay(1);
        assert!((900..=1100).contains(&delay1)); // 1000ms with 10% jitter

        // Second retry should be around 2x initial_delay
        let delay2 = policy.calculate_delay(2);
        assert!((1800..=2200).contains(&delay2)); // 2000ms with 10% jitter

        // Third retry should be around 4x initial_delay
        let delay3 = policy.calculate_delay(3);
        assert!((3600..=4400).contains(&delay3)); // 4000ms with 10% jitter
    }

    #[test]
    fn tool_retry_policy_respects_max_delay() {
        let policy = ToolRetryPolicy::exponential(10)
            .with_initial_delay(1000)
            .with_max_delay(5000);

        // High attempt numbers should be capped at max_delay
        let delay = policy.calculate_delay(10);
        assert!(delay <= 5500); // max_delay + jitter
    }

    #[tokio::test]
    async fn execute_with_retry_succeeds_immediately_on_success() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let policy = ToolRetryPolicy::exponential(3);
        let call_count = Arc::new(AtomicUsize::new(0));

        let result = {
            let call_count = call_count.clone();
            execute_with_retry(&policy, "test_tool", move || {
                let call_count = call_count.clone();
                async move {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    ToolResult::ok("success")
                }
            })
            .await
        };

        assert!(result.success);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        assert!(!result.metadata.contains_key("retry_attempts"));
    }

    #[tokio::test]
    async fn execute_with_retry_retries_on_failure_then_succeeds() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let policy = ToolRetryPolicy {
            max_retries: 3,
            initial_delay_ms: 10, // Short delay for testing
            max_delay_ms: 100,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for predictable testing
        };
        let call_count = Arc::new(AtomicUsize::new(0));

        let result = {
            let call_count = call_count.clone();
            execute_with_retry(&policy, "test_tool", move || {
                let call_count = call_count.clone();
                async move {
                    let count = call_count.fetch_add(1, Ordering::SeqCst) + 1;
                    if count < 3 {
                        ToolResult::err("network timeout")
                            .with_metadata("error_type", "TimeoutError")
                    } else {
                        ToolResult::ok("success")
                    }
                }
            })
            .await
        };

        assert!(result.success);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        assert_eq!(
            result.metadata.get("retry_attempts"),
            Some(&"2".to_string())
        );
    }

    #[tokio::test]
    async fn execute_with_retry_gives_up_after_max_retries() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let policy = ToolRetryPolicy {
            max_retries: 2,
            initial_delay_ms: 10,
            max_delay_ms: 100,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0,
        };
        let call_count = Arc::new(AtomicUsize::new(0));

        let result = {
            let call_count = call_count.clone();
            execute_with_retry(&policy, "test_tool", move || {
                let call_count = call_count.clone();
                async move {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    ToolResult::err("timeout").with_metadata("error_type", "TimeoutError")
                }
            })
            .await
        };

        assert!(!result.success);
        assert_eq!(call_count.load(Ordering::SeqCst), 3); // Initial + 2 retries
        assert!(result.metadata.contains_key("retry_exhausted"));
    }

    #[tokio::test]
    async fn execute_with_retry_skips_non_retryable_errors() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let policy = ToolRetryPolicy::exponential(3);
        let call_count = Arc::new(AtomicUsize::new(0));

        let result = {
            let call_count = call_count.clone();
            execute_with_retry(&policy, "test_tool", move || {
                let call_count = call_count.clone();
                async move {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    ToolResult::err("validation failed") // Not retryable
                }
            })
            .await
        };

        assert!(!result.success);
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // No retries
        assert_eq!(
            result.metadata.get("retry_skipped"),
            Some(&"error_not_retryable".to_string())
        );
    }

    #[test]
    fn tool_result_detects_retryable_errors() {
        // Retryable errors
        let timeout_result =
            ToolResult::err("connection timeout").with_metadata("error_type", "TimeoutError");
        assert!(timeout_result.is_retryable());

        let network_result =
            ToolResult::err("network error").with_metadata("error_type", "NetworkError");
        assert!(network_result.is_retryable());

        let rate_limit_result =
            ToolResult::err("rate limited").with_metadata("error_type", "RateLimitError");
        assert!(rate_limit_result.is_retryable());

        let http_503 = ToolResult::err("Service Unavailable: 503");
        assert!(http_503.is_retryable());

        // Non-retryable errors
        let success_result = ToolResult::ok("success");
        assert!(!success_result.is_retryable());

        let validation_result = ToolResult::err("invalid input");
        assert!(!validation_result.is_retryable());

        let auth_result = ToolResult::err("unauthorized").with_metadata("error_type", "AuthError");
        assert!(!auth_result.is_retryable());
    }

    #[tokio::test]
    async fn tool_execute_with_retry_uses_tool_policy() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let tool = Tool::new("test_tool", "Test tool").with_retry_policy(ToolRetryPolicy {
            max_retries: 2,
            initial_delay_ms: 10,
            max_delay_ms: 100,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0,
        });

        let call_count = Arc::new(AtomicUsize::new(0));
        let result = {
            let call_count = call_count.clone();
            tool.execute_with_retry(move || {
                let call_count = call_count.clone();
                async move {
                    let count = call_count.fetch_add(1, Ordering::SeqCst) + 1;
                    if count < 2 {
                        ToolResult::err("timeout").with_metadata("error_type", "TimeoutError")
                    } else {
                        ToolResult::ok("success")
                    }
                }
            })
            .await
        };

        assert!(result.success);
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
        assert_eq!(
            result.metadata.get("retry_attempts"),
            Some(&"1".to_string())
        );
    }

    // Approval gate tests for Google Drive tools (TICKET-056)

    #[tokio::test]
    async fn google_share_file_requires_approval_in_supervised_mode() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_gdrive_share_approval_{}",
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
                "google_share_file",
                HashMap::from([
                    ("file_id".to_string(), serde_json::json!("test_file_123")),
                    ("email".to_string(), serde_json::json!("user@example.com")),
                    ("role".to_string(), serde_json::json!("reader")),
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
            Some("google_share_file")
        );
    }

    #[tokio::test]
    async fn google_remove_permission_requires_approval_in_supervised_mode() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_gdrive_rmperm_approval_{}",
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
                "google_remove_permission",
                HashMap::from([
                    ("file_id".to_string(), serde_json::json!("test_file_123")),
                    ("permission_id".to_string(), serde_json::json!("perm_456")),
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
            Some("google_remove_permission")
        );
    }

    #[tokio::test]
    async fn google_delete_file_requires_approval_in_supervised_mode() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_gdrive_delete_approval_{}",
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
                "google_delete_file",
                HashMap::from([
                    ("file_id".to_string(), serde_json::json!("test_file_123")),
                    ("permanent".to_string(), serde_json::json!(true)),
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
            Some("google_delete_file")
        );
    }

    #[tokio::test]
    async fn google_tools_with_approval_definitions_match_enforcement() {
        // Verify that all tools marked with_approval(true) are in the set of
        // tools that actually enforce approval via require_tool_approval
        let tools = builtin_tools();
        let approval_tools: Vec<&str> = tools
            .iter()
            .filter(|t| t.requires_approval)
            .map(|t| t.name.as_str())
            .collect();

        // These Google Drive tools must require approval
        assert!(
            approval_tools.contains(&"google_share_file"),
            "google_share_file must require approval"
        );
        assert!(
            approval_tools.contains(&"google_remove_permission"),
            "google_remove_permission must require approval"
        );
        assert!(
            approval_tools.contains(&"google_delete_file"),
            "google_delete_file must require approval"
        );
    }
}

/// Built-in tools
pub fn builtin_tools() -> Vec<Tool> {
    let mut tools = Vec::new();
    browser::register(&mut tools);
    file::register(&mut tools);
    github::register(&mut tools);
    google::register(&mut tools);
    mcp::register(&mut tools);
    media::register(&mut tools);
    memory::register(&mut tools);
    plugin::register(&mut tools);
    schedule::register(&mut tools);
    shell::register(&mut tools);
    web::register(&mut tools);
    tools.extend(vec![Tool::new("message", "Send a message to the user")
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
        .with_tags(vec!["communication".to_string()])]);
    tools
}
