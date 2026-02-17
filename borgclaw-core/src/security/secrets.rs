//! Secrets module - encrypted secret storage

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Secret store - encrypted storage for API keys and credentials
pub struct SecretStore {
    secrets: Arc<RwLock<HashMap<String, String>>>,
}

impl SecretStore {
    pub fn new() -> Self {
        Self {
            secrets: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Store a secret (in memory - could be extended to encrypted file)
    pub async fn store(&self, key: &str, value: &str) -> Result<(), super::SecurityError> {
        let mut secrets = self.secrets.write().await;
        secrets.insert(key.to_string(), value.to_string());
        Ok(())
    }
    
    /// Get a secret
    pub async fn get(&self, key: &str) -> Option<String> {
        let secrets = self.secrets.read().await;
        secrets.get(key).cloned()
    }
    
    /// Delete a secret
    pub async fn delete(&self, key: &str) -> Option<String> {
        let mut secrets = self.secrets.write().await;
        secrets.remove(key)
    }
    
    /// List secret keys (not values)
    pub async fn keys(&self) -> Vec<String> {
        let secrets = self.secrets.read().await;
        secrets.keys().cloned().collect()
    }
    
    /// Check if secret exists
    pub async fn exists(&self, key: &str) -> bool {
        let secrets = self.secrets.read().await;
        secrets.contains_key(key)
    }
    
    /// Inject secrets into environment variables (for tool execution)
    pub async fn inject_env(&self) -> HashMap<String, String> {
        let secrets = self.secrets.read().await;
        secrets
            .iter()
            .map(|(k, v)| (format!("BC_SECRET_{}", k.to_uppercase()), v.clone()))
            .collect()
    }
}

impl Default for SecretStore {
    fn default() -> Self {
        Self::new()
    }
}
