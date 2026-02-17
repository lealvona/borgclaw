//! Configuration module for BorgClaw

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Agent configuration
    pub agent: AgentConfig,
    /// Channel configurations
    pub channels: HashMap<String, ChannelConfig>,
    /// Security settings
    pub security: SecurityConfig,
    /// Memory settings
    pub memory: MemoryConfig,
    /// Scheduler settings
    pub scheduler: SchedulerConfig,
    /// Skills configuration
    pub skills: SkillsConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            agent: AgentConfig::default(),
            channels: HashMap::new(),
            security: SecurityConfig::default(),
            memory: MemoryConfig::default(),
            scheduler: SchedulerConfig::default(),
            skills: SkillsConfig::default(),
        }
    }
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct ChannelConfig {
    /// Enable this channel
    pub enabled: bool,
    /// Bot token / credentials
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
    Closed,
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
pub struct SecurityConfig {
    /// Enable WASM sandbox
    pub wasm_sandbox: bool,
    /// Enable Docker sandbox
    pub docker_sandbox: bool,
    /// Command blocklist (regex patterns)
    pub command_blocklist: Vec<String>,
    /// Execution approval mode
    pub approval_mode: ApprovalMode,
    /// Pairing code length
    pub pairing_code_length: usize,
    /// Pairing code expiry seconds
    pub pairing_code_expiry: u64,
    /// Enable prompt injection defense
    pub prompt_injection_defense: bool,
    /// Enable secret leak detection
    pub secret_leak_detection: bool,
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
            docker_sandbox: false,
            command_blocklist: vec![
                r"rm\s+-rf\s+/".to_string(),
                r"dd\s+if=".to_string(),
                r"mkfs".to_string(),
                r"format".to_string(),
                r"shutdown".to_string(),
                r"reboot".to_string(),
                r"poweroff".to_string(),
            ],
            approval_mode: ApprovalMode::Autonomous,
            pairing_code_length: 6,
            pairing_code_expiry: 300,
            prompt_injection_defense: true,
            secret_leak_detection: true,
        }
    }
}

/// Memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Enable hybrid search
    pub hybrid_search: bool,
    /// Vector store provider
    pub vector_provider: String,
    /// Embedding model
    pub embedding_model: String,
    /// Max entries in session before compaction
    pub session_compaction_threshold: usize,
    /// Memory file path
    pub memory_path: PathBuf,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            hybrid_search: true,
            vector_provider: "sqlite".to_string(),
            embedding_model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            session_compaction_threshold: 50,
            memory_path: PathBuf::from(".borgclaw/memory"),
        }
    }
}

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
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
