//! Skills module - SKILL.md parser and skill management

pub mod browser;
pub mod github;
pub mod google;
pub mod image;
mod parser;
pub mod plugin;
pub mod qr;
pub mod stt;
pub mod tts;
pub mod url_shortener;

pub use parser::{SkillCommand, SkillManifest};

pub use browser::{
    BrowserConfig, BrowserSkill, BrowserType, CdpClient, Cookie, PlaywrightClient, PlaywrightConfig,
};
pub use github::{GitHubClient, GitHubConfig, GitHubSafety, OperationType, RepoAccess};
pub use google::{
    CalendarClient, CalendarEvent, DriveClient, GmailClient, GoogleAuth, GoogleClient,
    GoogleOAuthConfig,
};
pub use image::{ImageBackend, ImageClient, ImageFormat, ImageParams, ImageResult};
pub use plugin::{PluginManifest, PluginRegistry, WasmPermission};
pub use qr::{QrCodeSkill, QrFormat, QrSkill};
pub use stt::{AudioFormat, SttBackend, SttClient};
pub use tts::{ElevenLabsClient, ElevenLabsConfig, TtsClient, Voice};
pub use url_shortener::{UrlShortener, UrlShortenerProvider};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Skills registry
pub struct SkillsRegistry {
    skills: Arc<RwLock<HashMap<String, Skill>>>,
    skills_path: PathBuf,
}

impl SkillsRegistry {
    pub fn new(skills_path: PathBuf) -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
            skills_path,
        }
    }

    /// Load all skills from skills directory
    pub async fn load_all(&self) -> Result<(), SkillsError> {
        let mut skills = self.skills.write().await;

        if !self.skills_path.exists() {
            return Ok(());
        }

        let entries = std::fs::read_dir(&self.skills_path)
            .map_err(|e| SkillsError::IoError(e.to_string()))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    if let Ok(content) = std::fs::read_to_string(&skill_md) {
                        if let Ok(skill) = SkillManifest::parse(&content) {
                            let id = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown")
                                .to_string();

                            skills.insert(
                                id,
                                Skill {
                                    manifest: skill,
                                    path,
                                },
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get a skill by ID
    pub async fn get(&self, id: &str) -> Option<Skill> {
        let skills = self.skills.read().await;
        skills.get(id).cloned()
    }

    /// List all skills
    pub async fn list(&self) -> Vec<(String, String)> {
        let skills = self.skills.read().await;
        skills
            .iter()
            .map(|(id, s)| (id.clone(), s.manifest.name.clone()))
            .collect()
    }

    /// Get skill by command name
    pub async fn get_by_command(&self, command: &str) -> Option<Skill> {
        let skills = self.skills.read().await;
        skills
            .values()
            .find(|s| s.manifest.commands.iter().any(|c| c == command))
            .cloned()
    }
}

/// Loaded skill
#[derive(Debug, Clone)]
pub struct Skill {
    pub manifest: SkillManifest,
    pub path: PathBuf,
}

/// Skills errors
#[derive(Debug, thiserror::Error)]
pub enum SkillsError {
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Skill not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("borgclaw_test_{}", uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn skills_registry_new_creates_empty() {
        let temp_path = temp_dir();
        let registry = SkillsRegistry::new(temp_path.clone());

        let list = registry.list().await;
        assert!(list.is_empty());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_path);
    }

    #[tokio::test]
    async fn skills_registry_load_all_empty_dir() {
        let temp_path = temp_dir();
        std::fs::create_dir_all(&temp_path).unwrap();

        let registry = SkillsRegistry::new(temp_path.clone());

        let result = registry.load_all().await;
        assert!(result.is_ok());

        let list = registry.list().await;
        assert!(list.is_empty());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_path);
    }

    #[tokio::test]
    async fn skills_registry_load_all_nonexistent_dir() {
        let temp_path = temp_dir();
        let registry = SkillsRegistry::new(temp_path);

        let result = registry.load_all().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn skills_registry_get_returns_none_for_empty() {
        let temp_path = temp_dir();
        let registry = SkillsRegistry::new(temp_path.clone());

        let skill = registry.get("nonexistent").await;
        assert!(skill.is_none());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_path);
    }

    #[tokio::test]
    async fn skills_registry_get_by_command_returns_none_for_empty() {
        let temp_path = temp_dir();
        let registry = SkillsRegistry::new(temp_path.clone());

        let skill = registry.get_by_command("unknown").await;
        assert!(skill.is_none());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_path);
    }

    #[test]
    fn skills_error_display_messages() {
        let io_err = SkillsError::IoError("disk full".to_string());
        assert!(io_err.to_string().contains("IO error"));
        assert!(io_err.to_string().contains("disk full"));

        let parse_err = SkillsError::ParseError("invalid syntax".to_string());
        assert!(parse_err.to_string().contains("Parse error"));
        assert!(parse_err.to_string().contains("invalid syntax"));

        let not_found_err = SkillsError::NotFound("skill-x".to_string());
        assert!(not_found_err.to_string().contains("Skill not found"));
        assert!(not_found_err.to_string().contains("skill-x"));
    }
}
