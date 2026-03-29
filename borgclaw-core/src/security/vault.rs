//! Vault integration - Bitwarden (primary) and 1Password (secondary)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultItem {
    pub id: String,
    pub name: String,
    pub folder: Option<String>,
    pub item_type: VaultItemType,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VaultItemType {
    Login,
    SecureNote,
    Card,
    Identity,
}

#[async_trait]
pub trait VaultClient: Send + Sync {
    async fn get_secret(&self, name: &str) -> Result<String, VaultError>;
    async fn list_items(&self, folder: Option<&str>) -> Result<Vec<VaultItem>, VaultError>;
    async fn create_item(
        &self,
        name: &str,
        value: &str,
        folder: Option<&str>,
    ) -> Result<String, VaultError>;
    async fn update_item(&self, id: &str, value: &str) -> Result<(), VaultError>;
    async fn delete_item(&self, id: &str) -> Result<(), VaultError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitwardenConfig {
    pub cli_path: PathBuf,
    pub session_env: String,
    pub server_url: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub use_cli: bool,
}

impl Default for BitwardenConfig {
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

pub struct BitwardenClient {
    config: BitwardenConfig,
    session_key: Arc<RwLock<Option<String>>>,
}

impl BitwardenClient {
    pub fn new(config: BitwardenConfig) -> Self {
        Self {
            config,
            session_key: Arc::new(RwLock::new(None)),
        }
    }

    async fn ensure_unlocked(&self) -> Result<(), VaultError> {
        if self.session_key.read().await.is_some() {
            return Ok(());
        }

        if !self.config.use_cli {
            return Err(VaultError::NotAuthenticated);
        }

        if let Ok(session) = std::env::var(&self.config.session_env) {
            *self.session_key.write().await = Some(session);
            return Ok(());
        }

        let output = tokio::process::Command::new(&self.config.cli_path)
            .arg("unlocked")
            .output()
            .await
            .map_err(|e| VaultError::CliError(e.to_string()))?;

        if !output.status.success() {
            return Err(VaultError::NotAuthenticated);
        }

        *self.session_key.write().await = Some(String::new());
        Ok(())
    }

    async fn run_bw(&self, args: &[&str]) -> Result<String, VaultError> {
        self.ensure_unlocked().await?;

        let mut command = tokio::process::Command::new(&self.config.cli_path);
        if let Some(session) = self.session_key.read().await.clone() {
            command.env(&self.config.session_env, session);
        }
        let output = command
            .args(args)
            .output()
            .await
            .map_err(|e| VaultError::CliError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VaultError::CliError(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[async_trait]
impl VaultClient for BitwardenClient {
    async fn get_secret(&self, name: &str) -> Result<String, VaultError> {
        let output = self.run_bw(&["get", "item", name]).await?;

        #[derive(Deserialize)]
        struct BwItem {
            login: Option<Login>,
            notes: Option<String>,
        }

        #[derive(Deserialize)]
        struct Login {
            password: Option<String>,
        }

        let item: BwItem =
            serde_json::from_str(&output).map_err(|e| VaultError::ParseFailed(e.to_string()))?;

        if let Some(login) = item.login {
            if let Some(password) = login.password {
                return Ok(password);
            }
        }

        if let Some(notes) = item.notes {
            return Ok(notes);
        }

        Err(VaultError::NotFound(name.to_string()))
    }

    async fn list_items(&self, folder: Option<&str>) -> Result<Vec<VaultItem>, VaultError> {
        let folder_arg = match folder {
            Some(f) => vec!["--folderid", f],
            None => vec![],
        };

        let mut args = vec!["list", "items"];
        args.extend(folder_arg.iter().map(|s| *s));

        let output = self
            .run_bw(&args.iter().map(|s| *s).collect::<Vec<_>>())
            .await?;

        #[derive(Deserialize)]
        struct BwListItem {
            id: String,
            name: String,
            #[serde(rename = "folderId")]
            folder_id: Option<String>,
            #[serde(rename = "type")]
            item_type: u32,
            #[serde(rename = "creationTime")]
            creation_time: Option<String>,
            #[serde(rename = "revisionTime")]
            revision_time: Option<String>,
        }

        let items: Vec<BwListItem> =
            serde_json::from_str(&output).map_err(|e| VaultError::ParseFailed(e.to_string()))?;

        Ok(items
            .into_iter()
            .map(|i| VaultItem {
                id: i.id,
                name: i.name,
                folder: i.folder_id,
                item_type: match i.item_type {
                    1 => VaultItemType::Login,
                    2 => VaultItemType::SecureNote,
                    3 => VaultItemType::Card,
                    4 => VaultItemType::Identity,
                    _ => VaultItemType::SecureNote,
                },
                created_at: i.creation_time.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                }),
                modified_at: i.revision_time.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                }),
            })
            .collect())
    }

    async fn create_item(
        &self,
        name: &str,
        value: &str,
        folder: Option<&str>,
    ) -> Result<String, VaultError> {
        let item_json = serde_json::json!({
            "name": name,
            "notes": value,
            "type": 2,
            "folderId": folder
        });

        let temp_path = std::env::temp_dir().join(format!("bw_item_{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&temp_path, item_json.to_string())
            .map_err(|e| VaultError::IoError(e.to_string()))?;

        let output = self
            .run_bw(&["create", "item", &temp_path.to_string_lossy()])
            .await;

        let _ = std::fs::remove_file(&temp_path);

        #[derive(Deserialize)]
        struct BwCreateResponse {
            id: String,
        }

        let response: BwCreateResponse =
            serde_json::from_str(&output?).map_err(|e| VaultError::ParseFailed(e.to_string()))?;

        Ok(response.id)
    }

    async fn update_item(&self, id: &str, value: &str) -> Result<(), VaultError> {
        let item_json = serde_json::json!({
            "notes": value,
        });

        let temp_path = std::env::temp_dir().join(format!("bw_item_{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&temp_path, item_json.to_string())
            .map_err(|e| VaultError::IoError(e.to_string()))?;

        let output = self
            .run_bw(&["edit", "item", id, &temp_path.to_string_lossy()])
            .await;

        let _ = std::fs::remove_file(&temp_path);

        output.map(|_| ())
    }

    async fn delete_item(&self, id: &str) -> Result<(), VaultError> {
        self.run_bw(&["delete", "item", id]).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnePasswordConfig {
    pub cli_path: PathBuf,
    pub vault: Option<String>,
    pub account: Option<String>,
}

impl Default for OnePasswordConfig {
    fn default() -> Self {
        Self {
            cli_path: PathBuf::from("op"),
            vault: None,
            account: None,
        }
    }
}

pub struct OnePasswordClient {
    config: OnePasswordConfig,
}

impl OnePasswordClient {
    pub fn new(config: OnePasswordConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl VaultClient for OnePasswordClient {
    async fn get_secret(&self, name: &str) -> Result<String, VaultError> {
        let mut args = vec!["item", "get", name, "--format", "json"];

        if let Some(ref vault) = self.config.vault {
            args.extend(&["--vault", vault]);
        }
        if let Some(ref account) = self.config.account {
            args.extend(&["--account", account]);
        }

        let output = tokio::process::Command::new(&self.config.cli_path)
            .args(&args)
            .output()
            .await
            .map_err(|e| VaultError::CliError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VaultError::CliError(stderr.to_string()));
        }

        #[derive(Deserialize)]
        struct OpItem {
            #[serde(rename = "secureNote")]
            secure_note: Option<SecureNote>,
            login: Option<Login>,
        }

        #[derive(Deserialize)]
        struct SecureNote {
            notes: String,
        }

        #[derive(Deserialize)]
        struct Login {
            password: Option<String>,
        }

        let item: OpItem = serde_json::from_str(&String::from_utf8_lossy(&output.stdout))
            .map_err(|e| VaultError::ParseFailed(e.to_string()))?;

        if let Some(notes) = item.secure_note.map(|n| n.notes) {
            return Ok(notes);
        }

        if let Some(login) = item.login {
            if let Some(password) = login.password {
                return Ok(password);
            }
        }

        Err(VaultError::NotFound(name.to_string()))
    }

    async fn list_items(&self, _folder: Option<&str>) -> Result<Vec<VaultItem>, VaultError> {
        let mut args = vec!["item", "list", "--format", "json"];

        if let Some(ref vault) = self.config.vault {
            args.extend(&["--vault", vault]);
        }
        if let Some(ref account) = self.config.account {
            args.extend(&["--account", account]);
        }

        let output = tokio::process::Command::new(&self.config.cli_path)
            .args(&args)
            .output()
            .await
            .map_err(|e| VaultError::CliError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VaultError::CliError(stderr.to_string()));
        }

        #[derive(Deserialize)]
        struct OpListItem {
            id: String,
            title: String,
            vault: Option<String>,
            #[serde(rename = "itemType")]
            item_type: String,
        }

        let items: Vec<OpListItem> = serde_json::from_str(&String::from_utf8_lossy(&output.stdout))
            .map_err(|e| VaultError::ParseFailed(e.to_string()))?;

        Ok(items
            .into_iter()
            .map(|i| VaultItem {
                id: i.id,
                name: i.title,
                folder: i.vault,
                item_type: match i.item_type.as_str() {
                    "LOGIN" => VaultItemType::Login,
                    "SECURENOTE" | "SECURE_NOTE" => VaultItemType::SecureNote,
                    "CARD" => VaultItemType::Card,
                    "IDENTITY" => VaultItemType::Identity,
                    _ => VaultItemType::Login,
                },
                created_at: None,
                modified_at: None,
            })
            .collect())
    }

    async fn create_item(
        &self,
        name: &str,
        value: &str,
        folder: Option<&str>,
    ) -> Result<String, VaultError> {
        let mut args = vec![
            "item".to_string(),
            "create".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];

        args.push(format!("--title={}", name));
        args.push(format!("notesPlain={}", value));

        if let Some(folder) = folder {
            args.extend(["--vault".to_string(), folder.to_string()]);
        } else if let Some(ref vault) = self.config.vault {
            args.extend(["--vault".to_string(), vault.clone()]);
        }
        if let Some(ref account) = self.config.account {
            args.extend(["--account".to_string(), account.clone()]);
        }

        let output = tokio::process::Command::new(&self.config.cli_path)
            .args(&args)
            .output()
            .await
            .map_err(|e| VaultError::CliError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VaultError::CliError(stderr.to_string()));
        }

        #[derive(Deserialize)]
        struct CreatedItem {
            id: String,
        }

        let item: CreatedItem = serde_json::from_str(&String::from_utf8_lossy(&output.stdout))
            .map_err(|e| VaultError::ParseFailed(e.to_string()))?;

        Ok(item.id)
    }

    async fn update_item(&self, id: &str, value: &str) -> Result<(), VaultError> {
        let mut args = vec![
            "item".to_string(),
            "edit".to_string(),
            id.to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];

        args.push(format!("notesPlain={}", value));

        if let Some(ref vault) = self.config.vault {
            args.extend(["--vault".to_string(), vault.clone()]);
        }
        if let Some(ref account) = self.config.account {
            args.extend(["--account".to_string(), account.clone()]);
        }

        let output = tokio::process::Command::new(&self.config.cli_path)
            .args(&args)
            .output()
            .await
            .map_err(|e| VaultError::CliError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VaultError::CliError(stderr.to_string()));
        }

        Ok(())
    }

    async fn delete_item(&self, id: &str) -> Result<(), VaultError> {
        let mut args = vec!["item", "delete", id];

        if let Some(ref vault) = self.config.vault {
            args.extend(&["--vault", vault]);
        }
        if let Some(ref account) = self.config.account {
            args.extend(&["--account", account]);
        }

        let output = tokio::process::Command::new(&self.config.cli_path)
            .args(&args)
            .output()
            .await
            .map_err(|e| VaultError::CliError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VaultError::CliError(stderr.to_string()));
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("CLI error: {0}")]
    CliError(String),

    #[error("Parse failed: {0}")]
    ParseFailed(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Not authenticated")]
    NotAuthenticated,

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Not supported: {0}")]
    NotSupported(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitwarden_config_default_values() {
        let config = BitwardenConfig::default();
        assert_eq!(config.cli_path, PathBuf::from("bw"));
        assert_eq!(config.session_env, "BW_SESSION");
        assert!(config.server_url.is_none());
        assert!(config.client_id.is_none());
        assert!(config.client_secret.is_none());
        assert!(config.use_cli);
    }

    #[test]
    fn onepassword_config_default_values() {
        let config = OnePasswordConfig::default();
        assert_eq!(config.cli_path, PathBuf::from("op"));
        assert!(config.vault.is_none());
        assert!(config.account.is_none());
    }

    #[test]
    fn vault_item_type_variants() {
        assert_eq!(VaultItemType::Login, VaultItemType::Login);
        assert_eq!(VaultItemType::SecureNote, VaultItemType::SecureNote);
        assert_eq!(VaultItemType::Card, VaultItemType::Card);
        assert_eq!(VaultItemType::Identity, VaultItemType::Identity);
        assert_ne!(VaultItemType::Login, VaultItemType::SecureNote);
    }

    #[test]
    fn vault_item_type_equality() {
        let login1 = VaultItemType::Login;
        let login2 = VaultItemType::Login;
        let card = VaultItemType::Card;

        assert_eq!(login1, login2);
        assert_ne!(login1, card);
    }

    #[test]
    fn vault_item_creation() {
        let item = VaultItem {
            id: "test-id-123".to_string(),
            name: "Test Item".to_string(),
            folder: Some("Test Folder".to_string()),
            item_type: VaultItemType::Login,
            created_at: None,
            modified_at: None,
        };

        assert_eq!(item.id, "test-id-123");
        assert_eq!(item.name, "Test Item");
        assert_eq!(item.folder, Some("Test Folder".to_string()));
        assert_eq!(item.item_type, VaultItemType::Login);
    }

    #[test]
    fn vault_error_display_cli_error() {
        let err = VaultError::CliError("command failed".to_string());
        let display = err.to_string();
        assert!(display.contains("CLI error"));
        assert!(display.contains("command failed"));
    }

    #[test]
    fn vault_error_display_parse_failed() {
        let err = VaultError::ParseFailed("invalid json".to_string());
        let display = err.to_string();
        assert!(display.contains("Parse failed"));
        assert!(display.contains("invalid json"));
    }

    #[test]
    fn vault_error_display_io_error() {
        let err = VaultError::IoError("disk full".to_string());
        let display = err.to_string();
        assert!(display.contains("IO error"));
        assert!(display.contains("disk full"));
    }

    #[test]
    fn vault_error_display_not_authenticated() {
        let err = VaultError::NotAuthenticated;
        assert_eq!(err.to_string(), "Not authenticated");
    }

    #[test]
    fn vault_error_display_not_found() {
        let err = VaultError::NotFound("my-secret".to_string());
        let display = err.to_string();
        assert!(display.contains("Not found"));
        assert!(display.contains("my-secret"));
    }

    #[test]
    fn vault_error_display_not_supported() {
        let err = VaultError::NotSupported("feature x".to_string());
        let display = err.to_string();
        assert!(display.contains("Not supported"));
        assert!(display.contains("feature x"));
    }

    #[test]
    fn bitwarden_client_new_stores_config() {
        let config = BitwardenConfig::default();
        let client = BitwardenClient::new(config);
        assert!(client.config.server_url.is_none());
    }

    #[test]
    fn onepassword_client_new_stores_config() {
        let config = OnePasswordConfig::default();
        let client = OnePasswordClient::new(config);
        assert_eq!(client.config.account, None);
    }

    #[test]
    fn bitwarden_config_custom_values() {
        let config = BitwardenConfig {
            cli_path: PathBuf::from("/custom/bw"),
            session_env: "MY_BW_SESSION".to_string(),
            server_url: Some("https://vault.example.com".to_string()),
            client_id: Some("client-123".to_string()),
            client_secret: Some("secret-456".to_string()),
            use_cli: false,
        };

        assert_eq!(config.cli_path, PathBuf::from("/custom/bw"));
        assert_eq!(config.session_env, "MY_BW_SESSION");
        assert_eq!(
            config.server_url,
            Some("https://vault.example.com".to_string())
        );
        assert_eq!(config.client_id, Some("client-123".to_string()));
        assert_eq!(config.client_secret, Some("secret-456".to_string()));
        assert!(!config.use_cli);
    }

    #[test]
    fn onepassword_config_custom_values() {
        let config = OnePasswordConfig {
            cli_path: PathBuf::from("/custom/op"),
            vault: Some("My Vault".to_string()),
            account: Some("my@email.com".to_string()),
        };

        assert_eq!(config.cli_path, PathBuf::from("/custom/op"));
        assert_eq!(config.vault, Some("My Vault".to_string()));
        assert_eq!(config.account, Some("my@email.com".to_string()));
    }
}
