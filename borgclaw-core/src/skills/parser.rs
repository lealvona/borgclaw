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
        let mut name = String::new();
        let mut version = DEFAULT_SKILL_VERSION.to_string();
        let mut description = String::new();
        let mut author = None;
        let mut commands = Vec::new();
        let mut env = HashMap::new();
        let mut dependencies = Vec::new();
        let mut instructions = String::new();
        let mut examples = Vec::new();

        let mut min_version = None;
        let mut in_instructions = false;
        let mut in_examples = false;

        for line in content.lines() {
            let line = line.trim();

            // Frontmatter parsing
            if line.starts_with("min_version:") {
                min_version = Some(line.trim_start_matches("min_version:").trim().to_string());
            } else if line.starts_with("name:") {
                name = line.trim_start_matches("name:").trim().to_string();
            } else if line.starts_with("version:") {
                version = line.trim_start_matches("version:").trim().to_string();
            } else if line.starts_with("description:") {
                description = line.trim_start_matches("description:").trim().to_string();
            } else if line.starts_with("author:") {
                author = Some(line.trim_start_matches("author:").trim().to_string());
            } else if line.starts_with("commands:") {
                // Commands follow on subsequent lines
            } else if line.starts_with("-") && !in_instructions && !in_examples {
                let cmd = line.trim_start_matches('-').trim();
                if !cmd.is_empty() {
                    commands.push(cmd.to_string());
                }
            } else if line.starts_with("env:") {
                // Environment variables follow
            } else if line.starts_with("- ") && !in_instructions && !in_examples {
                // Could be env or dependency
                let item = line.trim_start_matches("- ").trim();
                if item.contains('=') {
                    let parts: Vec<_> = item.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        env.insert(parts[0].to_string(), parts[1].to_string());
                    }
                } else {
                    dependencies.push(item.to_string());
                }
            } else if line == "## Instructions" || line == "## instructions" {
                in_instructions = true;
                in_examples = false;
            } else if line == "## Examples" || line == "## examples" {
                in_examples = true;
                in_instructions = false;
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
            instructions: instructions.trim().to_string(),
            examples,
            min_version,
        })
    }

    /// Check whether this skill is compatible with the given borgclaw version.
    /// Returns true if no min_version is set or if `current >= min_version`.
    pub fn is_compatible(&self, current_version: &str) -> bool {
        match &self.min_version {
            None => true,
            Some(min) => version_gte(current_version, min),
        }
    }
}

/// Simple semver comparison: returns true when `current >= minimum`.
fn version_gte(current: &str, minimum: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let cur = parse(current);
    let min = parse(minimum);
    for i in 0..cur.len().max(min.len()) {
        let c = cur.get(i).copied().unwrap_or(0);
        let m = min.get(i).copied().unwrap_or(0);
        if c != m {
            return c > m;
        }
    }
    true // equal
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
}
