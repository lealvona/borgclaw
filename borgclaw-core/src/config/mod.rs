//! Configuration module for BorgClaw

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            agent: AgentConfig::default(),
            channels: HashMap::new(),
            security: SecurityConfig::default(),
            memory: MemoryConfig::default(),
            heartbeat: HeartbeatConfig::default(),
            scheduler: SchedulerConfig::default(),
            skills: SkillsConfig::default(),
            mcp: McpConfig::default(),
            registrar: RegistrarConfig::default(),
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
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            soul_path: None,
            workspace: PathBuf::from(".borgclaw/workspace"),
            heartbeat_interval: 30,
        }
    }
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
    /// Enable WASM sandbox
    pub wasm_sandbox: bool,
    /// Max registered WASM sandbox instances
    pub wasm_max_instances: usize,
    /// Enable Docker sandbox
    pub docker_sandbox: bool,
    /// Enable the default command blocklist
    pub command_blocklist: bool,
    /// Additional blocklist entries
    pub extra_blocked: Vec<String>,
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
            docker_sandbox: false,
            command_blocklist: true,
            extra_blocked: Vec::new(),
            approval_mode: ApprovalMode::Autonomous,
            pairing: PairingConfig::default(),
            prompt_injection_defense: true,
            injection_action: InjectionAction::Block,
            secret_leak_detection: true,
            leak_action: LeakAction::Redact,
            secrets_encryption: true,
            secrets_path: PathBuf::from(".borgclaw/secrets.enc"),
            vault: VaultConfig::default(),
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
    pub server_url: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub use_cli: bool,
}

impl Default for BitwardenVaultConfig {
    fn default() -> Self {
        Self {
            server_url: None,
            client_id: None,
            client_secret: None,
            use_cli: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct OnePasswordVaultConfig {
    pub vault: Option<String>,
    pub account: Option<String>,
}

/// Memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    /// SQLite database path
    #[serde(alias = "memory_path")]
    pub database_path: PathBuf,
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
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            database_path: PathBuf::from(".borgclaw/memory"),
            hybrid_search: true,
            vector_provider: "sqlite".to_string(),
            embedding_model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            session_max_entries: 100,
            session_keep_recent: 20,
            session_keep_important: true,
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
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            skills_path: PathBuf::from(".borgclaw/skills"),
            auto_load: true,
            registry_url: None,
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
pub fn load_config(path: &PathBuf) -> Result<AppConfig, config::ConfigError> {
    let content = std::fs::read_to_string(path)?;
    toml::from_str(&content).map_err(config::ConfigError::Parse)
}

/// Save configuration to file
pub fn save_config(config: &AppConfig, path: &PathBuf) -> Result<(), config::ConfigError> {
    let content = toml::to_string_pretty(config)?;
    std::fs::write(path, content)?;
    Ok(())
}

mod config {
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum ConfigError {
        #[error("IO error: {0}")]
        Io(#[from] std::io::Error),
        #[error("TOML parse error: {0}")]
        Parse(#[from] toml::de::Error),
        #[error("TOML serialize error: {0}")]
        Serialize(#[from] toml::ser::Error),
    }
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
}
