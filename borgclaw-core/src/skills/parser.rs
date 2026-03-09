//! SKILL.md parser - parses the SKILL.md standard format

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
        let mut version = "1.0.0".to_string();
        let mut description = String::new();
        let mut author = None;
        let mut commands = Vec::new();
        let mut env = HashMap::new();
        let mut dependencies = Vec::new();
        let mut instructions = String::new();
        let mut examples = Vec::new();

        let mut in_instructions = false;
        let mut in_examples = false;

        for line in content.lines() {
            let line = line.trim();

            // Frontmatter parsing
            if line.starts_with("name:") {
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
        })
    }
}

/// Simple skill manifest for basic skills
pub fn default_skill(name: &str, description: &str) -> SkillManifest {
    SkillManifest {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: description.to_string(),
        author: None,
        commands: Vec::new(),
        env: HashMap::new(),
        dependencies: Vec::new(),
        instructions: String::new(),
        examples: Vec::new(),
    }
}
