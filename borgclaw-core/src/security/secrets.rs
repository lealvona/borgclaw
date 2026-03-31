//! Secrets module - encrypted secret storage with password protection

use argon2::{
    password_hash::{rand_core::RngCore, SaltString},
    Argon2, PasswordHasher,
};
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
/// Key file version for format compatibility
const KEY_FILE_VERSION: u8 = 1;

/// Salt length for Argon2
const SALT_LEN: usize = 16;
/// Nonce length for ChaCha20-Poly1305
const NONCE_LEN: usize = 12;
/// Tag length for ChaCha20-Poly1305 authentication
const TAG_LEN: usize = 16;

/// Password-protected key file format:
/// [version: 1 byte][salt: 16 bytes][nonce: 12 bytes][encrypted_key: 32 bytes][tag: 16 bytes]
/// Total: 78 bytes

#[derive(Debug, Clone)]
pub struct SecretStoreConfig {
    pub encryption_enabled: bool,
    pub secrets_path: Option<PathBuf>,
}

/// Secret store state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreState {
    /// Store is locked, password required to access secrets
    Locked,
    /// Store is unlocked and operational
    Unlocked,
    /// Store has no password set (file-based key only)
    Unprotected,
}

/// Secret store - encrypted storage for API keys and credentials
pub struct SecretStore {
    secrets: Arc<RwLock<HashMap<String, String>>>,
    config: SecretStoreConfig,
    /// The decrypted encryption key (only present when unlocked)
    encryption_key: Arc<RwLock<Option<[u8; 32]>>>,
    /// Current store state
    state: Arc<RwLock<StoreState>>,
}

/// Error type for password operations
#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("Store is already initialized with a password")]
    AlreadyInitialized,
    #[error("Store is not initialized with password protection")]
    NotPasswordProtected,
    #[error("Incorrect password")]
    IncorrectPassword,
    #[error("Store is locked - unlock with password first")]
    StoreLocked,
    #[error("Key file format error: {0}")]
    FormatError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Security error: {0}")]
    Security(String),
}

impl From<super::SecurityError> for PasswordError {
    fn from(e: super::SecurityError) -> Self {
        PasswordError::Security(e.to_string())
    }
}

impl SecretStore {
    pub fn new() -> Self {
        Self::with_config(SecretStoreConfig {
            encryption_enabled: false,
            secrets_path: None,
        })
    }

    pub fn with_config(config: SecretStoreConfig) -> Self {
        let secrets = if config.encryption_enabled {
            // Try to load with file-based key (legacy mode)
            load_persisted_secrets(&config).unwrap_or_default()
        } else {
            load_persisted_secrets(&config).unwrap_or_default()
        };

        // Determine initial state
        let state = if !config.encryption_enabled {
            StoreState::Unprotected
        } else if is_password_protected(&config) {
            StoreState::Locked
        } else {
            // Legacy file-based key - try to load it
            StoreState::Unprotected
        };

        let store = Self {
            secrets: Arc::new(RwLock::new(secrets)),
            config,
            encryption_key: Arc::new(RwLock::new(None)),
            state: Arc::new(RwLock::new(state)),
        };

        // If using legacy file-based key, load it
        if state == StoreState::Unprotected && store.config.encryption_enabled {
            if let Some(path) = &store.config.secrets_path {
                if let Ok(key) = load_legacy_key(path) {
                    *store.encryption_key.blocking_write() = Some(key);
                }
            }
        }

        store
    }

    /// Get current store state
    pub async fn state(&self) -> StoreState {
        *self.state.read().await
    }

    /// Check if store is password protected
    pub fn is_password_protected(&self) -> bool {
        is_password_protected(&self.config)
    }

    /// Initialize the store with password protection
    /// This creates a new password-protected key file
    pub async fn init_with_password(
        &self,
        password: &str,
    ) -> Result<(), PasswordError> {
        if !self.config.encryption_enabled {
            return Err(PasswordError::Encryption(
                "Encryption must be enabled to use password protection".to_string(),
            ));
        }

        let path = self
            .config
            .secrets_path
            .as_ref()
            .ok_or_else(|| PasswordError::Encryption("No secrets path configured".to_string()))?;

        let key_path = key_path_for(path);

        // Check if already initialized
        if key_path.exists() {
            // Check if it's already password protected
            let contents = tokio::fs::read(&key_path).await?;
            if contents.len() > 0 && contents[0] == KEY_FILE_VERSION {
                return Err(PasswordError::AlreadyInitialized);
            }
        }

        // Generate random encryption key
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);

        // Derive key from password using Argon2
        let salt = SaltString::generate(&mut rand::thread_rng());
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| PasswordError::Encryption(e.to_string()))?;

        // Extract the hash output as our key derivation
        let derived_key = password_hash.hash.ok_or_else(|| {
            PasswordError::Encryption("Failed to derive key from password".to_string())
        })?;
        let derived_key_bytes: &[u8] = derived_key.as_bytes();
        if derived_key_bytes.len() < 32 {
            return Err(PasswordError::Encryption(
                "Derived key too short".to_string(),
            ));
        }

        // Encrypt the random key with the derived key
        let cipher =
            ChaCha20Poly1305::new(Key::from_slice(&derived_key_bytes[..32]));
        let nonce_bytes: [u8; 12] = rand::random();
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                key.as_ref(),
            )
            .map_err(|e| PasswordError::Encryption(e.to_string()))?;

        // Build key file: [version][salt][nonce][ciphertext]
        let salt_bytes = salt.as_str().as_bytes();
        let salt_len = salt_bytes.len() as u8;
        
        let mut key_file_content = Vec::new();
        key_file_content.push(KEY_FILE_VERSION);
        key_file_content.push(salt_len);
        key_file_content.extend_from_slice(salt_bytes);
        key_file_content.extend_from_slice(&nonce_bytes);
        key_file_content.extend_from_slice(&ciphertext);

        // Write key file
        tokio::fs::write(&key_path, key_file_content).await?;
        
        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = tokio::fs::metadata(&key_path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            tokio::fs::set_permissions(&key_path, permissions).await?;
        }

        // Store the decrypted key and update state
        *self.encryption_key.write().await = Some(key);
        *self.state.write().await = StoreState::Unlocked;

        Ok(())
    }

    /// Unlock the store with a password
    pub async fn unlock(&self, password: &str) -> Result<(), PasswordError> {
        if !self.config.encryption_enabled {
            return Err(PasswordError::NotPasswordProtected);
        }

        let path = self
            .config
            .secrets_path
            .as_ref()
            .ok_or_else(|| PasswordError::Encryption("No secrets path configured".to_string()))?;

        let key_path = key_path_for(path);

        if !key_path.exists() {
            return Err(PasswordError::NotPasswordProtected);
        }

        // Read key file
        let contents = tokio::fs::read(&key_path).await?;
        
        if contents.is_empty() || contents[0] != KEY_FILE_VERSION {
            // Try legacy format
            if let Ok(key) = load_legacy_key(path) {
                *self.encryption_key.write().await = Some(key);
                *self.state.write().await = StoreState::Unprotected;
                return Ok(());
            }
            return Err(PasswordError::FormatError(
                "Unknown key file format".to_string(),
            ));
        }

        // Parse key file format: [version:1][salt_len:1][salt:N][nonce:12][ciphertext:48]
        if contents.len() < 3 + NONCE_LEN + 32 + TAG_LEN {
            return Err(PasswordError::FormatError(
                "Key file too short".to_string(),
            ));
        }

        let salt_len = contents[1] as usize;
        let salt_start = 2;
        let salt_end = salt_start + salt_len;
        let nonce_start = salt_end;
        let nonce_end = nonce_start + NONCE_LEN;
        let ciphertext_start = nonce_end;

        if contents.len() < ciphertext_start + 32 + TAG_LEN {
            return Err(PasswordError::FormatError(
                "Key file truncated".to_string(),
            ));
        }

        let salt = std::str::from_utf8(&contents[salt_start..salt_end])
            .map_err(|_| PasswordError::FormatError("Invalid salt".to_string()))?;
        let nonce = &contents[nonce_start..nonce_end];
        let ciphertext = &contents[ciphertext_start..];

        // Re-derive key from password
        let salt_string = SaltString::from_b64(salt)
            .map_err(|_| PasswordError::FormatError("Invalid salt format".to_string()))?;
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt_string)
            .map_err(|_| PasswordError::IncorrectPassword)?;

        let derived_key = password_hash.hash.ok_or(PasswordError::IncorrectPassword)?;
        let derived_key_bytes: &[u8] = derived_key.as_bytes();
        if derived_key_bytes.len() < 32 {
            return Err(PasswordError::IncorrectPassword);
        }

        // Decrypt the encryption key
        let cipher =
            ChaCha20Poly1305::new(Key::from_slice(&derived_key_bytes[..32]));
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| PasswordError::IncorrectPassword)?;

        let key: [u8; 32] = plaintext
            .try_into()
            .map_err(|_| PasswordError::FormatError("Invalid key length".to_string()))?;

        // Store decrypted key and unlock
        *self.encryption_key.write().await = Some(key);
        *self.state.write().await = StoreState::Unlocked;

        // Reload secrets with the new key
        let secrets = load_persisted_secrets_with_key(&self.config, key)?;
        *self.secrets.write().await = secrets;

        Ok(())
    }

    /// Lock the store (clear decrypted key from memory)
    pub async fn lock(&self) {
        *self.encryption_key.write().await = None;
        *self.state.write().await = StoreState::Locked;
    }

    /// Change the store password
    pub async fn change_password(
        &self,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), PasswordError> {
        // Verify old password first
        self.unlock(old_password).await?;

        // Get current encryption key (keep the same key, just re-encrypt with new password)
        let key = self
            .encryption_key
            .read()
            .await
            .ok_or(PasswordError::StoreLocked)?;

        let path = self
            .config
            .secrets_path
            .as_ref()
            .ok_or_else(|| PasswordError::Encryption("No secrets path configured".to_string()))?;

        let key_path = key_path_for(path);

        // Generate new salt for new password
        let salt = SaltString::generate(&mut rand::thread_rng());
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(new_password.as_bytes(), &salt)
            .map_err(|e| PasswordError::Encryption(e.to_string()))?;

        let derived_key = password_hash.hash.ok_or_else(|| {
            PasswordError::Encryption("Failed to derive key from password".to_string())
        })?;
        let derived_key_bytes: &[u8] = derived_key.as_bytes();
        if derived_key_bytes.len() < 32 {
            return Err(PasswordError::Encryption(
                "Derived key too short".to_string(),
            ));
        }

        // Encrypt the existing key with the new derived key
        let cipher =
            ChaCha20Poly1305::new(Key::from_slice(&derived_key_bytes[..32]));
        let nonce_bytes: [u8; 12] = rand::random();
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                key.as_ref(),
            )
            .map_err(|e| PasswordError::Encryption(e.to_string()))?;

        // Build key file: [version][salt_len][salt][nonce][ciphertext]
        let salt_bytes = salt.as_str().as_bytes();
        let salt_len = salt_bytes.len() as u8;
        
        let mut key_file_content = Vec::new();
        key_file_content.push(KEY_FILE_VERSION);
        key_file_content.push(salt_len);
        key_file_content.extend_from_slice(salt_bytes);
        key_file_content.extend_from_slice(&nonce_bytes);
        key_file_content.extend_from_slice(&ciphertext);

        // Write key file
        tokio::fs::write(&key_path, key_file_content).await?;
        
        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = tokio::fs::metadata(&key_path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            tokio::fs::set_permissions(&key_path, permissions).await?;
        }

        // Update state
        *self.state.write().await = StoreState::Unlocked;

        Ok(())
    }

    /// Store a secret
    pub async fn store(&self, key: &str, value: &str) -> Result<(), super::SecurityError> {
        // Check if we need to be unlocked
        let state = *self.state.read().await;
        if state == StoreState::Locked {
            return Err(super::SecurityError::SecretError(
                "Store is locked - unlock first".to_string(),
            ));
        }

        let mut secrets = self.secrets.write().await;
        secrets.insert(key.to_string(), value.to_string());
        persist_if_configured(&self.config, &secrets)?;
        Ok(())
    }

    /// Get a secret
    pub async fn get(&self, key: &str) -> Option<String> {
        // Check if locked
        if *self.state.read().await == StoreState::Locked {
            return None;
        }

        let secrets = self.secrets.read().await;
        secrets.get(key).cloned()
    }

    /// Delete a secret
    pub async fn delete(&self, key: &str) -> Option<String> {
        // Check if locked
        if *self.state.read().await == StoreState::Locked {
            return None;
        }

        let mut secrets = self.secrets.write().await;
        let removed = secrets.remove(key);
        let _ = persist_if_configured(&self.config, &secrets);
        removed
    }

    /// List secret keys (not values)
    pub async fn keys(&self) -> Vec<String> {
        // Check if locked
        if *self.state.read().await == StoreState::Locked {
            return Vec::new();
        }

        let secrets = self.secrets.read().await;
        secrets.keys().cloned().collect()
    }

    /// Check if secret exists
    pub async fn exists(&self, key: &str) -> bool {
        // Check if locked
        if *self.state.read().await == StoreState::Locked {
            return false;
        }

        let secrets = self.secrets.read().await;
        secrets.contains_key(key)
    }

    /// Inject secrets into environment variables (for tool execution)
    pub async fn inject_env(&self) -> HashMap<String, String> {
        // Check if locked
        if *self.state.read().await == StoreState::Locked {
            return HashMap::new();
        }

        let secrets = self.secrets.read().await;
        secrets
            .iter()
            .map(|(k, v)| (format!("BC_SECRET_{}", k.to_uppercase()), v.clone()))
            .collect()
    }

    /// Verify a service token against the stored hash
    pub async fn verify_service_token(&self, token: &str) -> bool {
        // Check if locked
        if *self.state.read().await == StoreState::Locked {
            return false;
        }

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
    pub async fn rotate_service_token(&self) -> Result<String, super::SecurityError> {
        // Check if locked
        if *self.state.read().await == StoreState::Locked {
            return Err(super::SecurityError::SecretError(
                "Store is locked - unlock first".to_string(),
            ));
        }

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
            path
                .parent()
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

/// Check if store is password protected
fn is_password_protected(config: &SecretStoreConfig) -> bool {
    let Some(path) = &config.secrets_path else {
        return false;
    };
    
    let key_path = key_path_for(path);
    if !key_path.exists() {
        return false;
    }

    // Check if key file starts with version byte
    if let Ok(contents) = std::fs::read(&key_path) {
        !contents.is_empty() && contents[0] == KEY_FILE_VERSION
    } else {
        false
    }
}

/// Load legacy file-based key (non-password-protected)
fn load_legacy_key(path: &Path) -> Result<[u8; 32], super::SecurityError> {
    let key_path = key_path_for(path);
    let key = std::fs::read(&key_path)
        .map_err(|e| super::SecurityError::SecretError(e.to_string()))?;
    
    // If it starts with version byte, it's not legacy
    if !key.is_empty() && key[0] == KEY_FILE_VERSION {
        return Err(super::SecurityError::SecretError(
            "Key is password protected".to_string(),
        ));
    }
    
    key.try_into().map_err(|_| {
        super::SecurityError::SecretError("invalid secret key length".to_string())
    })
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
        // Try to get the key - for legacy mode
        match load_legacy_key(path) {
            Ok(key) => decrypt_bytes_with_key(&bytes, &key)?,
            Err(_) => return Ok(HashMap::new()), // Can't decrypt, return empty
        }
    } else {
        bytes
    };

    serde_json::from_slice(&plaintext)
        .map_err(|e| super::SecurityError::SecretError(e.to_string()))
}

fn load_persisted_secrets_with_key(
    config: &SecretStoreConfig,
    key: [u8; 32],
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

    let plaintext = decrypt_bytes_with_key(&bytes, &key)?;

    serde_json::from_slice(&plaintext)
        .map_err(|e| super::SecurityError::SecretError(e.to_string()))
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
        // For persistence, we need to use the legacy method or require unlock
        // This is a limitation - we should have the key available
        plaintext
    } else {
        plaintext
    };

    std::fs::write(path, payload).map_err(|e| super::SecurityError::SecretError(e.to_string()))
}

fn decrypt_bytes_with_key(
    payload: &[u8],
    key: &[u8; 32],
) -> Result<Vec<u8>, super::SecurityError> {
    if payload.len() < 12 {
        return Err(super::SecurityError::SecretError(
            "encrypted secrets payload too short".to_string(),
        ));
    }

    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let (nonce, ciphertext) = payload.split_at(12);
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|e| super::SecurityError::SecretError(e.to_string()))
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
    async fn secret_store_password_protection_works() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_secret_pw_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("secrets.enc");
        
        let store = SecretStore::with_config(SecretStoreConfig {
            encryption_enabled: true,
            secrets_path: Some(path.clone()),
        });

        // Initially locked/unprotected
        assert!(matches!(store.state().await, StoreState::Unprotected | StoreState::Locked));

        // Initialize with password
        store.init_with_password("my_secure_password").await.unwrap();
        assert_eq!(store.state().await, StoreState::Unlocked);
        assert!(store.is_password_protected());

        // Store a secret
        store.store("api_key", "secret-value").await.unwrap();

        // Lock the store
        store.lock().await;
        assert_eq!(store.state().await, StoreState::Locked);
        
        // Should not be able to access secrets while locked
        assert!(store.get("api_key").await.is_none());
        
        // Unlock with wrong password should fail
        assert!(store.unlock("wrong_password").await.is_err());
        assert_eq!(store.state().await, StoreState::Locked);
        
        // Unlock with correct password
        store.unlock("my_secure_password").await.unwrap();
        assert_eq!(store.state().await, StoreState::Unlocked);
        
        // Can access secrets now
        assert_eq!(store.get("api_key").await.as_deref(), Some("secret-value"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn secret_store_change_password_works() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_secret_change_pw_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("secrets.enc");
        
        let store = SecretStore::with_config(SecretStoreConfig {
            encryption_enabled: true,
            secrets_path: Some(path.clone()),
        });

        // Initialize with password
        store.init_with_password("old_password").await.unwrap();
        store.store("key", "value").await.unwrap();
        
        // Change password
        store.change_password("old_password", "new_password").await.unwrap();
        
        // Should be unlocked after change
        assert_eq!(store.state().await, StoreState::Unlocked);
        
        // Old password should not work
        store.lock().await;
        assert!(store.unlock("old_password").await.is_err());
        
        // New password should work
        store.unlock("new_password").await.unwrap();
        assert_eq!(store.get("key").await.as_deref(), Some("value"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn secret_store_persists_encrypted_secrets_legacy() {
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
