//! Security module - WASM sandbox, pairing, secrets, blocklist

mod pairing;
mod secrets;
mod wasm;

pub use pairing::PairingManager;
pub use secrets::SecretStore;
pub use wasm::WasmSandbox;

use super::config::{ApprovalMode, SecurityConfig};
use regex::Regex;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Security layer - combines all security features
pub struct SecurityLayer {
    config: SecurityConfig,
    command_blocklist: Vec<Regex>,
    pairing: Arc<RwLock<PairingManager>>,
    secrets: Arc<RwLock<SecretStore>>,
    wasm: Option<WasmSandbox>,
}

impl SecurityLayer {
    pub fn new() -> Self {
        let config = SecurityConfig::default();
        let command_blocklist = config
            .command_blocklist
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        
        Self {
            config,
            command_blocklist,
            pairing: Arc::new(RwLock::new(PairingManager::new(6, 300))),
            secrets: Arc::new(RwLock::new(SecretStore::new())),
            wasm: None,
        }
    }
    
    pub fn with_config(config: SecurityConfig) -> Self {
        let command_blocklist = config
            .command_blocklist
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        
        Self {
            config: config.clone(),
            command_blocklist,
            pairing: Arc::new(RwLock::new(PairingManager::new(
                config.pairing_code_length,
                config.pairing_code_expiry,
            ))),
            secrets: Arc::new(RwLock::new(SecretStore::new())),
            wasm: None,
        }
    }
    
    /// Check if command is allowed
    pub fn check_command(&self, command: &str) -> CommandCheck {
        for pattern in &self.command_blocklist {
            if pattern.is_match(command) {
                return CommandCheck::Blocked(pattern.to_string());
            }
        }
        CommandCheck::Allowed
    }
    
    /// Check pairing status for sender
    pub async fn check_pairing(&self, sender_id: &str) -> PairingStatus {
        let pairing = self.pairing.read().await;
        pairing.check_sender(sender_id)
    }
    
    /// Generate pairing code
    pub async fn generate_pairing(&self, sender_id: &str) -> Result<String, SecurityError> {
        let mut pairing = self.pairing.write().await;
        pairing.generate_code(sender_id)
    }
    
    /// Approve pairing code
    pub async fn approve_pairing(&self, code: &str) -> Result<String, SecurityError> {
        let mut pairing = self.pairing.write().await;
        pairing.approve_code(code)
    }
    
    /// Store a secret
    pub async fn store_secret(&self, key: &str, value: &str) -> Result<(), SecurityError> {
        let mut secrets = self.secrets.write().await;
        secrets.store(key, value).await
    }
    
    /// Get a secret (injected into environment)
    pub async fn get_secret(&self, key: &str) -> Option<String> {
        let secrets = self.secrets.read().await;
        secrets.get(key).await
    }
    
    /// Check for secret leaks in output
    pub fn check_leak(&self, content: &str) -> Vec<String> {
        // Simple pattern matching for API key formats
        let patterns = [
            r"sk-[a-zA-Z0-9]{20,}",
            r"ghp_[a-zA-Z0-9]{36}",
            r"gho_[a-zA-Z0-9]{36}",
            r"glpat-[a-zA-Z0-9\-]{20,}",
            r"AIza[0-9A-Za-z\-_]{35}",
            r"xox[baprs]-[0-9a-zA-Z]{10,48}",
        ];
        
        let mut leaks = Vec::new();
        for pattern in patterns {
            if let Ok(re) = Regex::new(pattern) {
                for cap in re.find_iter(content) {
                    leaks.push(cap.as_str().to_string());
                }
            }
        }
        leaks
    }
    
    /// Check prompt for injection attempts
    pub fn check_prompt_injection(&self, content: &str) -> InjectionCheck {
        if !self.config.prompt_injection_defense {
            return InjectionCheck::Allowed;
        }
        
        let injection_patterns = [
            r"(?i)ignore.*(previous|above|prior).*(instruction|directive)",
            r"(?i)forget.*(everything|all).*(instruction|rule)",
            r"(?i)new.*(instruction|rule|directive).*(you|your)",
            r"(?i)system.*:",
            r"(?i)<\|.*\|>",
            r"(?i)assistant.*role",
        ];
        
        let mut score = 0.0;
        for pattern in injection_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(content) {
                    score += 0.25;
                }
            }
        }
        
        if score >= 1.0 {
            InjectionCheck::Blocked
        } else if score > 0.0 {
            InjectionCheck::Warning(score)
        } else {
            InjectionCheck::Allowed
        }
    }
    
    /// Check execution approval requirement
    pub fn needs_approval(&self, tool_name: &str, mode: &ApprovalMode) -> bool {
        match mode {
            ApprovalMode::ReadOnly => true,
            ApprovalMode::Supervised => {
                // Only dangerous tools need approval
                matches!(tool_name, "execute_command" | "write_file" | "delete")
            }
            ApprovalMode::Autonomous => false,
        }
    }
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
    Blocked,
}

/// Security errors
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Pairing error: {0}")]
    PairingError(String),
    #[error("Secret error: {0}")]
    SecretError(String),
    #[error("WASM error: {0}")]
    WasmError(String),
}
