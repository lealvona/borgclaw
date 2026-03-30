//! SKILL.md parser - parses the SKILL.md standard format

use crate::constants::DEFAULT_SKILL_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// SKILL.md manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Skill name
    pub name: String,
    /// Version
    pub version: String,
    /// Description
    pub description: String,
    /// Author
    pub author: Option<String>,
    /// Commands this skill provides
    pub commands: Vec<String>,
    /// Required secrets/environment
    pub env: HashMap<String, String>,
    /// Dependencies
    pub dependencies: Vec<String>,
    /// Companion files that should be installed alongside SKILL.md
    pub files: Vec<String>,
    /// Required binaries for the skill to be usable
    pub binaries: Vec<String>,
    /// Required config keys for the skill to be usable
    pub config: HashMap<String, String>,
    /// Instructions for the AI
    pub instructions: String,
    /// Examples
    pub examples: Vec<Example>,
    /// Minimum borgclaw version required
    pub min_version: Option<String>,
}

/// Example usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    pub description: String,
    pub input: String,
    pub output: Option<String>,
}

/// Skill command (parsed from instructions)
#[derive(Debug, Clone)]
pub struct SkillCommand {
    /// Command trigger
    pub trigger: String,
    /// Description
    pub description: String,
    /// Parameters
    pub parameters: Vec<Parameter>,
}

/// Parameter definition
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub default: Option<String>,
}

impl SkillManifest {
    /// Parse SKILL.md content
    pub fn parse(content: &str) -> Result<Self, super::SkillsError> {
        #[derive(Clone, Copy)]
        enum ManifestList {
            Commands,
            Env,
            Dependencies,
            Files,
            Binaries,
            Config,
        }

        let mut name = String::new();
        let mut version = DEFAULT_SKILL_VERSION.to_string();
        let mut description = String::new();
        let mut author = None;
        let mut commands = Vec::new();
        let mut env = HashMap::new();
        let mut dependencies = Vec::new();
        let mut files = Vec::new();
        let mut binaries = Vec::new();
        let mut config = HashMap::new();
        let mut instructions = String::new();
        let mut examples = Vec::new();

        let mut min_version = None;
        let mut in_instructions = false;
        let mut in_examples = false;
        let mut current_list = None;

        for line in content.lines() {
            let line = line.trim();

            // Frontmatter parsing
            if line.starts_with("min_version:") {
                min_version = Some(line.trim_start_matches("min_version:").trim().to_string());
                current_list = None;
            } else if line.starts_with("name:") {
                name = line.trim_start_matches("name:").trim().to_string();
                current_list = None;
            } else if line.starts_with("version:") {
                version = line.trim_start_matches("version:").trim().to_string();
                current_list = None;
            } else if line.starts_with("description:") {
                description = line.trim_start_matches("description:").trim().to_string();
                current_list = None;
            } else if line.starts_with("author:") {
                author = Some(line.trim_start_matches("author:").trim().to_string());
                current_list = None;
            } else if line.starts_with("commands:") {
                current_list = Some(ManifestList::Commands);
            } else if line.starts_with("env:") {
                current_list = Some(ManifestList::Env);
            } else if line.starts_with("dependencies:") {
                current_list = Some(ManifestList::Dependencies);
            } else if line.starts_with("files:") {
                current_list = Some(ManifestList::Files);
            } else if line.starts_with("binaries:") {
                current_list = Some(ManifestList::Binaries);
            } else if line.starts_with("config:") {
                current_list = Some(ManifestList::Config);
            } else if line.starts_with("- ") && !in_instructions && !in_examples {
                let item = line.trim_start_matches("- ").trim();
                match current_list {
                    Some(ManifestList::Commands) | None => {
                        if !item.is_empty() {
                            commands.push(item.to_string());
                        }
                    }
                    Some(ManifestList::Env) => {
                        let parts: Vec<_> = item.splitn(2, '=').collect();
                        if parts.len() == 2 {
                            env.insert(parts[0].to_string(), parts[1].to_string());
                        }
                    }
                    Some(ManifestList::Dependencies) => {
                        dependencies.push(item.to_string());
                    }
                    Some(ManifestList::Files) => {
                        files.push(item.to_string());
                    }
                    Some(ManifestList::Binaries) => {
                        binaries.push(item.to_string());
                    }
                    Some(ManifestList::Config) => {
                        let parts: Vec<_> = item.splitn(2, '=').collect();
                        if parts.len() == 2 {
                            config.insert(parts[0].to_string(), parts[1].to_string());
                        } else if !item.is_empty() {
                            config.insert(item.to_string(), String::new());
                        }
                    }
                }
            } else if line == "## Instructions" || line == "## instructions" {
                in_instructions = true;
                in_examples = false;
                current_list = None;
            } else if line == "## Examples" || line == "## examples" {
                in_examples = true;
                in_instructions = false;
                current_list = None;
            } else if (in_instructions || in_examples) && line.starts_with("## ") {
                in_instructions = false;
                in_examples = false;
            } else if in_instructions {
                instructions.push_str(line);
                instructions.push('\n');
            } else if in_examples && line.starts_with("- ") {
                // Parse example
                let example_text = line.trim_start_matches("- ");
                examples.push(Example {
                    description: example_text.to_string(),
                    input: String::new(),
                    output: None,
                });
            }
        }

        // If no name found, try to get from first heading
        if name.is_empty() {
            for line in content.lines() {
                if line.starts_with("# ") {
                    name = line.trim_start_matches("# ").trim().to_string();
                    break;
                }
            }
        }

        if name.is_empty() {
            return Err(super::SkillsError::ParseError(
                "Missing skill name".to_string(),
            ));
        }

        Ok(Self {
            name,
            version,
            description,
            author,
            commands,
            env,
            dependencies,
            files,
            binaries,
            config,
            instructions: instructions.trim().to_string(),
            examples,
            min_version,
        })
    }

    /// Check whether this skill is compatible with the given borgclaw version.
    /// Returns true if no min_version is set or if `current >= min_version`.
    ///
    /// Uses proper semantic versioning comparison via the semver crate.
    pub fn is_compatible(&self, current_version: &str) -> bool {
        match &self.min_version {
            None => true,
            Some(min) => version_gte(current_version, min),
        }
    }
}

/// Normalize a version string to valid semver (adds missing patch version).
fn normalize_version(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    match parts.len() {
        1 => format!("{}.0.0", parts[0]),
        2 => format!("{}.{}.0", parts[0], parts[1]),
        _ => version.to_string(),
    }
}

/// Compare two versions using proper semantic versioning.
/// Returns true if `current >= minimum`.
///
/// This function normalizes partial versions (e.g., "1.8" -> "1.8.0") before comparison.
pub fn version_gte(current: &str, minimum: &str) -> bool {
    let current_norm = normalize_version(current);
    let minimum_norm = normalize_version(minimum);

    match (
        semver::Version::parse(&current_norm),
        semver::Version::parse(&minimum_norm),
    ) {
        (Ok(current), Ok(minimum)) => current >= minimum,
        _ => false, // Invalid versions are not compatible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extracts_min_version() {
        let content = "---\nname: test-skill\nmin_version: 1.8.0\n---\n# Test Skill\n";
        let manifest = SkillManifest::parse(content).unwrap();
        assert_eq!(manifest.min_version.as_deref(), Some("1.8.0"));
    }

    #[test]
    fn parse_without_min_version() {
        let content = "---\nname: test-skill\n---\n# Test Skill\n";
        let manifest = SkillManifest::parse(content).unwrap();
        assert!(manifest.min_version.is_none());
    }

    #[test]
    fn parse_extracts_manifest_files() {
        let content = "---\nname: test-skill\nfiles:\n- assets/icon.txt\n- prompts/system.txt\n---\n# Test Skill\n";
        let manifest = SkillManifest::parse(content).unwrap();
        assert_eq!(
            manifest.files,
            vec![
                "assets/icon.txt".to_string(),
                "prompts/system.txt".to_string()
            ]
        );
    }

    #[test]
    fn parse_extracts_binary_and_config_requirements() {
        let content = "---\nname: test-skill\nbinaries:\n- curl\n- jq\nconfig:\n- skills.github.token=GitHub token\n- security.docker.enabled\n---\n# Test Skill\n";
        let manifest = SkillManifest::parse(content).unwrap();
        assert_eq!(
            manifest.binaries,
            vec!["curl".to_string(), "jq".to_string()]
        );
        assert_eq!(
            manifest
                .config
                .get("skills.github.token")
                .map(String::as_str),
            Some("GitHub token")
        );
        assert_eq!(
            manifest
                .config
                .get("security.docker.enabled")
                .map(String::as_str),
            Some("")
        );
    }

    #[test]
    fn is_compatible_returns_true_when_no_min_version() {
        let content = "---\nname: test-skill\n---\n# Test Skill\n";
        let manifest = SkillManifest::parse(content).unwrap();
        assert!(manifest.is_compatible("1.0.0"));
    }

    #[test]
    fn is_compatible_checks_semver() {
        let content = "---\nname: test-skill\nmin_version: 1.8.0\n---\n# Test Skill\n";
        let manifest = SkillManifest::parse(content).unwrap();

        assert!(!manifest.is_compatible("1.7.9"));
        assert!(manifest.is_compatible("1.8.0"));
        assert!(manifest.is_compatible("1.8.1"));
        assert!(manifest.is_compatible("1.10.2"));
        assert!(manifest.is_compatible("2.0.0"));
        assert!(!manifest.is_compatible("0.9.0"));
    }

    #[test]
    fn version_gte_handles_different_lengths() {
        assert!(version_gte("1.8", "1.8.0"));
        assert!(version_gte("1.8.0", "1.8"));
        assert!(!version_gte("1.7", "1.8.0"));
    }

    #[test]
    fn version_gte_handles_double_digit_minor_versions() {
        assert!(version_gte("1.10.0", "1.9.9"));
        assert!(!version_gte("1.9.9", "1.10.0"));
    }

    #[test]
    fn version_gte_rejects_prerelease_before_stable_release() {
        assert!(!version_gte("1.10.0-beta.1", "1.10.0"));
        assert!(version_gte("1.10.0", "1.10.0-beta.1"));
    }

    #[test]
    fn version_gte_returns_false_for_invalid_versions() {
        assert!(!version_gte("not-a-version", "1.8.0"));
        assert!(!version_gte("1.8.0", "still-not-a-version"));
    }
}
