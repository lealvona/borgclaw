//! Secrets module - encrypted secret storage

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Service token constant - key for stored hash
pub const SERVICE_TOKEN_HASH_KEY: &str = "borgclaw_service_token_hash";
/// Service token file name
pub const SERVICE_TOKEN_FILE: &str = ".service_token";

#[derive(Debug, Clone)]
pub struct SecretStoreConfig {
    pub encryption_enabled: bool,
    pub secrets_path: Option<PathBuf>,
}

/// Secret store - encrypted storage for API keys and credentials
pub struct SecretStore {
    secrets: Arc<RwLock<HashMap<String, String>>>,
    config: SecretStoreConfig,
}

impl SecretStore {
    pub fn new() -> Self {
        Self::with_config(SecretStoreConfig {
            encryption_enabled: false,
            secrets_path: None,
        })
    }

    pub fn with_config(config: SecretStoreConfig) -> Self {
        let secrets = load_persisted_secrets(&config).unwrap_or_default();
        Self {
            secrets: Arc::new(RwLock::new(secrets)),
            config,
        }
    }

    /// Store a secret
    pub async fn store(&self, key: &str, value: &str) -> Result<(), super::SecurityError> {
        let mut secrets = self.secrets.write().await;
        secrets.insert(key.to_string(), value.to_string());
        persist_if_configured(&self.config, &secrets)?;
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
        let removed = secrets.remove(key);
        let _ = persist_if_configured(&self.config, &secrets);
        removed
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

    /// Verify a service token against the stored hash
    pub async fn verify_service_token(&self, token: &str) -> bool {
        let secrets = self.secrets.read().await;
        match secrets.get(SERVICE_TOKEN_HASH_KEY) {
            Some(stored_hash) => {
                let computed_hash = Self::hash_token(token);
                computed_hash == *stored_hash
            }
            None => false,
        }
    }

    /// Generate and store a new service token
    /// Returns the token (which should be written to the service token file)
    pub async fn rotate_service_token(&self) -> Result<String, super::SecurityError> {
        let token = Self::generate_service_token();
        let hash = Self::hash_token(&token);
        self.store(SERVICE_TOKEN_HASH_KEY, &hash).await?;
        Ok(token)
    }

    /// Generate a cryptographically secure service token
    fn generate_service_token() -> String {
        let bytes: [u8; 32] = rand::random();
        hex::encode(bytes)
    }

    /// Hash a token using SHA-256
    fn hash_token(token: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Get the path for the service token file
    pub fn service_token_path(&self) -> Option<PathBuf> {
        self.config.secrets_path.as_ref().map(|path| {
            path.parent()
                .map(|p| p.join(SERVICE_TOKEN_FILE))
                .unwrap_or_else(|| PathBuf::from(SERVICE_TOKEN_FILE))
        })
    }
}

impl Default for SecretStore {
    fn default() -> Self {
        Self::new()
    }
}

fn load_persisted_secrets(
    config: &SecretStoreConfig,
) -> Result<HashMap<String, String>, super::SecurityError> {
    let path = match &config.secrets_path {
        Some(path) if path.exists() => path,
        _ => return Ok(HashMap::new()),
    };

    let bytes =
        std::fs::read(path).map_err(|e| super::SecurityError::SecretError(e.to_string()))?;
    if bytes.is_empty() {
        return Ok(HashMap::new());
    }

    let plaintext = if config.encryption_enabled {
        decrypt_bytes(path, &bytes)?
    } else {
        bytes
    };

    serde_json::from_slice(&plaintext).map_err(|e| super::SecurityError::SecretError(e.to_string()))
}

fn persist_if_configured(
    config: &SecretStoreConfig,
    secrets: &HashMap<String, String>,
) -> Result<(), super::SecurityError> {
    let path = match &config.secrets_path {
        Some(path) => path,
        None => return Ok(()),
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| super::SecurityError::SecretError(e.to_string()))?;
    }

    let plaintext = serde_json::to_vec(secrets)
        .map_err(|e| super::SecurityError::SecretError(e.to_string()))?;
    let payload = if config.encryption_enabled {
        encrypt_bytes(path, &plaintext)?
    } else {
        plaintext
    };

    std::fs::write(path, payload).map_err(|e| super::SecurityError::SecretError(e.to_string()))
}

fn encrypt_bytes(path: &Path, plaintext: &[u8]) -> Result<Vec<u8>, super::SecurityError> {
    let key_bytes = load_or_create_key(path)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let nonce_bytes: [u8; 12] = rand::random();
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|e| super::SecurityError::SecretError(e.to_string()))?;

    let mut payload = nonce_bytes.to_vec();
    payload.extend_from_slice(&ciphertext);
    Ok(payload)
}

fn decrypt_bytes(path: &Path, payload: &[u8]) -> Result<Vec<u8>, super::SecurityError> {
    if payload.len() < 12 {
        return Err(super::SecurityError::SecretError(
            "encrypted secrets payload too short".to_string(),
        ));
    }

    let key_bytes = load_or_create_key(path)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let (nonce, ciphertext) = payload.split_at(12);
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|e| super::SecurityError::SecretError(e.to_string()))
}

fn load_or_create_key(path: &Path) -> Result<[u8; 32], super::SecurityError> {
    let key_path = key_path_for(path);
    if key_path.exists() {
        let key = std::fs::read(&key_path)
            .map_err(|e| super::SecurityError::SecretError(e.to_string()))?;
        return key.try_into().map_err(|_| {
            super::SecurityError::SecretError("invalid secret key length".to_string())
        });
    }

    let key: [u8; 32] = rand::random();
    std::fs::write(&key_path, key).map_err(|e| super::SecurityError::SecretError(e.to_string()))?;
    Ok(key)
}

/// Return the path where the encryption key lives for a given secrets file.
pub fn secrets_key_path(path: &Path) -> PathBuf {
    key_path_for(path)
}

fn key_path_for(path: &Path) -> PathBuf {
    let mut key_path = path.to_path_buf();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("enc");
    key_path.set_extension(format!("{extension}.key"));
    key_path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn secret_store_persists_encrypted_secrets() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_secret_store_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("secrets.enc");
        let store = SecretStore::with_config(SecretStoreConfig {
            encryption_enabled: true,
            secrets_path: Some(path.clone()),
        });

        store.store("api_key", "secret-value").await.unwrap();

        let restored = SecretStore::with_config(SecretStoreConfig {
            encryption_enabled: true,
            secrets_path: Some(path),
        });

        assert_eq!(
            restored.get("api_key").await.as_deref(),
            Some("secret-value")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn service_token_verification_works() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_service_token_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("secrets.enc");
        let store = SecretStore::with_config(SecretStoreConfig {
            encryption_enabled: false,
            secrets_path: Some(path),
        });

        // Generate a token
        let token = store.rotate_service_token().await.unwrap();

        // Verify the token works
        assert!(store.verify_service_token(&token).await);

        // Verify wrong token fails
        assert!(!store.verify_service_token("wrong-token").await);

        // Verify after rotation, old token fails
        let old_token = token;
        let new_token = store.rotate_service_token().await.unwrap();
        assert!(!store.verify_service_token(&old_token).await);
        assert!(store.verify_service_token(&new_token).await);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn service_token_generation_produces_valid_hex() {
        let token = SecretStore::generate_service_token();
        // Should be 64 hex characters (32 bytes)
        assert_eq!(token.len(), 64);
        // Should be valid hex
        assert!(hex::decode(&token).is_ok());
    }

    #[test]
    fn service_token_hashing_is_consistent() {
        let token = "test-token-123";
        let hash1 = SecretStore::hash_token(token);
        let hash2 = SecretStore::hash_token(token);
        assert_eq!(hash1, hash2);
        // Hash should be different from original
        assert_ne!(hash1, token);
        // Should be valid hex (SHA-256 = 32 bytes = 64 hex chars)
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn service_token_path_derived_from_secrets_path() {
        let store = SecretStore::with_config(SecretStoreConfig {
            encryption_enabled: false,
            secrets_path: Some(PathBuf::from("/home/user/.borgclaw/secrets.enc")),
        });
        let token_path = store.service_token_path();
        assert_eq!(
            token_path,
            Some(PathBuf::from("/home/user/.borgclaw/.service_token"))
        );
    }
}
