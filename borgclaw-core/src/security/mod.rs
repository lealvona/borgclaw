//! Security module - WASM sandbox, pairing, secrets, blocklist

mod pairing;
mod secrets;
mod vault;
mod wasm;

pub use pairing::PairingManager;
pub use secrets::SecretStore;
pub use vault::{
    BitwardenClient, BitwardenConfig, OnePasswordClient, OnePasswordConfig, VaultClient,
    VaultError, VaultItem, VaultItemType,
};
pub use wasm::WasmSandbox;

use super::config::{ApprovalMode, SecurityConfig};
use chrono::{Duration, Utc};
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Security layer - combines all security features
pub struct SecurityLayer {
    config: SecurityConfig,
    command_blocklist: Vec<Regex>,
    pairing: Arc<RwLock<PairingManager>>,
    secrets: Arc<RwLock<SecretStore>>,
    approvals: Arc<RwLock<HashMap<String, PendingApproval>>>,
    vault: Option<Arc<dyn VaultClient>>,
    wasm: Option<WasmSandbox>,
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
            approvals: Arc::new(RwLock::new(HashMap::new())),
            vault: None,
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
            approvals: Arc::new(RwLock::new(HashMap::new())),
            vault: configured_vault(&config),
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
        let approval = approvals
            .get_mut(token)
            .ok_or_else(|| SecurityError::ApprovalError("invalid or expired approval token".to_string()))?;

        if approval.tool_name != tool_name {
            return Err(SecurityError::ApprovalError("approval token does not match tool".to_string()));
        }

        approval.approved = true;

        Ok(())
    }

    pub async fn consume_approval(&self, tool_name: &str, token: &str) -> Result<(), SecurityError> {
        let mut approvals = self.approvals.write().await;
        approvals.retain(|_, approval| approval.expires_at > Utc::now());
        let approval = approvals
            .remove(token)
            .ok_or_else(|| SecurityError::ApprovalError("invalid or expired approval token".to_string()))?;

        if approval.tool_name != tool_name {
            return Err(SecurityError::ApprovalError("approval token does not match tool".to_string()));
        }

        if !approval.approved {
            return Err(SecurityError::ApprovalError("approval token has not been approved yet".to_string()));
        }

        Ok(())
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

fn configured_vault(config: &SecurityConfig) -> Option<Arc<dyn VaultClient>> {
    match config.vault.provider.as_deref() {
        Some("bitwarden") => Some(Arc::new(BitwardenClient::new(BitwardenConfig {
            server_url: config.vault.bitwarden.server_url.clone(),
            client_id: config.vault.bitwarden.client_id.clone(),
            client_secret: config.vault.bitwarden.client_secret.clone(),
            use_cli: config.vault.bitwarden.use_cli,
        }))),
        Some("1password") => Some(Arc::new(OnePasswordClient::new(OnePasswordConfig {
            vault: config.vault.one_password.vault.clone(),
            account: config.vault.one_password.account.clone(),
        }))),
        _ => None,
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
    #[error("Approval error: {0}")]
    ApprovalError(String),
    #[error("Vault error: {0}")]
    VaultError(String),
    #[error("WASM error: {0}")]
    WasmError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_secret_is_cached_and_injected_into_env() {
        let security = SecurityLayer::new();
        security.store_secret("api_key", "secret-value").await.unwrap();

        assert_eq!(security.get_secret("api_key").await.as_deref(), Some("secret-value"));

        let env = security.secret_env().await;
        assert_eq!(env.get("BC_SECRET_API_KEY").map(String::as_str), Some("secret-value"));
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
}
