//! Configuration module for BorgClaw

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::skills::image::{SdApiType, SdConfig};
use crate::skills::stt::{OpenAiConfig, OpenWebUiConfig, WhisperCppConfig};
use crate::skills::url_shortener::YourlsConfig;
use crate::skills::{
    BrowserConfig, ElevenLabsConfig, GitHubConfig, GitHubSafety, GoogleOAuthConfig, ImageBackend,
    OperationType, RepoAccess, SttBackend, UrlShortenerProvider,
};

/// Main application configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Agent configuration
    pub agent: AgentConfig,
    /// Channel configurations
    pub channels: HashMap<String, ChannelConfig>,
    /// Security settings
    pub security: SecurityConfig,
    /// Memory settings
    pub memory: MemoryConfig,
    /// Heartbeat settings
    pub heartbeat: HeartbeatConfig,
    /// Scheduler settings
    pub scheduler: SchedulerConfig,
    /// Skills configuration
    pub skills: SkillsConfig,
    /// MCP server configuration
    pub mcp: McpConfig,
    /// Onboarding registrar (title/chapter tracking)
    pub registrar: RegistrarConfig,
}

impl AppConfig {
    #[allow(clippy::result_large_err)]
    pub fn validate(&self) -> Result<(), config_support::ValidationError> {
        let mut error = config_support::ValidationError::default();

        for (name, server) in &self.mcp.servers {
            if let Some(url) = &server.url {
                if !url.is_empty() && (url.starts_with("http://") || url.starts_with("https://")) {
                    continue;
                }
                error
                    .mcp_servers
                    .push(format!("{}: invalid URL '{}'", name, url));
            } else {
                error.mcp_servers.push(format!("{}: missing URL", name));
            }
        }

        if let Some(ref path) = self.agent.soul_path {
            if !path.exists() {
                error.soul_path = Some(path.display().to_string());
            }
        }

        if let Some(ref path) = self.agent.workspace.parent() {
            if !path.exists() {
                if let Err(e) = std::fs::create_dir_all(path) {
                    error.workspace = Some(format!("{}: {}", path.display(), e));
                }
            }
        }

        if let Some(rpm) = self.agent.rate_limit_rpm {
            if rpm == 0 {
                error.rate_limit = Some("rate_limit_rpm must be greater than 0".to_string());
            }
        }

        if self.heartbeat.check_interval_seconds == 0 {
            error.heartbeat_interval =
                Some("heartbeat check_interval_seconds must be greater than 0".to_string());
        }

        if matches!(self.memory.effective_backend(), MemoryBackend::Postgres)
            && self
                .memory
                .connection_string
                .as_deref()
                .map_or(true, |value| value.trim().is_empty())
        {
            error.memory = Some(
                "memory.connection_string is required when memory.backend = postgres".to_string(),
            );
        }

        if self.memory.external.enabled
            && self
                .memory
                .external
                .endpoint
                .as_deref()
                .map_or(true, |value| value.trim().is_empty())
        {
            error.memory = Some(
                "memory.external.endpoint is required when memory.external.enabled = true"
                    .to_string(),
            );
        }

        if self.security.docker.enabled {
            if self.security.docker.image.trim().is_empty() {
                error.security = Some(
                    "security.docker.image is required when security.docker.enabled = true"
                        .to_string(),
                );
            } else if self.security.docker.timeout_seconds == 0 {
                error.security =
                    Some("security.docker.timeout_seconds must be greater than 0".to_string());
            } else if self.security.docker.memory_limit_mb == 0 {
                error.security =
                    Some("security.docker.memory_limit_mb must be greater than 0".to_string());
            }
        }

        if error.mcp_servers.is_empty()
            && error.soul_path.is_none()
            && error.workspace.is_none()
            && error.rate_limit.is_none()
            && error.heartbeat_interval.is_none()
            && error.memory.is_none()
            && error.security.is_none()
        {
            Ok(())
        } else {
            Err(error)
        }
    }
}

/// Onboarding registrar for component title/chapter mapping
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RegistrarConfig {
    /// Title -> chapters mapping
    pub chapters: HashMap<String, Vec<String>>,
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    /// Default model to use
    pub model: String,
    /// API provider (openai, anthropic, etc.)
    pub provider: String,
    /// Optional named provider profile selection
    pub provider_profile: Option<String>,
    /// Identity document format for `soul_path`
    pub identity_format: IdentityFormat,
    /// Max tokens per response
    pub max_tokens: u32,
    /// Temperature setting
    pub temperature: f32,
    /// System prompt / Soul.md path
    pub soul_path: Option<PathBuf>,
    /// Workspace directory
    pub workspace: PathBuf,
    /// Heartbeat interval in minutes
    pub heartbeat_interval: u32,
    /// Rate limit: requests per minute (default: provider-specific)
    pub rate_limit_rpm: Option<u32>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            provider_profile: None,
            identity_format: IdentityFormat::Auto,
            max_tokens: 4096,
            temperature: 0.7,
            soul_path: None,
            workspace: PathBuf::from(".borgclaw/workspace"),
            heartbeat_interval: 30,
            rate_limit_rpm: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IdentityFormat {
    #[default]
    Auto,
    Markdown,
    Aieos,
}

/// Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelConfig {
    /// Enable this channel
    pub enabled: bool,
    /// Bot token / credentials
    #[serde(alias = "token")]
    #[serde(skip_serializing)]
    pub credentials: Option<String>,
    /// Allowed senders (empty = allow all)
    pub allow_from: Vec<String>,
    /// DM policy: open, pairing, closed
    pub dm_policy: DmPolicy,
    /// Custom settings per channel
    #[serde(flatten)]
    pub extra: HashMap<String, toml::Value>,
}

/// DM policy for channels
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DmPolicy {
    /// Anyone can message
    Open,
    /// Must enter pairing code
    #[default]
    Pairing,
    /// No DMs allowed
    #[serde(alias = "closed")]
    Blocked,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            credentials: None,
            allow_from: vec![],
            dm_policy: DmPolicy::Pairing,
            extra: HashMap::new(),
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// Enable WASM sandbox (primary sandbox mechanism for plugin execution)
    pub wasm_sandbox: bool,
    /// Max registered WASM sandbox instances
    pub wasm_max_instances: usize,
    /// Enable the default command blocklist
    pub command_blocklist: bool,
    /// Additional blocklist entries
    pub extra_blocked: Vec<String>,
    /// Optional command allowlist; when non-empty, commands must match at least one pattern
    pub allowed_commands: Vec<String>,
    /// Execution approval mode
    pub approval_mode: ApprovalMode,
    /// Pairing configuration
    pub pairing: PairingConfig,
    /// Enable prompt injection defense
    pub prompt_injection_defense: bool,
    /// Prompt injection action
    pub injection_action: InjectionAction,
    /// Enable secret leak detection
    #[serde(alias = "leak_detection")]
    pub secret_leak_detection: bool,
    /// Secret leak action
    pub leak_action: LeakAction,
    /// Enable encrypted secret persistence
    pub secrets_encryption: bool,
    /// Encrypted secrets file path
    pub secrets_path: PathBuf,
    /// Optional vault integration
    pub vault: VaultConfig,
    /// Workspace file/path policy
    pub workspace: WorkspacePolicyConfig,
    /// Optional Docker sandbox for shell command execution
    pub docker: DockerSandboxConfig,
    /// Enable SSRF protection
    pub ssrf_protection: bool,
    /// SSRF allowlist - additional hosts to allow (regex patterns)
    pub ssrf_allowlist: Vec<String>,
    /// SSRF blocklist - additional hosts to block (regex patterns)
    pub ssrf_blocklist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PairingConfig {
    pub enabled: bool,
    pub code_length: usize,
    pub expiry_seconds: u64,
}

impl Default for PairingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            code_length: 6,
            expiry_seconds: 300,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InjectionAction {
    #[default]
    Block,
    Sanitize,
    Warn,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LeakAction {
    #[default]
    Redact,
    Block,
    Warn,
}

/// Execution approval mode
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalMode {
    /// Read-only, no execution
    ReadOnly,
    /// Supervised, requires approval
    Supervised,
    /// Full autonomy
    #[default]
    Autonomous,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            wasm_sandbox: true,
            wasm_max_instances: 10,
            command_blocklist: true,
            extra_blocked: Vec::new(),
            allowed_commands: Vec::new(),
            approval_mode: ApprovalMode::Autonomous,
            pairing: PairingConfig::default(),
            prompt_injection_defense: true,
            injection_action: InjectionAction::Block,
            secret_leak_detection: true,
            leak_action: LeakAction::Redact,
            secrets_encryption: true,
            secrets_path: PathBuf::from(".borgclaw/secrets.enc"),
            vault: VaultConfig::default(),
            workspace: WorkspacePolicyConfig::default(),
            docker: DockerSandboxConfig::default(),
            ssrf_protection: true,
            ssrf_allowlist: Vec::new(),
            ssrf_blocklist: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerSandboxConfig {
    pub enabled: bool,
    pub image: String,
    pub network: DockerNetworkPolicy,
    pub workspace_mount: DockerWorkspaceMount,
    pub read_only_rootfs: bool,
    pub tmpfs: bool,
    pub memory_limit_mb: u32,
    pub cpu_limit: Option<String>,
    pub timeout_seconds: u64,
    pub allowed_tools: Vec<String>,
    pub allowed_roots: Vec<PathBuf>,
    pub extra_env_allowlist: Vec<String>,
}

impl Default for DockerSandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            image: "borgclaw-sandbox:base".to_string(),
            network: DockerNetworkPolicy::None,
            workspace_mount: DockerWorkspaceMount::ReadOnly,
            read_only_rootfs: true,
            tmpfs: true,
            memory_limit_mb: 512,
            cpu_limit: Some("1.0".to_string()),
            timeout_seconds: 120,
            allowed_tools: vec!["execute_command".to_string()],
            allowed_roots: Vec::new(),
            extra_env_allowlist: vec!["PATH".to_string(), "HOME".to_string()],
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DockerNetworkPolicy {
    #[default]
    None,
    Bridge,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum DockerWorkspaceMount {
    #[default]
    #[serde(rename = "ro")]
    ReadOnly,
    #[serde(rename = "rw")]
    ReadWrite,
    #[serde(rename = "off")]
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspacePolicyConfig {
    /// Restrict file tools to the agent workspace only
    pub workspace_only: bool,
    /// Additional allowed roots when workspace_only is disabled
    pub allowed_roots: Vec<PathBuf>,
    /// Forbidden paths relative to the workspace or as absolute paths
    pub forbidden_paths: Vec<PathBuf>,
}

impl Default for WorkspacePolicyConfig {
    fn default() -> Self {
        Self {
            workspace_only: true,
            allowed_roots: Vec::new(),
            forbidden_paths: Vec::new(),
        }
    }
}

/// External vault configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct VaultConfig {
    pub provider: Option<String>,
    pub bitwarden: BitwardenVaultConfig,
    #[serde(rename = "1password")]
    pub one_password: OnePasswordVaultConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BitwardenVaultConfig {
    pub cli_path: PathBuf,
    pub session_env: String,
    pub server_url: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub use_cli: bool,
}

impl Default for BitwardenVaultConfig {
    fn default() -> Self {
        Self {
            cli_path: PathBuf::from("bw"),
            session_env: "BW_SESSION".to_string(),
            server_url: None,
            client_id: None,
            client_secret: None,
            use_cli: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OnePasswordVaultConfig {
    pub cli_path: PathBuf,
    pub vault: Option<String>,
    pub account: Option<String>,
}

impl Default for OnePasswordVaultConfig {
    fn default() -> Self {
        Self {
            cli_path: PathBuf::from("op"),
            vault: None,
            account: None,
        }
    }
}

/// Memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    /// Storage backend
    pub backend: MemoryBackend,
    /// SQLite database path
    #[serde(alias = "memory_path")]
    pub database_path: PathBuf,
    /// PostgreSQL connection string for the postgres backend
    pub connection_string: Option<String>,
    /// HTTP embedding endpoint used for hybrid search
    pub embedding_endpoint: Option<String>,
    /// Enable hybrid search
    pub hybrid_search: bool,
    /// Vector store provider
    pub vector_provider: String,
    /// Embedding model
    pub embedding_model: String,
    /// Max entries in session before compaction
    #[serde(alias = "session_compaction_threshold")]
    pub session_max_entries: usize,
    /// Number of recent messages preserved during compaction
    pub session_keep_recent: usize,
    /// Whether important context should be preserved during compaction
    pub session_keep_important: bool,
    /// Optional external OpenMemory-style adapter
    pub external: ExternalMemoryConfig,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            backend: MemoryBackend::Sqlite,
            database_path: PathBuf::from(".borgclaw/memory"),
            connection_string: None,
            embedding_endpoint: None,
            hybrid_search: true,
            vector_provider: "sqlite".to_string(),
            embedding_model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            session_max_entries: 100,
            session_keep_recent: 20,
            session_keep_important: true,
            external: ExternalMemoryConfig::default(),
        }
    }
}

impl MemoryConfig {
    pub fn effective_backend(&self) -> MemoryBackend {
        if matches!(self.backend, MemoryBackend::Sqlite) {
            match self.vector_provider.as_str() {
                "postgres" => MemoryBackend::Postgres,
                "memory" => MemoryBackend::Memory,
                _ => MemoryBackend::Sqlite,
            }
        } else {
            self.backend
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryBackend {
    Sqlite,
    Postgres,
    Memory,
}

/// Optional external memory adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExternalMemoryConfig {
    /// Enable additive external memory integration
    pub enabled: bool,
    /// Base endpoint for the external memory service
    pub endpoint: Option<String>,
    /// Mirror writes to the external adapter
    pub mirror_writes: bool,
    /// Request timeout in seconds
    pub timeout_seconds: u64,
}

impl Default for ExternalMemoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            mirror_writes: true,
            timeout_seconds: 15,
        }
    }
}

/// Heartbeat configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HeartbeatConfig {
    /// Enable heartbeat scheduling
    pub enabled: bool,
    /// Scheduler polling interval in seconds
    pub check_interval_seconds: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_seconds: 60,
        }
    }
}

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SchedulerConfig {
    /// Enable scheduler
    pub enabled: bool,
    /// Max concurrent jobs
    pub max_concurrent_jobs: usize,
    /// Job timeout seconds
    pub job_timeout: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_concurrent_jobs: 5,
            job_timeout: 3600,
        }
    }
}

/// Skills configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillsConfig {
    /// Skills directory
    pub skills_path: PathBuf,
    /// Enable skill auto-loading
    pub auto_load: bool,
    /// Skill registry URL (for remote skills)
    pub registry_url: Option<String>,
    /// GitHub skill configuration
    pub github: GitHubSkillConfig,
    /// Google skill configuration
    pub google: GoogleOAuthConfig,
    /// Browser skill configuration
    pub browser: BrowserConfig,
    /// Speech-to-text configuration
    pub stt: SttSkillConfig,
    /// Text-to-speech configuration
    pub tts: TtsSkillConfig,
    /// Image generation configuration
    pub image: ImageSkillConfig,
    /// URL shortener configuration
    pub url_shortener: UrlShortenerSkillConfig,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            skills_path: PathBuf::from(".borgclaw/skills"),
            auto_load: true,
            registry_url: None,
            github: GitHubSkillConfig::default(),
            google: GoogleOAuthConfig::default(),
            browser: BrowserConfig::default(),
            stt: SttSkillConfig::default(),
            tts: TtsSkillConfig::default(),
            image: ImageSkillConfig::default(),
            url_shortener: UrlShortenerSkillConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GitHubSkillConfig {
    pub token: String,
    pub user_agent: String,
    pub base_url: String,
    pub safety: GitHubSafetyConfig,
}

impl Default for GitHubSkillConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            user_agent: "BorgClaw/1.0".to_string(),
            base_url: "https://api.github.com".to_string(),
            safety: GitHubSafetyConfig::default(),
        }
    }
}

impl GitHubSkillConfig {
    pub fn client_config(&self) -> Option<GitHubConfig> {
        if self.token.trim().is_empty() {
            return None;
        }

        Some(
            GitHubConfig::new(self.token.clone())
                .with_base_url(self.base_url.clone())
                .with_user_agent(self.user_agent.clone()),
        )
    }

    pub fn safety_policy(&self) -> GitHubSafety {
        self.safety.to_runtime()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GitHubSafetyConfig {
    pub repo_access: String,
    pub require_confirmation: bool,
    pub allowlist: Vec<String>,
}

impl Default for GitHubSafetyConfig {
    fn default() -> Self {
        Self {
            repo_access: "owned_only".to_string(),
            require_confirmation: true,
            allowlist: Vec::new(),
        }
    }
}

impl GitHubSafetyConfig {
    pub fn to_runtime(&self) -> GitHubSafety {
        let repo_access = match self.repo_access.as_str() {
            "all" => RepoAccess::Any,
            "whitelist" | "allowlist" => RepoAccess::Allowlisted(self.allowlist.clone()),
            _ => RepoAccess::OwnedOnly,
        };

        let require_double_confirm_for = if self.require_confirmation {
            vec![
                OperationType::DeleteBranch,
                OperationType::ForcePush,
                OperationType::MergePR,
                OperationType::DeleteRepo,
                OperationType::DeleteRelease,
                OperationType::DeleteTag,
                OperationType::ClosePR,
            ]
        } else {
            Vec::new()
        };

        GitHubSafety {
            repo_access,
            require_double_confirm_for,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SttSkillConfig {
    pub backend: String,
    pub openai: OpenAiSttConfig,
    pub openwebui: OpenWebUiConfig,
    pub whispercpp: WhisperCppConfig,
}

impl Default for SttSkillConfig {
    fn default() -> Self {
        Self {
            backend: "openai".to_string(),
            openai: OpenAiSttConfig::default(),
            openwebui: OpenWebUiConfig::default(),
            whispercpp: WhisperCppConfig::default(),
        }
    }
}

impl SttSkillConfig {
    pub fn backend_config(&self) -> SttBackend {
        match self.backend.as_str() {
            "openwebui" => SttBackend::OpenWebUi(self.openwebui.clone()),
            "whispercpp" => SttBackend::WhisperCpp(self.whispercpp.clone()),
            _ => SttBackend::OpenAI(OpenAiConfig {
                api_key: self.openai.api_key.clone(),
                model: self.openai.model.clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiSttConfig {
    pub api_key: String,
    pub model: String,
}

impl Default for OpenAiSttConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "whisper-1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsSkillConfig {
    pub provider: String,
    #[serde(flatten)]
    pub elevenlabs: ElevenLabsConfig,
}

impl Default for TtsSkillConfig {
    fn default() -> Self {
        Self {
            provider: "elevenlabs".to_string(),
            elevenlabs: ElevenLabsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageSkillConfig {
    pub provider: String,
    pub dalle: DallEImageConfig,
    pub stable_diffusion: StableDiffusionImageConfig,
}

impl Default for ImageSkillConfig {
    fn default() -> Self {
        Self {
            provider: "dalle".to_string(),
            dalle: DallEImageConfig::default(),
            stable_diffusion: StableDiffusionImageConfig::default(),
        }
    }
}

impl ImageSkillConfig {
    pub fn backend(&self) -> ImageBackend {
        match self.provider.as_str() {
            "stable_diffusion" => ImageBackend::StableDiffusion(SdConfig {
                base_url: self.stable_diffusion.base_url.clone(),
                api_type: self.stable_diffusion.api_type,
            }),
            _ => ImageBackend::DallE3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DallEImageConfig {
    pub api_key: String,
    pub model: String,
    pub size: String,
}

impl Default for DallEImageConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "dall-e-3".to_string(),
            size: "1024x1024".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StableDiffusionImageConfig {
    pub base_url: String,
    pub api_type: SdApiType,
}

impl Default for StableDiffusionImageConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:7860".to_string(),
            api_type: SdApiType::Automatic1111,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UrlShortenerSkillConfig {
    pub provider: String,
    pub yourls: DocumentedYourlsConfig,
}

impl Default for UrlShortenerSkillConfig {
    fn default() -> Self {
        Self {
            provider: "isgd".to_string(),
            yourls: DocumentedYourlsConfig::default(),
        }
    }
}

impl UrlShortenerSkillConfig {
    pub fn provider_config(&self) -> UrlShortenerProvider {
        match self.provider.as_str() {
            "tinyurl" => UrlShortenerProvider::TinyUrl,
            "yourls" => UrlShortenerProvider::Yourls(self.yourls.to_runtime()),
            _ => UrlShortenerProvider::IsGd,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentedYourlsConfig {
    #[serde(alias = "api_url")]
    pub base_url: String,
    pub signature: String,
    pub username: String,
    pub password: String,
}

impl DocumentedYourlsConfig {
    pub fn to_runtime(&self) -> YourlsConfig {
        YourlsConfig {
            api_url: self.base_url.clone(),
            signature: self.signature.clone(),
            username: if self.username.is_empty() {
                None
            } else {
                Some(self.username.clone())
            },
            password: if self.password.is_empty() {
                None
            } else {
                Some(self.password.clone())
            },
        }
    }
}

/// MCP configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct McpConfig {
    pub servers: HashMap<String, McpServerConfig>,
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct McpServerConfig {
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub url: Option<String>,
    pub headers: HashMap<String, String>,
}

/// Load configuration from file
pub fn load_config(path: &PathBuf) -> Result<AppConfig, config_support::ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let mut config: AppConfig =
        toml::from_str(&content).map_err(config_support::ConfigError::Parse)?;

    // Normalize deprecated model names
    config.agent.model = normalize_model_name(&config.agent.model);

    Ok(config)
}

/// Normalize deprecated model names to current versions
fn normalize_model_name(model: &str) -> String {
    match model {
        "m2.77" => {
            tracing::info!("Normalized deprecated model name: m2.77 -> MiniMax-M2.7");
            "MiniMax-M2.7".to_string()
        }
        "m2.5" => {
            tracing::info!("Normalized deprecated model name: m2.5 -> MiniMax-M2.5");
            "MiniMax-M2.5".to_string()
        }
        "m2.7" => {
            tracing::info!("Normalized deprecated model name: m2.7 -> MiniMax-M2.7");
            "MiniMax-M2.7".to_string()
        }
        "k2.5" => {
            tracing::info!("Normalized deprecated model name: k2.5 -> kimi-k2.5");
            "kimi-k2.5".to_string()
        }
        other => other.to_string(),
    }
}

/// Save configuration to file
pub fn save_config(config: &AppConfig, path: &PathBuf) -> Result<(), config_support::ConfigError> {
    let content = toml::to_string_pretty(config)?;
    std::fs::write(path, content)?;
    Ok(())
}

mod config_support {
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum ConfigError {
        #[error("IO error: {0}")]
        Io(#[from] std::io::Error),
        #[error("TOML parse error: {0}")]
        Parse(#[from] toml::de::Error),
        #[error("TOML serialize error: {0}")]
        Serialize(#[from] toml::ser::Error),
        #[error("Validation error: {0}")]
        Validation(String),
    }

    #[derive(Debug, Default)]
    pub struct ValidationError {
        pub mcp_servers: Vec<String>,
        pub soul_path: Option<String>,
        pub workspace: Option<String>,
        pub rate_limit: Option<String>,
        pub heartbeat_interval: Option<String>,
        pub memory: Option<String>,
        pub security: Option<String>,
    }

    impl std::fmt::Display for ValidationError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let mut errors = Vec::new();
            for server in &self.mcp_servers {
                errors.push(format!("MCP server URL '{}' is not a valid URL", server));
            }
            if let Some(ref path) = self.soul_path {
                errors.push(format!("soul_path '{}' does not exist", path));
            }
            if let Some(ref path) = self.workspace {
                errors.push(format!("workspace '{}' cannot be created", path));
            }
            if let Some(ref msg) = self.rate_limit {
                errors.push(msg.clone());
            }
            if let Some(ref msg) = self.heartbeat_interval {
                errors.push(msg.clone());
            }
            if let Some(ref msg) = self.memory {
                errors.push(msg.clone());
            }
            if let Some(ref msg) = self.security {
                errors.push(msg.clone());
            }
            write!(f, "{}", errors.join("; "))
        }
    }

    impl std::error::Error for ValidationError {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_config_parses_documented_contract_shape() {
        let config: AppConfig = toml::from_str(
            r#"
            [security]
            wasm_sandbox = true
            wasm_max_instances = 10
            command_blocklist = true
            extra_blocked = ["^custom_dangerous_command"]
            allowed_commands = ["^git status$", "^ls( .*)?$"]
            prompt_injection_defense = true
            injection_action = "sanitize"
            leak_detection = true
            leak_action = "warn"
            secrets_encryption = true
            secrets_path = ".local/data/secrets.enc"

            [security.pairing]
            enabled = true
            code_length = 6
            expiry_seconds = 300
            "#,
        )
        .unwrap();

        assert!(config.security.command_blocklist);
        assert_eq!(
            config.security.extra_blocked,
            vec!["^custom_dangerous_command"]
        );
        assert_eq!(
            config.security.allowed_commands,
            vec!["^git status$", "^ls( .*)?$"]
        );
        assert!(matches!(
            config.security.injection_action,
            InjectionAction::Sanitize
        ));
        assert!(config.security.secret_leak_detection);
        assert_eq!(config.security.leak_action, LeakAction::Warn);
        assert_eq!(
            config.security.secrets_path,
            PathBuf::from(".local/data/secrets.enc")
        );
        assert!(config.security.pairing.enabled);
        assert_eq!(config.security.pairing.code_length, 6);
        assert_eq!(config.security.pairing.expiry_seconds, 300);
    }

    #[test]
    fn security_workspace_policy_parses_additive_contract_shape() {
        let config: AppConfig = toml::from_str(
            r#"
            [security.workspace]
            workspace_only = false
            allowed_roots = ["/tmp", "/var/tmp"]
            forbidden_paths = ["secrets", "/etc"]

            [security.docker]
            enabled = true
            image = "borgclaw-sandbox:base"
            network = "bridge"
            workspace_mount = "rw"
            read_only_rootfs = true
            tmpfs = true
            memory_limit_mb = 768
            cpu_limit = "1.5"
            timeout_seconds = 90
            allowed_tools = ["execute_command"]
            allowed_roots = ["/tmp"]
            extra_env_allowlist = ["PATH", "HOME", "LANG"]
            "#,
        )
        .unwrap();

        assert!(!config.security.workspace.workspace_only);
        assert_eq!(
            config.security.workspace.allowed_roots,
            vec![PathBuf::from("/tmp"), PathBuf::from("/var/tmp")]
        );
        assert_eq!(
            config.security.workspace.forbidden_paths,
            vec![PathBuf::from("secrets"), PathBuf::from("/etc")]
        );
        assert!(config.security.docker.enabled);
        assert_eq!(config.security.docker.image, "borgclaw-sandbox:base");
        assert_eq!(config.security.docker.network, DockerNetworkPolicy::Bridge);
        assert_eq!(
            config.security.docker.workspace_mount,
            DockerWorkspaceMount::ReadWrite
        );
        assert_eq!(config.security.docker.memory_limit_mb, 768);
        assert_eq!(config.security.docker.cpu_limit.as_deref(), Some("1.5"));
        assert_eq!(
            config.security.docker.allowed_roots,
            vec![PathBuf::from("/tmp")]
        );
        assert_eq!(
            config.security.docker.extra_env_allowlist,
            vec!["PATH", "HOME", "LANG"]
        );
    }

    #[test]
    fn vault_config_parses_documented_cli_contract_shape() {
        let config: AppConfig = toml::from_str(
            r#"
            [security.vault]
            provider = "bitwarden"

            [security.vault.bitwarden]
            cli_path = "bw"
            session_env = "BW_SESSION"

            [security.vault.1password]
            cli_path = "op"
            account = "my-account"
            vault = "Private"
            "#,
        )
        .unwrap();

        assert_eq!(config.security.vault.provider.as_deref(), Some("bitwarden"));
        assert_eq!(
            config.security.vault.bitwarden.cli_path,
            PathBuf::from("bw")
        );
        assert_eq!(config.security.vault.bitwarden.session_env, "BW_SESSION");
        assert_eq!(
            config.security.vault.one_password.cli_path,
            PathBuf::from("op")
        );
        assert_eq!(
            config.security.vault.one_password.account.as_deref(),
            Some("my-account")
        );
        assert_eq!(
            config.security.vault.one_password.vault.as_deref(),
            Some("Private")
        );
    }

    #[test]
    fn memory_and_heartbeat_config_parse_documented_contract_shape() {
        let config: AppConfig = toml::from_str(
            r#"
            [memory]
            database_path = ".local/data/memory.db"
            hybrid_search = true
            session_max_entries = 100
            session_keep_recent = 20
            session_keep_important = true

            [heartbeat]
            enabled = true
            check_interval_seconds = 60
            "#,
        )
        .unwrap();

        assert_eq!(
            config.memory.database_path,
            PathBuf::from(".local/data/memory.db")
        );
        assert!(config.memory.embedding_endpoint.is_none());
        assert!(config.memory.hybrid_search);
        assert_eq!(config.memory.session_max_entries, 100);
        assert_eq!(config.memory.session_keep_recent, 20);
        assert!(config.memory.session_keep_important);
        assert!(config.heartbeat.enabled);
        assert_eq!(config.heartbeat.check_interval_seconds, 60);
    }

    #[test]
    fn memory_config_accepts_legacy_aliases() {
        let config: AppConfig = toml::from_str(
            r#"
            [memory]
            memory_path = ".borgclaw/memory"
            session_compaction_threshold = 50
            "#,
        )
        .unwrap();

        assert_eq!(
            config.memory.database_path,
            PathBuf::from(".borgclaw/memory")
        );
        assert_eq!(config.memory.session_max_entries, 50);
    }

    #[test]
    fn memory_config_parses_embedding_endpoint_for_pgvector() {
        let config: AppConfig = toml::from_str(
            r#"
            [memory]
            backend = "postgres"
            connection_string = "postgres://localhost/borgclaw"
            embedding_endpoint = "http://127.0.0.1:9000/embed"
            hybrid_search = true
            "#,
        )
        .unwrap();

        assert_eq!(config.memory.effective_backend(), MemoryBackend::Postgres);
        assert_eq!(
            config.memory.embedding_endpoint.as_deref(),
            Some("http://127.0.0.1:9000/embed")
        );
        assert!(config.memory.hybrid_search);
    }

    #[test]
    fn memory_config_parses_external_adapter_contract() {
        let config: AppConfig = toml::from_str(
            r#"
            [memory.external]
            enabled = true
            endpoint = "http://127.0.0.1:8765"
            mirror_writes = false
            timeout_seconds = 30
            "#,
        )
        .unwrap();

        assert!(config.memory.external.enabled);
        assert_eq!(
            config.memory.external.endpoint.as_deref(),
            Some("http://127.0.0.1:8765")
        );
        assert!(!config.memory.external.mirror_writes);
        assert_eq!(config.memory.external.timeout_seconds, 30);
    }

    #[test]
    fn channel_config_parses_documented_aliases() {
        let config: AppConfig = toml::from_str(
            r#"
            [channels.telegram]
            enabled = true
            token = "telegram-token"
            dm_policy = "blocked"

            [channels.signal]
            enabled = true
            phone_number = "+1234567890"

            [channels.websocket]
            enabled = true
            port = 18789
            require_pairing = true
            "#,
        )
        .unwrap();

        let telegram = config.channels.get("telegram").unwrap();
        assert_eq!(telegram.credentials.as_deref(), Some("telegram-token"));
        assert!(matches!(telegram.dm_policy, DmPolicy::Blocked));

        let signal = config.channels.get("signal").unwrap();
        assert_eq!(
            signal
                .extra
                .get("phone_number")
                .and_then(|value| value.as_str()),
            Some("+1234567890")
        );

        let websocket = config.channels.get("websocket").unwrap();
        assert_eq!(
            websocket
                .extra
                .get("port")
                .and_then(|value| value.as_integer()),
            Some(18789)
        );
        assert_eq!(
            websocket
                .extra
                .get("require_pairing")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn skills_config_parses_documented_skill_sections() {
        let config: AppConfig = toml::from_str(
            r#"
            [skills.github]
            token = "${GITHUB_TOKEN}"
            user_agent = "BorgClaw/1.0"
            base_url = "https://api.github.com"

            [skills.github.safety]
            repo_access = "owned_only"
            require_confirmation = true

            [skills.google]
            client_id = "${GOOGLE_CLIENT_ID}"
            client_secret = "${GOOGLE_CLIENT_SECRET}"
            redirect_uri = "http://localhost:8080/callback"
            token_path = ".local/data/google_token.json"

            [skills.browser]
            browser = "chromium"
            headless = true
            bridge_path = ".local/tools/playwright/playwright-bridge.js"
            node_path = "node"
            cdp_url = "http://localhost:9222"

            [skills.stt]
            backend = "whispercpp"

            [skills.stt.openai]
            api_key = "${OPENAI_API_KEY}"
            model = "whisper-1"

            [skills.stt.openwebui]
            base_url = "http://localhost:3000"
            api_key = "${OPENWEBUI_API_KEY}"
            model = "whisper-1"

            [skills.stt.whispercpp]
            binary_path = ".local/tools/whisper.cpp/build/bin/whisper-cli"
            model_path = ".local/tools/whisper.cpp/models/ggml-base.en.bin"

            [skills.tts]
            provider = "elevenlabs"
            api_key = "${ELEVENLABS_API_KEY}"
            voice_id = "21m00Tcm4TlvDq8ikWAM"
            model_id = "eleven_monolingual_v1"

            [skills.image]
            provider = "stable_diffusion"

            [skills.image.dalle]
            api_key = "${OPENAI_API_KEY}"
            model = "dall-e-3"
            size = "1024x1024"

            [skills.image.stable_diffusion]
            base_url = "http://localhost:7860"
            api_type = "automatic1111"

            [skills.url_shortener]
            provider = "yourls"

            [skills.url_shortener.yourls]
            base_url = "https://your-domain.com/yourls-api.php"
            username = "admin"
            password = "${YOURLS_PASSWORD}"
            "#,
        )
        .unwrap();

        assert_eq!(config.skills.github.user_agent, "BorgClaw/1.0");
        assert_eq!(
            config.skills.google.redirect_uri,
            "http://localhost:8080/callback"
        );
        assert_eq!(
            config.skills.browser.browser,
            crate::skills::browser::BrowserType::Chromium
        );
        assert_eq!(config.skills.stt.backend, "whispercpp");
        assert_eq!(config.skills.tts.provider, "elevenlabs");
        assert_eq!(config.skills.image.provider, "stable_diffusion");
        assert_eq!(config.skills.url_shortener.provider, "yourls");
        assert_eq!(
            config.skills.url_shortener.yourls.base_url,
            "https://your-domain.com/yourls-api.php"
        );
    }

    #[test]
    fn config_validation_rejects_invalid_mcp_urls() {
        let mut config = AppConfig::default();
        config.mcp.servers.insert(
            "test".to_string(),
            crate::config::McpServerConfig {
                url: Some("not-a-valid-url".to_string()),
                transport: "stdio".to_string(),
                command: Some("echo".to_string()),
                args: Vec::new(),
                env: std::collections::HashMap::new(),
                headers: std::collections::HashMap::new(),
            },
        );

        let result = config.validate();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(!error.mcp_servers.is_empty());
    }

    #[test]
    fn config_validation_rejects_zero_heartbeat_interval() {
        let mut config = AppConfig::default();
        config.heartbeat.check_interval_seconds = 0;

        let result = config.validate();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.heartbeat_interval.is_some());
    }

    #[test]
    fn config_validation_rejects_enabled_docker_without_image() {
        let mut config = AppConfig::default();
        config.security.docker.enabled = true;
        config.security.docker.image.clear();

        let result = config.validate();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.security.is_some());
    }

    #[test]
    fn config_validation_accepts_valid_config() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn security_config_ssrf_defaults_are_correct() {
        let config = SecurityConfig::default();
        assert!(config.ssrf_protection);
        assert!(config.ssrf_allowlist.is_empty());
        assert!(config.ssrf_blocklist.is_empty());
    }

    #[test]
    fn security_config_parses_ssrf_options() {
        let config: AppConfig = toml::from_str(
            r#"
            [security]
            ssrf_protection = true
            ssrf_allowlist = ["^trusted\\.internal\\.example\\.com$"]
            ssrf_blocklist = ["^evil\\.example\\.com$", "^malware\\.example\\.com$"]
            "#,
        )
        .unwrap();

        assert!(config.security.ssrf_protection);
        assert_eq!(config.security.ssrf_allowlist.len(), 1);
        assert_eq!(
            config.security.ssrf_allowlist[0],
            "^trusted\\.internal\\.example\\.com$"
        );
        assert_eq!(config.security.ssrf_blocklist.len(), 2);
    }

    #[test]
    fn security_config_parses_ssrf_disabled() {
        let config: AppConfig = toml::from_str(
            r#"
            [security]
            ssrf_protection = false
            "#,
        )
        .unwrap();

        assert!(!config.security.ssrf_protection);
    }

    #[test]
    fn agent_config_minimax_roundtrips_correctly() {
        // Create config with MiniMax settings
        let config = AppConfig {
            agent: AgentConfig {
                provider: "minimax".to_string(),
                model: "MiniMax-M2.7".to_string(),
                max_tokens: 4096,
                temperature: 0.7,
                ..Default::default()
            },
            ..Default::default()
        };

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&config).unwrap();

        // Verify the model is in the TOML
        assert!(toml_str.contains("model = \"MiniMax-M2.7\""));
        assert!(toml_str.contains("provider = \"minimax\""));

        // Deserialize back
        let loaded: AppConfig = toml::from_str(&toml_str).unwrap();

        // Verify the model is preserved
        assert_eq!(loaded.agent.provider, "minimax");
        assert_eq!(loaded.agent.model, "MiniMax-M2.7");
    }

    #[test]
    fn agent_config_parses_minimax_from_toml() {
        let config: AppConfig = toml::from_str(
            r#"
            [agent]
            provider = "minimax"
            provider_profile = "work"
            identity_format = "aieos"
            model = "MiniMax-M2.7"
            max_tokens = 4096
            temperature = 0.7
            "#,
        )
        .unwrap();

        assert_eq!(config.agent.provider, "minimax");
        assert_eq!(config.agent.provider_profile.as_deref(), Some("work"));
        assert_eq!(config.agent.identity_format, IdentityFormat::Aieos);
        assert_eq!(config.agent.model, "MiniMax-M2.7");
        assert_eq!(config.agent.max_tokens, 4096);
        assert_eq!(config.agent.temperature, 0.7);
    }

    #[test]
    fn normalize_model_name_m277() {
        assert_eq!(normalize_model_name("m2.77"), "MiniMax-M2.7");
    }

    #[test]
    fn normalize_model_name_m25() {
        assert_eq!(normalize_model_name("m2.5"), "MiniMax-M2.5");
    }

    #[test]
    fn normalize_model_name_m27() {
        assert_eq!(normalize_model_name("m2.7"), "MiniMax-M2.7");
    }

    #[test]
    fn normalize_model_name_k25() {
        assert_eq!(normalize_model_name("k2.5"), "kimi-k2.5");
    }

    #[test]
    fn normalize_model_name_unchanged() {
        assert_eq!(normalize_model_name("MiniMax-M2.7"), "MiniMax-M2.7");
        assert_eq!(normalize_model_name("gpt-4o"), "gpt-4o");
        assert_eq!(normalize_model_name("claude-sonnet-4"), "claude-sonnet-4");
    }
}
