//! BorgClaw constants - centralized default values
//!
//! This module provides centralized constants for version strings and other
//! default values used throughout the codebase.

/// Default version for skills
pub const DEFAULT_SKILL_VERSION: &str = "1.0.0";

/// Default version for plugins
pub const DEFAULT_PLUGIN_VERSION: &str = "1.0.0";

/// Default version for tools
pub const DEFAULT_TOOL_VERSION: &str = "1.0.0";

/// Default version for manifests
pub const DEFAULT_MANIFEST_VERSION: &str = "1.0.0";

/// Current BorgClaw version (synced with Cargo.toml)
pub const BORGCLAW_VERSION: &str = env!("CARGO_PKG_VERSION");

/// BorgClaw name
pub const BORGCLAW_NAME: &str = "BorgClaw";

/// BorgClaw description
pub const BORGCLAW_DESCRIPTION: &str = "Personal AI Agent Framework";

/// Default workspace directory
pub const DEFAULT_WORKSPACE: &str = ".borgclaw/workspace";

/// Default config directory
pub const DEFAULT_CONFIG_DIR: &str = ".config/borgclaw";

/// Default session max entries
pub const DEFAULT_SESSION_MAX_ENTRIES: usize = 1000;

/// Default session keep recent
pub const DEFAULT_SESSION_KEEP_RECENT: usize = 4;

/// Default temperature
pub const DEFAULT_TEMPERATURE: f32 = 0.7;

/// Default max tokens
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Default rate limit (requests per minute)
pub const DEFAULT_RATE_LIMIT_RPM: u32 = 60;

/// Default WebSocket port
pub const DEFAULT_WEBSOCKET_PORT: u16 = 3000;

/// Default webhook port
pub const DEFAULT_WEBHOOK_PORT: u16 = 8080;

/// Default pairing code length
pub const DEFAULT_PAIRING_CODE_LENGTH: usize = 6;

/// Default pairing expiry (seconds)
pub const DEFAULT_PAIRING_EXPIRY_SECONDS: u64 = 300;

/// Default heartbeat interval (minutes)
pub const DEFAULT_HEARTBEAT_INTERVAL_MINUTES: u32 = 30;

/// Default heartbeat check interval (seconds)
pub const DEFAULT_HEARTBEAT_CHECK_INTERVAL_SECONDS: u64 = 60;

/// Default scheduler enabled
pub const DEFAULT_SCHEDULER_ENABLED: bool = true;

/// Default skills auto-load
pub const DEFAULT_SKILLS_AUTO_LOAD: bool = true;

/// Default memory hybrid search
pub const DEFAULT_MEMORY_HYBRID_SEARCH: bool = true;

/// Default WASM sandbox enabled
pub const DEFAULT_WASM_SANDBOX: bool = true;

/// Default command blocklist enabled
pub const DEFAULT_COMMAND_BLOCKLIST: bool = true;

/// Default prompt injection defense
pub const DEFAULT_PROMPT_INJECTION_DEFENSE: bool = true;

/// Default secret leak detection
pub const DEFAULT_SECRET_LEAK_DETECTION: bool = true;

/// Default secrets encryption
pub const DEFAULT_SECRETS_ENCRYPTION: bool = false;

/// Default SSRF protection
pub const DEFAULT_SSRF_PROTECTION: bool = true;

/// Maximum tool iterations in agent loop
pub const MAX_TOOL_ITERATIONS: usize = 5;

/// Default model
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";

/// Default provider
pub const DEFAULT_PROVIDER: &str = "anthropic";
