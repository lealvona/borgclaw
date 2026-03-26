//! Security module - WASM sandbox, pairing, secrets, blocklist, audit

mod audit;
mod pairing;
mod secrets;
mod vault;
mod wasm;

pub use audit::{AuditConfig, AuditEntry, AuditError, AuditEventType, AuditLogger};
pub use pairing::PairingManager;
pub use secrets::{SecretStore, SecretStoreConfig};
pub use vault::{
    BitwardenClient, BitwardenConfig, OnePasswordClient, OnePasswordConfig, VaultClient,
    VaultError, VaultItem, VaultItemType,
};
pub use wasm::WasmSandbox;

use super::config::{ApprovalMode, InjectionAction, LeakAction, SecurityConfig};
use chrono::{Duration, Utc};
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub const INJECTION_PATTERNS: &[&str] = &[
    r"(?i)ignore.*(previous|above|prior).*(instruction|directive)",
    r"(?i)forget.*(everything|all).*(instruction|rule)",
    r"(?i)new.*(instruction|rule|directive).*(you|your)",
    r"(?i)system.*:",
    r"(?i)<\|.*\|>",
    r"(?i)assistant.*role",
];

pub const BLOCKED_COMMANDS: &[&str] = &[
    r"^rm\s+-rf\s+/",
    r"^rm\s+-rf\s+~",
    r"^mkfs",
    r"^dd\s+if=",
    r"^:\(\)\{.*\|.*&\};:",
    r"^chmod\s+777",
    r"^chown\s+.*:.*\s+/",
    r"^shutdown",
    r"^reboot",
    r"^halt",
    r"^init\s+[06]",
    r"^poweroff",
    r"^format",
];

pub const SECRET_PATTERNS: &[(&str, &str)] = &[
    ("OpenAI API Key", r"sk-[a-zA-Z0-9]{20,}"),
    ("GitHub Token", r"ghp_[a-zA-Z0-9]{36}"),
    ("GitHub OAuth Token", r"gho_[a-zA-Z0-9]{36}"),
    ("GitLab Token", r"glpat-[a-zA-Z0-9\-]{20,}"),
    ("Google API Key", r"AIza[0-9A-Za-z\-_]{35}"),
    ("Slack Token", r"xox[baprs]-[0-9a-zA-Z]{10,48}"),
];

pub struct InjectionDefender {
    patterns: Vec<Regex>,
}

impl InjectionDefender {
    pub fn new() -> Self {
        Self {
            patterns: INJECTION_PATTERNS
                .iter()
                .filter_map(|pattern| Regex::new(pattern).ok())
                .collect(),
        }
    }

    pub fn detect(&self, content: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.is_match(content))
    }

    pub fn sanitize(&self, content: &str) -> String {
        sanitize_injection_input(content)
    }
}

impl Default for InjectionDefender {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CommandBlocklist {
    patterns: Vec<Regex>,
}

impl CommandBlocklist {
    pub fn new() -> Self {
        Self {
            patterns: BLOCKED_COMMANDS
                .iter()
                .filter_map(|pattern| Regex::new(pattern).ok())
                .collect(),
        }
    }

    pub fn is_blocked(&self, command: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.is_match(command))
    }
}

impl Default for CommandBlocklist {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeakMatch {
    pub pattern_name: String,
    pub value: String,
}

pub struct LeakDetector {
    patterns: Vec<(String, Regex)>,
}

impl LeakDetector {
    pub fn new() -> Self {
        Self {
            patterns: SECRET_PATTERNS
                .iter()
                .filter_map(|(name, pattern)| {
                    Regex::new(pattern)
                        .ok()
                        .map(|regex| ((*name).to_string(), regex))
                })
                .collect(),
        }
    }

    pub fn scan(&self, content: &str) -> Vec<LeakMatch> {
        let mut matches = Vec::new();
        for (name, pattern) in &self.patterns {
            for capture in pattern.find_iter(content) {
                matches.push(LeakMatch {
                    pattern_name: name.clone(),
                    value: capture.as_str().to_string(),
                });
            }
        }
        matches
    }

    pub fn redact(&self, content: &str) -> String {
        let mut redacted = content.to_string();
        for leak in self.scan(content) {
            redacted = redacted.replace(&leak.value, "[REDACTED_SECRET]");
        }
        redacted
    }
}

impl Default for LeakDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// SSRF (Server-Side Request Forgery) protection
/// Validates URLs to prevent requests to internal/private networks
#[derive(Debug, Clone)]
pub struct SsrfGuard {
    /// Allow localhost/loopback addresses (default: false)
    allow_localhost: bool,
    /// Allow private IP ranges (default: false)
    allow_private_ips: bool,
    /// Additional allowed host patterns
    allowed_hosts: Vec<Regex>,
    /// Blocked host patterns
    blocked_hosts: Vec<Regex>,
}

impl SsrfGuard {
    pub fn new() -> Self {
        Self {
            allow_localhost: false,
            allow_private_ips: false,
            allowed_hosts: Vec::new(),
            blocked_hosts: Vec::new(),
        }
    }

    pub fn with_localhost(mut self, allow: bool) -> Self {
        self.allow_localhost = allow;
        self
    }

    pub fn with_private_ips(mut self, allow: bool) -> Self {
        self.allow_private_ips = allow;
        self
    }

    /// Validate a URL for SSRF vulnerabilities
    pub fn validate_url(&self, url: &str) -> Result<(), SsrfError> {
        let parsed = url::Url::parse(url)
            .map_err(|e| SsrfError::InvalidUrl(e.to_string()))?;

        let host = parsed.host_str()
            .ok_or(SsrfError::MissingHost)?;

        // Check if explicitly blocked
        for pattern in &self.blocked_hosts {
            if pattern.is_match(host) {
                return Err(SsrfError::BlockedHost(host.to_string()));
            }
        }

        // Check if explicitly allowed (overrides other checks)
        for pattern in &self.allowed_hosts {
            if pattern.is_match(host) {
                return Ok(());
            }
        }

        // Check for localhost variants
        if !self.allow_localhost {
            if Self::is_localhost(host) {
                return Err(SsrfError::LocalhostNotAllowed);
            }
        }

        // Check for private IP ranges
        if !self.allow_private_ips {
            if Self::is_private_ip(host)? {
                return Err(SsrfError::PrivateIpNotAllowed);
            }
        }

        Ok(())
    }

    /// Check if host is localhost or loopback
    fn is_localhost(host: &str) -> bool {
        let lower = host.to_lowercase();
        lower == "localhost"
            || lower == "127.0.0.1"
            || lower == "::1"
            || lower == "0:0:0:0:0:0:0:1"
            || lower.starts_with("127.")
    }

    /// Check if host is a private/internal IP
    fn is_private_ip(host: &str) -> Result<bool, SsrfError> {
        // Try to parse as IP address
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            return Ok(Self::is_ip_private(&ip));
        }

        // Check for common private IP patterns in hostnames
        // 10.x.x.x
        if host.starts_with("10.") {
            return Ok(true);
        }

        // 172.16-31.x.x
        if let Some(rest) = host.strip_prefix("172.") {
            if let Some(dot_pos) = rest.find('.') {
                if let Ok(second_octet) = rest[..dot_pos].parse::<u8>() {
                    if (16..=31).contains(&second_octet) {
                        return Ok(true);
                    }
                }
            }
        }

        // 192.168.x.x
        if host.starts_with("192.168.") {
            return Ok(true);
        }

        // 169.254.x.x (link-local)
        if host.starts_with("169.254.") {
            return Ok(true);
        }

        // fc00::/7 (IPv6 unique local)
        if host.to_lowercase().starts_with("fc") || host.to_lowercase().starts_with("fd") {
            return Ok(true);
        }

        // fe80::/10 (IPv6 link-local)
        if host.to_lowercase().starts_with("fe8") {
            return Ok(true);
        }

        Ok(false)
    }

    /// Check if an IP address is private (for older Rust versions)
    fn is_ip_private(ip: &std::net::IpAddr) -> bool {
        match ip {
            std::net::IpAddr::V4(ipv4) => {
                let octets = ipv4.octets();
                // 10.0.0.0/8
                if octets[0] == 10 {
                    return true;
                }
                // 172.16.0.0/12
                if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                    return true;
                }
                // 192.168.0.0/16
                if octets[0] == 192 && octets[1] == 168 {
                    return true;
                }
                // 127.0.0.0/8 (loopback)
                if octets[0] == 127 {
                    return true;
                }
                // 169.254.0.0/16 (link-local)
                if octets[0] == 169 && octets[1] == 254 {
                    return true;
                }
                // 224.0.0.0/4 (multicast)
                if octets[0] >= 224 && octets[0] <= 239 {
                    return true;
                }
                // 0.0.0.0 (unspecified)
                if octets == [0, 0, 0, 0] {
                    return true;
                }
                false
            }
            std::net::IpAddr::V6(ipv6) => {
                let octets = ipv6.octets();
                // fc00::/7 (unique local)
                if (octets[0] & 0xfe) == 0xfc {
                    return true;
                }
                // fe80::/10 (link-local)
                if octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80 {
                    return true;
                }
                // ::1 (loopback)
                if octets == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1] {
                    return true;
                }
                // :: (unspecified)
                if octets == [0; 16] {
                    return true;
                }
                // ff00::/8 (multicast)
                if octets[0] == 0xff {
                    return true;
                }
                false
            }
        }
    }

    /// Add an allowed host pattern
    pub fn allow_host(&mut self, pattern: &str) -> Result<(), regex::Error> {
        let regex = Regex::new(pattern)?;
        self.allowed_hosts.push(regex);
        Ok(())
    }

    /// Add a blocked host pattern
    pub fn block_host(&mut self, pattern: &str) -> Result<(), regex::Error> {
        let regex = Regex::new(pattern)?;
        self.blocked_hosts.push(regex);
        Ok(())
    }
}

impl Default for SsrfGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum SsrfError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("URL missing host")]
    MissingHost,
    #[error("Blocked host: {0}")]
    BlockedHost(String),
    #[error("Localhost/loopback addresses not allowed")]
    LocalhostNotAllowed,
    #[error("Private/internal IP addresses not allowed")]
    PrivateIpNotAllowed,
}

/// Security layer - combines all security features
pub struct SecurityLayer {
    config: SecurityConfig,
    command_allowlist: Vec<Regex>,
    command_blocklist: Vec<Regex>,
    pairing: Arc<RwLock<PairingManager>>,
    secrets: Arc<RwLock<SecretStore>>,
    approvals: Arc<RwLock<HashMap<String, PendingApproval>>>,
    vault: Option<Arc<dyn VaultClient>>,
    wasm: Option<WasmSandbox>,
    ssrf_guard: SsrfGuard,
}

#[derive(Debug, Clone)]
struct PendingApproval {
    tool_name: String,
    expires_at: chrono::DateTime<Utc>,
    approved: bool,
}

impl SecurityLayer {
    pub fn new() -> Self {
        let config = SecurityConfig::default();

        Self {
            config: config.clone(),
            command_allowlist: compile_allowlist(&config),
            command_blocklist: compile_blocklist(&config),
            pairing: Arc::new(RwLock::new(PairingManager::new(
                config.pairing.code_length,
                config.pairing.expiry_seconds,
            ))),
            secrets: Arc::new(RwLock::new(SecretStore::with_config(secret_store_config(
                &config,
            )))),
            approvals: Arc::new(RwLock::new(HashMap::new())),
            vault: None,
            wasm: config
                .wasm_sandbox
                .then(|| WasmSandbox::new(config.wasm_max_instances)),
            ssrf_guard: SsrfGuard::new(),
        }
    }

    pub fn with_config(config: SecurityConfig) -> Self {
        Self {
            config: config.clone(),
            command_allowlist: compile_allowlist(&config),
            command_blocklist: compile_blocklist(&config),
            pairing: Arc::new(RwLock::new(PairingManager::new(
                config.pairing.code_length,
                config.pairing.expiry_seconds,
            ))),
            secrets: Arc::new(RwLock::new(SecretStore::with_config(secret_store_config(
                &config,
            )))),
            approvals: Arc::new(RwLock::new(HashMap::new())),
            vault: configured_vault(&config),
            wasm: config
                .wasm_sandbox
                .then(|| WasmSandbox::new(config.wasm_max_instances)),
            ssrf_guard: SsrfGuard::new(),
        }
    }

    /// Check if command is allowed
    pub fn check_command(&self, command: &str) -> CommandCheck {
        if !self.command_allowlist.is_empty()
            && !self
                .command_allowlist
                .iter()
                .any(|pattern| pattern.is_match(command))
        {
            return CommandCheck::Blocked("not allowed by command allowlist".to_string());
        }
        for pattern in &self.command_blocklist {
            if pattern.is_match(command) {
                return CommandCheck::Blocked(pattern.to_string());
            }
        }
        CommandCheck::Allowed
    }

    /// Validate URL against SSRF attacks
    pub fn validate_url(&self, url: &str) -> Result<(), SsrfError> {
        self.ssrf_guard.validate_url(url)
    }

    /// Get a reference to the SSRF guard for custom configuration
    pub fn ssrf_guard(&self) -> &SsrfGuard {
        &self.ssrf_guard
    }

    /// Check pairing status for sender
    pub async fn check_pairing(&self, sender_id: &str) -> PairingStatus {
        if !self.config.pairing.enabled {
            return PairingStatus::Approved;
        }
        let pairing = self.pairing.read().await;
        pairing.check_sender(sender_id)
    }

    /// Generate pairing code
    pub async fn generate_pairing(&self, sender_id: &str) -> Result<String, SecurityError> {
        if !self.config.pairing.enabled {
            return Err(SecurityError::PairingError(
                "pairing is disabled".to_string(),
            ));
        }
        let mut pairing = self.pairing.write().await;
        pairing.generate_code(sender_id)
    }

    /// Approve pairing code
    pub async fn approve_pairing(&self, code: &str) -> Result<String, SecurityError> {
        if !self.config.pairing.enabled {
            return Err(SecurityError::PairingError(
                "pairing is disabled".to_string(),
            ));
        }
        let mut pairing = self.pairing.write().await;
        pairing.approve_code(code)
    }

    /// Store a secret
    pub async fn store_secret(&self, key: &str, value: &str) -> Result<(), SecurityError> {
        let secrets = self.secrets.write().await;
        secrets.store(key, value).await
    }

    /// Get a secret (injected into environment)
    pub async fn get_secret(&self, key: &str) -> Option<String> {
        let secrets = self.secrets.read().await;
        if let Some(value) = secrets.get(key).await {
            return Some(value);
        }
        drop(secrets);

        let vault = self.vault.as_ref()?;
        let value = vault.get_secret(key).await.ok()?;
        let secrets = self.secrets.write().await;
        let _ = secrets.store(key, &value).await;
        Some(value)
    }

    pub async fn secret_env(&self) -> HashMap<String, String> {
        let secrets = self.secrets.read().await;
        secrets.inject_env().await
    }

    pub fn vault_provider(&self) -> Option<&str> {
        self.config.vault.provider.as_deref()
    }

    pub fn configured_for_leak_detection(&self) -> bool {
        self.config.secret_leak_detection
    }

    pub fn leak_action(&self) -> &LeakAction {
        &self.config.leak_action
    }

    pub fn approval_mode(&self) -> &ApprovalMode {
        &self.config.approval_mode
    }

    pub async fn request_approval(&self, tool_name: &str) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let pending = PendingApproval {
            tool_name: tool_name.to_string(),
            expires_at: Utc::now() + Duration::minutes(5),
            approved: false,
        };
        let mut approvals = self.approvals.write().await;
        approvals.insert(token.clone(), pending);
        token
    }

    pub async fn approve_pending(&self, tool_name: &str, token: &str) -> Result<(), SecurityError> {
        let mut approvals = self.approvals.write().await;
        approvals.retain(|_, approval| approval.expires_at > Utc::now());
        let approval = approvals.get_mut(token).ok_or_else(|| {
            SecurityError::ApprovalError("invalid or expired approval token".to_string())
        })?;

        if approval.tool_name != tool_name {
            return Err(SecurityError::ApprovalError(
                "approval token does not match tool".to_string(),
            ));
        }

        approval.approved = true;

        Ok(())
    }

    pub async fn consume_approval(
        &self,
        tool_name: &str,
        token: &str,
    ) -> Result<(), SecurityError> {
        let mut approvals = self.approvals.write().await;
        approvals.retain(|_, approval| approval.expires_at > Utc::now());
        let approval = approvals.remove(token).ok_or_else(|| {
            SecurityError::ApprovalError("invalid or expired approval token".to_string())
        })?;

        if approval.tool_name != tool_name {
            return Err(SecurityError::ApprovalError(
                "approval token does not match tool".to_string(),
            ));
        }

        if !approval.approved {
            return Err(SecurityError::ApprovalError(
                "approval token has not been approved yet".to_string(),
            ));
        }

        Ok(())
    }

    /// Check for secret leaks in output
    pub fn check_leak(&self, content: &str) -> Vec<String> {
        LeakDetector::new()
            .scan(content)
            .into_iter()
            .map(|leak| leak.value)
            .collect()
    }

    pub fn redact_leaks(&self, content: &str) -> (String, usize) {
        let mut redacted = content.to_string();
        let leaks = self.check_leak(content);
        for leak in &leaks {
            redacted = redacted.replace(leak, "[REDACTED_SECRET]");
        }
        (redacted, leaks.len())
    }

    /// Check prompt for injection attempts
    pub fn check_prompt_injection(&self, content: &str) -> InjectionCheck {
        if !self.config.prompt_injection_defense {
            return InjectionCheck::Allowed;
        }

        let mut score = 0.0;
        for pattern in INJECTION_PATTERNS {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(content) {
                    score += 0.25;
                }
            }
        }

        if score == 0.0 {
            InjectionCheck::Allowed
        } else {
            match self.config.injection_action {
                InjectionAction::Block => InjectionCheck::Blocked,
                InjectionAction::Warn => InjectionCheck::Warning(score),
                InjectionAction::Sanitize => {
                    InjectionCheck::Sanitized(sanitize_injection_input(content))
                }
            }
        }
    }

    pub async fn load_secret_keys(&self) -> Vec<String> {
        let secrets = self.secrets.read().await;
        secrets.keys().await
    }

    pub fn wasm_max_instances(&self) -> usize {
        self.config.wasm_max_instances
    }

    pub fn secrets_path(&self) -> &std::path::Path {
        &self.config.secrets_path
    }

    pub fn injection_action(&self) -> &InjectionAction {
        &self.config.injection_action
    }

    pub fn pairing_enabled(&self) -> bool {
        self.config.pairing.enabled
    }

    pub fn pairing_settings(&self) -> &crate::config::PairingConfig {
        &self.config.pairing
    }

    pub fn secrets_encryption_enabled(&self) -> bool {
        self.config.secrets_encryption
    }

    pub fn has_wasm_sandbox(&self) -> bool {
        self.wasm.is_some()
    }

    /// Check execution approval requirement
    pub fn needs_approval(&self, tool_name: &str, mode: &ApprovalMode) -> bool {
        match mode {
            ApprovalMode::ReadOnly => true,
            ApprovalMode::Supervised => {
                // Only dangerous tools need approval
                matches!(
                    tool_name,
                    "execute_command" | "write_file" | "delete" | "plugin_invoke" | "mcp_call_tool"
                )
            }
            ApprovalMode::Autonomous => false,
        }
    }
}

fn configured_vault(config: &SecurityConfig) -> Option<Arc<dyn VaultClient>> {
    match config.vault.provider.as_deref() {
        Some("bitwarden") => Some(Arc::new(BitwardenClient::new(BitwardenConfig {
            cli_path: config.vault.bitwarden.cli_path.clone(),
            session_env: config.vault.bitwarden.session_env.clone(),
            server_url: config.vault.bitwarden.server_url.clone(),
            client_id: config.vault.bitwarden.client_id.clone(),
            client_secret: config.vault.bitwarden.client_secret.clone(),
            use_cli: config.vault.bitwarden.use_cli,
        }))),
        Some("1password") => Some(Arc::new(OnePasswordClient::new(OnePasswordConfig {
            cli_path: config.vault.one_password.cli_path.clone(),
            vault: config.vault.one_password.vault.clone(),
            account: config.vault.one_password.account.clone(),
        }))),
        _ => None,
    }
}

fn compile_blocklist(config: &SecurityConfig) -> Vec<Regex> {
    let mut patterns = Vec::new();
    if config.command_blocklist {
        patterns.extend(BLOCKED_COMMANDS.iter().copied());
    }
    patterns.extend(config.extra_blocked.iter().map(String::as_str));

    patterns
        .into_iter()
        .filter_map(|pattern| Regex::new(pattern).ok())
        .collect()
}

fn compile_allowlist(config: &SecurityConfig) -> Vec<Regex> {
    config
        .allowed_commands
        .iter()
        .filter_map(|pattern| Regex::new(pattern).ok())
        .collect()
}

fn secret_store_config(config: &SecurityConfig) -> SecretStoreConfig {
    SecretStoreConfig {
        encryption_enabled: config.secrets_encryption,
        secrets_path: Some(config.secrets_path.clone()),
    }
}

fn sanitize_injection_input(content: &str) -> String {
    INJECTION_PATTERNS
        .iter()
        .fold(content.to_string(), |acc, pattern| {
            Regex::new(pattern)
                .map(|re| re.replace_all(&acc, "[sanitized]").to_string())
                .unwrap_or(acc)
        })
}

/// Command check result
#[derive(Debug, Clone)]
pub enum CommandCheck {
    Allowed,
    Blocked(String),
}

/// Pairing status
#[derive(Debug, Clone)]
pub enum PairingStatus {
    Approved,
    Pending,
    Unknown,
}

/// Injection check result
#[derive(Debug, Clone)]
pub enum InjectionCheck {
    Allowed,
    Warning(f32),
    Sanitized(String),
    Blocked,
}

/// Security errors
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Pairing error: {0}")]
    PairingError(String),
    #[error("Secret error: {0}")]
    SecretError(String),
    #[error("Approval error: {0}")]
    ApprovalError(String),
    #[error("Vault error: {0}")]
    VaultError(String),
    #[error("WASM error: {0}")]
    WasmError(String),
    #[error("Prompt injection detected")]
    InjectionDetected,
    #[error("Blocked command")]
    BlockedCommand,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_secret_is_cached_and_injected_into_env() {
        let security = SecurityLayer::new();
        security
            .store_secret("api_key", "secret-value")
            .await
            .unwrap();

        assert_eq!(
            security.get_secret("api_key").await.as_deref(),
            Some("secret-value")
        );

        let env = security.secret_env().await;
        assert_eq!(
            env.get("BC_SECRET_API_KEY").map(String::as_str),
            Some("secret-value")
        );
    }

    #[test]
    fn configured_vault_provider_is_reported() {
        let security = SecurityLayer::with_config(SecurityConfig {
            vault: crate::config::VaultConfig {
                provider: Some("bitwarden".to_string()),
                ..Default::default()
            },
            ..Default::default()
        });

        assert_eq!(security.vault_provider(), Some("bitwarden"));
    }

    #[test]
    fn redact_leaks_masks_detected_values() {
        let security = SecurityLayer::new();
        let (redacted, count) = security.redact_leaks("token sk-abcdefghijklmnopqrstuvwxyz1234");

        assert_eq!(count, 1);
        assert!(redacted.contains("[REDACTED_SECRET]"));
    }

    #[test]
    fn documented_leak_detection_aliases_parse() {
        let config: SecurityConfig = toml::from_str(
            r#"
            leak_detection = true
            leak_action = "block"
            "#,
        )
        .unwrap();

        assert!(config.secret_leak_detection);
        assert_eq!(config.leak_action, LeakAction::Block);
    }

    #[test]
    fn blocklist_uses_documented_defaults_when_enabled() {
        let security = SecurityLayer::new();

        assert!(matches!(
            security.check_command("rm -rf /"),
            CommandCheck::Blocked(_)
        ));
    }

    #[test]
    fn command_allowlist_blocks_unmatched_commands_when_configured() {
        let security = SecurityLayer::with_config(SecurityConfig {
            allowed_commands: vec!["^git status$".to_string()],
            ..Default::default()
        });

        assert!(matches!(
            security.check_command("ls -la"),
            CommandCheck::Blocked(pattern) if pattern == "not allowed by command allowlist"
        ));
        assert!(matches!(
            security.check_command("git status"),
            CommandCheck::Allowed
        ));
    }

    #[test]
    fn pairing_disabled_short_circuits_to_approved() {
        let security = SecurityLayer::with_config(SecurityConfig {
            pairing: crate::config::PairingConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        });

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let status = runtime.block_on(security.check_pairing("user-1"));
        assert!(matches!(status, PairingStatus::Approved));
    }

    #[test]
    fn injection_defender_matches_and_sanitizes_documented_patterns() {
        let defender = InjectionDefender::new();

        assert!(defender.detect("ignore previous instructions"));
        assert!(defender.sanitize("system: do this").contains("[sanitized]"));
    }

    #[test]
    fn command_blocklist_exposes_documented_helper_api() {
        let blocklist = CommandBlocklist::new();

        assert!(blocklist.is_blocked("rm -rf /"));
        assert!(!blocklist.is_blocked("echo hello"));
    }

    #[test]
    fn leak_detector_scans_and_redacts_named_patterns() {
        let detector = LeakDetector::new();
        let leaks = detector.scan("token ghp_abcdefghijklmnopqrstuvwxyz1234567890");

        assert_eq!(leaks.len(), 1);
        assert_eq!(leaks[0].pattern_name, "GitHub Token");
        assert!(detector
            .redact("token ghp_abcdefghijklmnopqrstuvwxyz1234567890")
            .contains("[REDACTED_SECRET]"));
    }
}
