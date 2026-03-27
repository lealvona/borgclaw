use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDef {
    #[serde(default)]
    pub id: String,
    #[serde(alias = "name")]
    pub display: String,
    pub api_base: String,
    #[serde(default)]
    pub models_endpoint: String,
    #[serde(
        default,
        rename = "env_key",
        alias = "api_key_env",
        deserialize_with = "deserialize_env_key",
        serialize_with = "serialize_env_key"
    )]
    pub api_key_env: Option<String>,
    pub default_model: String,
    #[serde(default)]
    pub static_models: Vec<String>,
    #[serde(default = "default_requires_auth")]
    pub requires_auth: bool,
    #[serde(default)]
    pub rate_limit_rpm: Option<u32>,
}

impl Default for ProviderDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            display: String::new(),
            api_base: String::new(),
            models_endpoint: String::new(),
            api_key_env: None,
            default_model: String::new(),
            static_models: Vec::new(),
            requires_auth: true,
            rate_limit_rpm: None,
        }
    }
}

impl ProviderDef {
    pub fn rate_limit_rpm_with_default(&self) -> u32 {
        self.rate_limit_rpm
            .unwrap_or_else(|| default_rate_limit_for_provider(&self.id))
    }
}

fn default_rate_limit_for_provider(id: &str) -> u32 {
    match id {
        "openai" => 60,
        "anthropic" => 50,
        "google" => 15,
        "kimi" => 30,
        "minimax" => 30,
        "z" => 30,
        "ollama" => 120,
        _ => 30,
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProviderRegistry {
    #[serde(flatten)]
    pub providers: HashMap<String, ProviderDef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct LegacyProviderRegistry {
    #[serde(default)]
    providers: HashMap<String, ProviderDef>,
}

impl<'de> Deserialize<'de> for ProviderRegistry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = toml::Value::deserialize(deserializer)?;
        let table = raw
            .as_table()
            .cloned()
            .ok_or_else(|| serde::de::Error::custom("expected provider registry table"))?;

        if table.contains_key("providers") {
            let legacy: LegacyProviderRegistry =
                raw.try_into().map_err(serde::de::Error::custom)?;
            return Ok(Self {
                providers: legacy.providers,
            });
        }

        let mut providers = HashMap::new();
        for (id, value) in table {
            let mut provider: ProviderDef = value.try_into().map_err(serde::de::Error::custom)?;
            if provider.id.is_empty() {
                provider.id = id.clone();
            }
            provider.finalize_defaults();
            providers.insert(id, provider);
        }

        Ok(Self { providers })
    }
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
                ..Default::default()
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
                ..Default::default()
            },
        );
        providers.insert(
            "google".to_string(),
            ProviderDef {
                id: "google".to_string(),
                display: "Google AI".to_string(),
                api_base: "https://generativelanguage.googleapis.com/v1".to_string(),
                models_endpoint: "https://generativelanguage.googleapis.com/v1beta/models"
                    .to_string(),
                api_key_env: Some("GOOGLE_API_KEY".to_string()),
                default_model: "gemini-2.5-pro".to_string(),
                static_models: vec![
                    "gemini-2.5-pro".to_string(),
                    "gemini-2.5-flash".to_string(),
                    "gemini-2.0-flash".to_string(),
                ],
                requires_auth: true,
                ..Default::default()
            },
        );
        providers.insert(
            "kimi".to_string(),
            ProviderDef {
                id: "kimi".to_string(),
                display: "Kimi (Moonshot)".to_string(),
                api_base: "https://api.moonshot.ai/v1".to_string(),
                models_endpoint: "https://api.moonshot.ai/v1/models".to_string(),
                api_key_env: Some("KIMI_API_KEY".to_string()),
                default_model: "kimi-k2.5".to_string(),
                static_models: vec!["kimi-k2.5".to_string(), "kimi-k2".to_string()],
                requires_auth: true,
                ..Default::default()
            },
        );
        providers.insert(
            "minimax".to_string(),
            ProviderDef {
                id: "minimax".to_string(),
                display: "MiniMax".to_string(),
                api_base: "https://api.minimax.io/v1".to_string(),
                models_endpoint: "".to_string(),  // Model listing not supported - see https://github.com/MiniMax-AI/MiniMax-M2/issues/60
                api_key_env: Some("MINIMAX_API_KEY".to_string()),
                default_model: "MiniMax-M2.7".to_string(),
                // Updated model list from https://platform.minimax.io/docs/guides/models-intro (Mar 2026)
                static_models: vec![
                    "MiniMax-M2.7".to_string(),
                    "MiniMax-M2.7-highspeed".to_string(),
                    "MiniMax-M2.5".to_string(),
                    "M2-her".to_string(),
                ],
                requires_auth: true,
                ..Default::default()
            },
        );
        providers.insert(
            "z".to_string(),
            ProviderDef {
                id: "z".to_string(),
                display: "Z.ai".to_string(),
                api_base: "https://api.z.ai/api/paas/v4".to_string(),
                models_endpoint: "".to_string(),  // Model listing not supported
                api_key_env: Some("Z_API_KEY".to_string()),
                default_model: "glm-4.7".to_string(),
                // Updated model list from https://docs.z.ai/ (Mar 2026)
                static_models: vec![
                    "glm-4.7".to_string(),
                    "glm-4.6".to_string(),
                    "glm-4.5".to_string(),
                    "glm-4".to_string(),
                    "glm-4-air".to_string(),
                    "glm-4-airx".to_string(),
                    "glm-4-flash".to_string(),
                    "glm-4v".to_string(),
                ],
                requires_auth: true,
                ..Default::default()
            },
        );
        providers.insert(
            "ollama".to_string(),
            ProviderDef {
                id: "ollama".to_string(),
                display: "Ollama (Local)".to_string(),
                api_base: "http://localhost:11434/api".to_string(),
                models_endpoint: "http://localhost:11434/api/tags".to_string(),
                api_key_env: None,
                default_model: "llama3".to_string(),
                static_models: vec!["llama3".to_string(), "mistral".to_string()],
                requires_auth: false,
                ..Default::default()
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
                ..Default::default()
            },
        );
        Self { providers }
    }

    pub fn load_or_create(path: &Path) -> Result<Self, String> {
        if path.exists() {
            let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            let mut registry: Self = toml::from_str(&content).map_err(|e| e.to_string())?;
            for (id, provider) in &mut registry.providers {
                if provider.id.is_empty() {
                    provider.id = id.clone();
                }
                provider.finalize_defaults();
            }
            Ok(registry)
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

impl ProviderDef {
    fn finalize_defaults(&mut self) {
        if self.models_endpoint.is_empty() {
            self.models_endpoint = default_models_endpoint(&self.id, &self.api_base);
        }
        if self.static_models.is_empty() {
            self.static_models = default_static_models(&self.id, &self.default_model);
        }
    }
}

fn default_requires_auth() -> bool {
    true
}

fn deserialize_env_key<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }))
}

fn serialize_env_key<S>(value: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(value.as_deref().unwrap_or(""))
}

fn default_models_endpoint(id: &str, api_base: &str) -> String {
    match id {
        "openai" => format!("{api_base}/models"),
        "anthropic" => format!("{api_base}/models"),
        "google" => "https://generativelanguage.googleapis.com/v1beta/models".to_string(),
        "kimi" => "https://api.moonshot.cn/v1/models".to_string(),
        "minimax" => "https://api.minimax.io/v1/models".to_string(),  // Note: MiniMax doesn't actually support this endpoint
        "z" => "https://api.z.ai/v1/models".to_string(),
        "ollama" => "http://localhost:11434/api/tags".to_string(),
        _ => format!("{}/models", api_base.trim_end_matches('/')),
    }
}

fn default_static_models(id: &str, default_model: &str) -> Vec<String> {
    match id {
        "openai" => vec![
            default_model.to_string(),
            "gpt-4.1".to_string(),
            "gpt-4o-mini".to_string(),
        ],
        "anthropic" => vec![
            default_model.to_string(),
            "claude-3-5-sonnet-20240620".to_string(),
            "claude-3-opus-20240229".to_string(),
        ],
        "google" => vec![
            default_model.to_string(),
            "gemini-2.5-flash".to_string(),
            "gemini-2.0-flash".to_string(),
        ],
        // Note: These providers don't support /v1/models endpoint, so we return full static lists
        "kimi" => vec![
            default_model.to_string(),
            "kimi-k2.5".to_string(),
            "kimi-k2".to_string(),
        ],
        "minimax" => vec![
            "MiniMax-M2.7".to_string(),
            "MiniMax-M2.7-highspeed".to_string(),
            "MiniMax-M2.5".to_string(),
            "M2-her".to_string(),
        ],
        "z" => vec![
            "glm-4.7".to_string(),
            "glm-4.6".to_string(),
            "glm-4.5".to_string(),
            "glm-4".to_string(),
            "glm-4-air".to_string(),
        ],
        "ollama" => vec![default_model.to_string(), "mistral".to_string()],
        _ => vec![default_model.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_registry_parses_documented_top_level_shape() {
        let registry: ProviderRegistry = toml::from_str(
            r#"
            [openai]
            name = "OpenAI"
            api_base = "https://api.openai.com/v1"
            env_key = "OPENAI_API_KEY"
            default_model = "gpt-4o"

            [ollama]
            name = "Ollama (Local)"
            api_base = "http://localhost:11434/api"
            env_key = ""
            default_model = "llama3"
            "#,
        )
        .unwrap();

        assert_eq!(registry.providers["openai"].display, "OpenAI");
        assert_eq!(
            registry.providers["openai"].api_key_env.as_deref(),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(registry.providers["openai"].id, "openai");
        assert_eq!(registry.providers["ollama"].api_key_env, None);
        assert_eq!(
            registry.providers["ollama"].models_endpoint,
            "http://localhost:11434/api/tags"
        );
    }

    #[test]
    fn provider_registry_parses_legacy_nested_shape() {
        let registry: ProviderRegistry = toml::from_str(
            r#"
            [providers.openai]
            id = "openai"
            display = "OpenAI"
            api_base = "https://api.openai.com/v1"
            models_endpoint = "https://api.openai.com/v1/models"
            api_key_env = "OPENAI_API_KEY"
            default_model = "gpt-4o"
            static_models = ["gpt-4o"]
            requires_auth = true
            "#,
        )
        .unwrap();

        assert_eq!(registry.providers["openai"].display, "OpenAI");
        assert_eq!(
            registry.providers["openai"].models_endpoint,
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn provider_registry_serializes_documented_env_key_shape() {
        let serialized = toml::to_string_pretty(&ProviderRegistry::default_registry()).unwrap();

        assert!(serialized.contains("env_key = \"OPENAI_API_KEY\""));
        assert!(serialized.contains("env_key = \"\""));
        assert!(!serialized.contains("api_key_env"));
    }
}
