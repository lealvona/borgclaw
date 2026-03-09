use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDef {
    pub id: String,
    pub display: String,
    pub api_base: String,
    pub models_endpoint: String,
    pub api_key_env: Option<String>,
    pub default_model: String,
    pub static_models: Vec<String>,
    pub requires_auth: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderRegistry {
    pub providers: HashMap<String, ProviderDef>,
}

impl ProviderRegistry {
    pub fn default_registry() -> Self {
        let mut providers = HashMap::new();
        providers.insert(
            "openai".to_string(),
            ProviderDef {
                id: "openai".to_string(),
                display: "OpenAI".to_string(),
                api_base: "https://api.openai.com/v1".to_string(),
                models_endpoint: "https://api.openai.com/v1/models".to_string(),
                api_key_env: Some("OPENAI_API_KEY".to_string()),
                default_model: "gpt-4o".to_string(),
                static_models: vec![
                    "gpt-4o".to_string(),
                    "gpt-4.1".to_string(),
                    "gpt-4o-mini".to_string(),
                ],
                requires_auth: true,
            },
        );
        providers.insert(
            "anthropic".to_string(),
            ProviderDef {
                id: "anthropic".to_string(),
                display: "Anthropic".to_string(),
                api_base: "https://api.anthropic.com/v1".to_string(),
                models_endpoint: "https://api.anthropic.com/v1/models".to_string(),
                api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
                default_model: "claude-sonnet-4-20250514".to_string(),
                static_models: vec![
                    "claude-sonnet-4-20250514".to_string(),
                    "claude-3-5-sonnet-20240620".to_string(),
                    "claude-3-opus-20240229".to_string(),
                ],
                requires_auth: true,
            },
        );
        providers.insert(
            "google".to_string(),
            ProviderDef {
                id: "google".to_string(),
                display: "Google Gemini".to_string(),
                api_base: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                models_endpoint: "https://generativelanguage.googleapis.com/v1beta/models"
                    .to_string(),
                api_key_env: Some("GOOGLE_API_KEY".to_string()),
                default_model: "gemini-1.5-pro".to_string(),
                static_models: vec!["gemini-1.5-pro".to_string(), "gemini-1.5-flash".to_string()],
                requires_auth: true,
            },
        );
        providers.insert(
            "ollama".to_string(),
            ProviderDef {
                id: "ollama".to_string(),
                display: "Ollama (Local)".to_string(),
                api_base: "http://localhost:11434".to_string(),
                models_endpoint: "http://localhost:11434/api/tags".to_string(),
                api_key_env: None,
                default_model: "llama3.1".to_string(),
                static_models: vec!["llama3.1".to_string(), "mistral".to_string()],
                requires_auth: false,
            },
        );
        providers.insert(
            "custom".to_string(),
            ProviderDef {
                id: "custom".to_string(),
                display: "Custom OpenAI-compatible".to_string(),
                api_base: "http://localhost:8000/v1".to_string(),
                models_endpoint: "http://localhost:8000/v1/models".to_string(),
                api_key_env: Some("BORGCLAW_API_KEY".to_string()),
                default_model: "custom-model".to_string(),
                static_models: vec!["custom-model".to_string()],
                requires_auth: false,
            },
        );
        Self { providers }
    }

    pub fn load_or_create(path: &Path) -> Result<Self, String> {
        if path.exists() {
            let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            toml::from_str(&content).map_err(|e| e.to_string())
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let registry = Self::default_registry();
            let content = toml::to_string_pretty(&registry).map_err(|e| e.to_string())?;
            std::fs::write(path, content).map_err(|e| e.to_string())?;
            Ok(registry)
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, content).map_err(|e| e.to_string())
    }
}
