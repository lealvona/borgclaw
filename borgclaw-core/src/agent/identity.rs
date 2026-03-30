use crate::config::{AgentConfig, IdentityFormat};
use serde::Deserialize;
use std::path::Path;

pub struct IdentityLoader;

impl IdentityLoader {
    pub fn load(config: &AgentConfig, default_identity: &str) -> String {
        let Some(path) = config.soul_path.as_ref() else {
            return default_identity.to_string();
        };

        let Ok(content) = std::fs::read_to_string(path) else {
            return default_identity.to_string();
        };

        match Self::resolve_format(config.identity_format, path) {
            IdentityFormat::Auto | IdentityFormat::Markdown => content,
            IdentityFormat::Aieos => {
                Self::parse_aieos(&content).unwrap_or_else(|| default_identity.to_string())
            }
        }
    }

    fn resolve_format(configured: IdentityFormat, path: &Path) -> IdentityFormat {
        if configured != IdentityFormat::Auto {
            return configured;
        }

        match path.extension().and_then(|value| value.to_str()) {
            Some("json") => IdentityFormat::Aieos,
            _ => IdentityFormat::Markdown,
        }
    }

    fn parse_aieos(content: &str) -> Option<String> {
        let doc: AieosIdentity = serde_json::from_str(content).ok()?;
        Some(doc.to_system_prompt())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AieosIdentity {
    name: Option<String>,
    role: Option<String>,
    summary: Option<String>,
    purpose: Option<String>,
    system_prompt: Option<String>,
    style: Option<Vec<String>>,
    instructions: Option<Vec<String>>,
    guardrails: Option<Vec<String>>,
}

impl AieosIdentity {
    fn to_system_prompt(&self) -> String {
        if let Some(system_prompt) = self.system_prompt.as_deref() {
            if !system_prompt.trim().is_empty() {
                return system_prompt.trim().to_string();
            }
        }

        let mut sections = Vec::new();

        if let Some(name) = self.name.as_deref() {
            sections.push(format!("Identity: {}", name.trim()));
        }
        if let Some(role) = self.role.as_deref() {
            sections.push(format!("Role: {}", role.trim()));
        }
        if let Some(summary) = self.summary.as_deref() {
            sections.push(summary.trim().to_string());
        }
        if let Some(purpose) = self.purpose.as_deref() {
            sections.push(format!("Purpose: {}", purpose.trim()));
        }
        if let Some(style) = self.style.as_ref() {
            let style = style
                .iter()
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            if !style.is_empty() {
                sections.push(format!("Style: {}", style.join(", ")));
            }
        }
        if let Some(instructions) = self.instructions.as_ref() {
            let instructions = instructions
                .iter()
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            if !instructions.is_empty() {
                sections.push(format!(
                    "Instructions:\n{}",
                    instructions
                        .iter()
                        .map(|item| format!("- {}", item))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }
        if let Some(guardrails) = self.guardrails.as_ref() {
            let guardrails = guardrails
                .iter()
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            if !guardrails.is_empty() {
                sections.push(format!(
                    "Guardrails:\n{}",
                    guardrails
                        .iter()
                        .map(|item| format!("- {}", item))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }

        if sections.is_empty() {
            String::new()
        } else {
            sections.join("\n\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentConfig;

    #[test]
    fn markdown_identity_returns_raw_file_contents() {
        let dir =
            std::env::temp_dir().join(format!("borgclaw_identity_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.md");
        std::fs::write(&path, "You are a markdown identity.").unwrap();

        let config = AgentConfig {
            soul_path: Some(path),
            identity_format: IdentityFormat::Markdown,
            ..Default::default()
        };

        let loaded = IdentityLoader::load(&config, "fallback");
        assert_eq!(loaded, "You are a markdown identity.");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn aieos_identity_builds_system_prompt() {
        let dir =
            std::env::temp_dir().join(format!("borgclaw_identity_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.json");
        std::fs::write(
            &path,
            r#"{
                "name": "Borg Unit",
                "role": "Systems operator",
                "summary": "You coordinate complex work.",
                "instructions": ["Be precise", "Prefer direct answers"],
                "guardrails": ["Do not leak secrets"]
            }"#,
        )
        .unwrap();

        let config = AgentConfig {
            soul_path: Some(path),
            identity_format: IdentityFormat::Aieos,
            ..Default::default()
        };

        let loaded = IdentityLoader::load(&config, "fallback");
        assert!(loaded.contains("Identity: Borg Unit"));
        assert!(loaded.contains("Role: Systems operator"));
        assert!(loaded.contains("- Be precise"));
        assert!(loaded.contains("- Do not leak secrets"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn auto_identity_format_uses_json_extension_for_aieos() {
        let dir =
            std::env::temp_dir().join(format!("borgclaw_identity_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.json");
        std::fs::write(&path, r#"{"system_prompt":"You are JSON."}"#).unwrap();

        let config = AgentConfig {
            soul_path: Some(path),
            ..Default::default()
        };

        let loaded = IdentityLoader::load(&config, "fallback");
        assert_eq!(loaded, "You are JSON.");
        let _ = std::fs::remove_dir_all(dir);
    }
}
