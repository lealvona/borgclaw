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
