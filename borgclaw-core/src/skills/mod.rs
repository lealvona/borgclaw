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
    GoogleOAuthConfig, OAuthPendingStore, OAuthState,
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
    catalog: Arc<RwLock<Vec<SkillCatalogEntry>>>,
    managed_path: PathBuf,
    bundled_path: Option<PathBuf>,
    workspace_path: Option<PathBuf>,
}

impl SkillsRegistry {
    pub fn new(managed_path: PathBuf) -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
            catalog: Arc::new(RwLock::new(Vec::new())),
            managed_path,
            bundled_path: None,
            workspace_path: None,
        }
    }

    pub fn with_bundled_path(mut self, bundled_path: PathBuf) -> Self {
        self.bundled_path = Some(bundled_path);
        self
    }

    pub fn with_workspace_path(mut self, workspace_path: PathBuf) -> Self {
        self.workspace_path = Some(workspace_path);
        self
    }

    /// Load all skills from skills directory
    pub async fn load_all(&self) -> Result<(), SkillsError> {
        let mut skills = self.skills.write().await;
        let mut catalog = self.catalog.write().await;
        skills.clear();
        catalog.clear();

        let mut seen_roots = std::collections::HashSet::new();
        for (source, path) in self.source_roots() {
            let normalized = normalize_root_key(&path);
            if !seen_roots.insert(normalized) {
                continue;
            }
            scan_skill_root(source, &path, &mut catalog)?;
        }

        catalog.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.source.priority().cmp(&right.source.priority()))
        });

        let mut effective_sources = HashMap::new();
        for entry in catalog.iter() {
            let candidate = effective_sources
                .entry(entry.id.clone())
                .or_insert(entry.source);
            if entry.source.priority() > candidate.priority() {
                *candidate = entry.source;
            }
        }

        for entry in catalog.iter_mut() {
            let effective_source = effective_sources.get(&entry.id).copied();
            entry.shadowed_by = effective_source.filter(|source| *source != entry.source);
            if effective_source == Some(entry.source) {
                skills.insert(
                    entry.id.clone(),
                    Skill {
                        id: entry.id.clone(),
                        manifest: entry.manifest.clone(),
                        path: entry.path.clone(),
                        source: entry.source,
                    },
                );
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

    /// List effective skills with source metadata
    pub async fn effective_skills(&self) -> Vec<Skill> {
        let mut skills = self
            .skills
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        skills.sort_by(|left, right| left.id.cmp(&right.id));
        skills
    }

    /// List all discovered skills across bundled, managed, and workspace roots
    pub async fn catalog(&self) -> Vec<SkillCatalogEntry> {
        let mut catalog = self.catalog.read().await.clone();
        catalog.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.source.priority().cmp(&right.source.priority()))
        });
        catalog
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
    pub id: String,
    pub manifest: SkillManifest,
    pub path: PathBuf,
    pub source: SkillSourceTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillSourceTier {
    Bundled,
    Managed,
    Workspace,
}

impl SkillSourceTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bundled => "bundled",
            Self::Managed => "managed",
            Self::Workspace => "workspace",
        }
    }

    fn priority(&self) -> u8 {
        match self {
            Self::Bundled => 0,
            Self::Managed => 1,
            Self::Workspace => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SkillCatalogEntry {
    pub id: String,
    pub manifest: SkillManifest,
    pub path: PathBuf,
    pub source: SkillSourceTier,
    pub shadowed_by: Option<SkillSourceTier>,
}

fn scan_skill_root(
    source: SkillSourceTier,
    root: &std::path::Path,
    catalog: &mut Vec<SkillCatalogEntry>,
) -> Result<(), SkillsError> {
    if !root.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(root).map_err(|e| SkillsError::IoError(e.to_string()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }

        let content =
            std::fs::read_to_string(&skill_md).map_err(|e| SkillsError::IoError(e.to_string()))?;
        let manifest = SkillManifest::parse(&content)?;
        let id = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        catalog.push(SkillCatalogEntry {
            id,
            manifest,
            path,
            source,
            shadowed_by: None,
        });
    }

    Ok(())
}

fn normalize_root_key(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

impl SkillsRegistry {
    fn source_roots(&self) -> Vec<(SkillSourceTier, PathBuf)> {
        let mut roots = Vec::new();
        if let Some(path) = &self.bundled_path {
            roots.push((SkillSourceTier::Bundled, path.clone()));
        }
        roots.push((SkillSourceTier::Managed, self.managed_path.clone()));
        if let Some(path) = &self.workspace_path {
            roots.push((SkillSourceTier::Workspace, path.clone()));
        }
        roots
    }
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

    #[tokio::test]
    async fn skills_registry_prefers_workspace_over_managed_over_bundled() {
        let root = temp_dir();
        let bundled = root.join("bundled");
        let managed = root.join("managed");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(bundled.join("release-skill")).unwrap();
        std::fs::create_dir_all(managed.join("release-skill")).unwrap();
        std::fs::create_dir_all(workspace.join("release-skill")).unwrap();
        std::fs::write(
            bundled.join("release-skill").join("SKILL.md"),
            "---\nname: bundled-release\ncommands:\n- audit\n---\n## Instructions\nUse bundled\n",
        )
        .unwrap();
        std::fs::write(
            managed.join("release-skill").join("SKILL.md"),
            "---\nname: managed-release\ncommands:\n- audit\n---\n## Instructions\nUse managed\n",
        )
        .unwrap();
        std::fs::write(
            workspace.join("release-skill").join("SKILL.md"),
            "---\nname: workspace-release\ncommands:\n- audit\n---\n## Instructions\nUse workspace\n",
        )
        .unwrap();

        let registry = SkillsRegistry::new(managed.clone())
            .with_bundled_path(bundled.clone())
            .with_workspace_path(workspace.clone());
        registry.load_all().await.unwrap();

        let effective = registry.get("release-skill").await.unwrap();
        assert_eq!(effective.manifest.name, "workspace-release");
        assert_eq!(effective.source, SkillSourceTier::Workspace);

        let catalog = registry.catalog().await;
        assert_eq!(catalog.len(), 3);
        assert_eq!(catalog[0].source, SkillSourceTier::Bundled);
        assert_eq!(catalog[0].shadowed_by, Some(SkillSourceTier::Workspace));
        assert_eq!(catalog[1].source, SkillSourceTier::Managed);
        assert_eq!(catalog[1].shadowed_by, Some(SkillSourceTier::Workspace));
        assert_eq!(catalog[2].source, SkillSourceTier::Workspace);
        assert_eq!(catalog[2].shadowed_by, None);

        let _ = std::fs::remove_dir_all(&root);
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
