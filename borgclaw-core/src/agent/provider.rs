use super::session::{Message, MessageRole};
use crate::{config::SecurityConfig, security::SecurityLayer};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl From<&Message> for ChatMessage {
    fn from(value: &Message) -> Self {
        let role = match value.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        };

        Self {
            role: role.to_string(),
            content: value.content.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequest {
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub messages: Vec<ChatMessage>,
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    async fn complete(&self, request: &ProviderRequest) -> Result<String, ProviderError>;
}

pub struct ProviderFactory;

impl ProviderFactory {
    pub async fn create(
        config: &crate::config::AgentConfig,
        security_config: &SecurityConfig,
    ) -> Result<Box<dyn ChatProvider>, ProviderError> {
        match config.provider.as_str() {
            "openai" => Ok(Box::new(
                OpenAiProvider::from_security(security_config).await?,
            )),
            "anthropic" => Ok(Box::new(
                AnthropicProvider::from_security(security_config).await?,
            )),
            "google" => Ok(Box::new(
                GoogleProvider::from_security(security_config).await?,
            )),
            "ollama" => Ok(Box::new(OllamaProvider::default())),
            other => Err(ProviderError::UnsupportedProvider(other.to_string())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("missing environment variable: {0}")]
    MissingEnv(&'static str),
    #[error("unsupported provider: {0}")]
    UnsupportedProvider(String),
    #[error("request failed: {0}")]
    Request(String),
    #[error("response parse failed: {0}")]
    Parse(String),
}

struct OpenAiProvider {
    api_key: String,
    http: reqwest::Client,
}

impl OpenAiProvider {
    async fn from_security(config: &SecurityConfig) -> Result<Self, ProviderError> {
        Ok(Self {
            api_key: resolve_provider_secret(config, "OPENAI_API_KEY").await?,
            http: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl ChatProvider for OpenAiProvider {
    async fn complete(&self, request: &ProviderRequest) -> Result<String, ProviderError> {
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a str,
            temperature: f32,
            max_tokens: u32,
            messages: &'a [ChatMessage],
        }

        #[derive(Deserialize)]
        struct Response {
            choices: Vec<Choice>,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: ChoiceMessage,
        }

        #[derive(Deserialize)]
        struct ChoiceMessage {
            content: String,
        }

        let response = self
            .http
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&Body {
                model: &request.model,
                temperature: request.temperature,
                max_tokens: request.max_tokens,
                messages: &request.messages,
            })
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Request(format!(
                "http {}",
                response.status()
            )));
        }

        let body: Response = response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        body.choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .ok_or_else(|| ProviderError::Parse("missing choice".to_string()))
    }
}

struct AnthropicProvider {
    api_key: String,
    http: reqwest::Client,
}

impl AnthropicProvider {
    async fn from_security(config: &SecurityConfig) -> Result<Self, ProviderError> {
        Ok(Self {
            api_key: resolve_provider_secret(config, "ANTHROPIC_API_KEY").await?,
            http: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl ChatProvider for AnthropicProvider {
    async fn complete(&self, request: &ProviderRequest) -> Result<String, ProviderError> {
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a str,
            max_tokens: u32,
            temperature: f32,
            system: String,
            messages: Vec<AnthropicMessage<'a>>,
        }

        #[derive(Serialize)]
        struct AnthropicMessage<'a> {
            role: &'a str,
            content: &'a str,
        }

        #[derive(Deserialize)]
        struct Response {
            content: Vec<Block>,
        }

        #[derive(Deserialize)]
        struct Block {
            #[serde(rename = "type")]
            block_type: String,
            text: Option<String>,
        }

        let mut system_parts = Vec::new();
        let mut messages = Vec::new();
        for message in &request.messages {
            if message.role == "system" {
                system_parts.push(message.content.clone());
            } else {
                messages.push(AnthropicMessage {
                    role: &message.role,
                    content: &message.content,
                });
            }
        }

        let response = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&Body {
                model: &request.model,
                max_tokens: request.max_tokens,
                temperature: request.temperature,
                system: system_parts.join("\n\n"),
                messages,
            })
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Request(format!(
                "http {}",
                response.status()
            )));
        }

        let body: Response = response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        let text = body
            .content
            .into_iter()
            .filter(|block| block.block_type == "text")
            .filter_map(|block| block.text)
            .collect::<Vec<_>>()
            .join("\n");

        if text.is_empty() {
            Err(ProviderError::Parse("missing text block".to_string()))
        } else {
            Ok(text)
        }
    }
}

struct GoogleProvider {
    api_key: String,
    http: reqwest::Client,
}

impl GoogleProvider {
    async fn from_security(config: &SecurityConfig) -> Result<Self, ProviderError> {
        Ok(Self {
            api_key: resolve_provider_secret(config, "GOOGLE_API_KEY").await?,
            http: reqwest::Client::new(),
        })
    }
}

async fn resolve_provider_secret(
    config: &SecurityConfig,
    env_key: &'static str,
) -> Result<String, ProviderError> {
    if let Ok(value) = std::env::var(env_key) {
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }

    SecurityLayer::with_config(config.clone())
        .get_secret(env_key)
        .await
        .ok_or(ProviderError::MissingEnv(env_key))
}

#[async_trait]
impl ChatProvider for GoogleProvider {
    async fn complete(&self, request: &ProviderRequest) -> Result<String, ProviderError> {
        #[derive(Serialize)]
        struct Body<'a> {
            contents: Vec<Content<'a>>,
            generation_config: GenerationConfig<'a>,
            system_instruction: Option<Instruction<'a>>,
        }

        #[derive(Serialize)]
        struct GenerationConfig<'a> {
            temperature: f32,
            max_output_tokens: u32,
            #[serde(skip_serializing_if = "str::is_empty")]
            response_mime_type: &'a str,
        }

        #[derive(Serialize)]
        struct Instruction<'a> {
            parts: Vec<Part<'a>>,
        }

        #[derive(Serialize)]
        struct Content<'a> {
            role: &'a str,
            parts: Vec<Part<'a>>,
        }

        #[derive(Serialize)]
        struct Part<'a> {
            text: &'a str,
        }

        #[derive(Deserialize)]
        struct Response {
            candidates: Option<Vec<Candidate>>,
        }

        #[derive(Deserialize)]
        struct Candidate {
            content: CandidateContent,
        }

        #[derive(Deserialize)]
        struct CandidateContent {
            parts: Vec<CandidatePart>,
        }

        #[derive(Deserialize)]
        struct CandidatePart {
            text: Option<String>,
        }

        let mut system = Vec::new();
        let mut contents = Vec::new();
        for message in &request.messages {
            match message.role.as_str() {
                "system" => system.push(message.content.as_str()),
                "assistant" => contents.push(Content {
                    role: "model",
                    parts: vec![Part {
                        text: &message.content,
                    }],
                }),
                _ => contents.push(Content {
                    role: "user",
                    parts: vec![Part {
                        text: &message.content,
                    }],
                }),
            }
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            request.model, self.api_key
        );

        let response = self
            .http
            .post(url)
            .json(&Body {
                contents,
                generation_config: GenerationConfig {
                    temperature: request.temperature,
                    max_output_tokens: request.max_tokens,
                    response_mime_type: "",
                },
                system_instruction: if system.is_empty() {
                    None
                } else {
                    Some(Instruction {
                        parts: system.into_iter().map(|text| Part { text }).collect(),
                    })
                },
            })
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Request(format!(
                "http {}",
                response.status()
            )));
        }

        let body: Response = response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        body.candidates
            .and_then(|candidates| candidates.into_iter().next())
            .map(|candidate| {
                candidate
                    .content
                    .parts
                    .into_iter()
                    .filter_map(|part| part.text)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|text| !text.is_empty())
            .ok_or_else(|| ProviderError::Parse("missing candidate text".to_string()))
    }
}

struct OllamaProvider {
    http: reqwest::Client,
    base_url: String,
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string()),
        }
    }
}

#[async_trait]
impl ChatProvider for OllamaProvider {
    async fn complete(&self, request: &ProviderRequest) -> Result<String, ProviderError> {
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a str,
            messages: &'a [ChatMessage],
            stream: bool,
            options: Options,
        }

        #[derive(Serialize)]
        struct Options {
            temperature: f32,
            num_predict: u32,
        }

        #[derive(Deserialize)]
        struct Response {
            message: OllamaMessage,
        }

        #[derive(Deserialize)]
        struct OllamaMessage {
            content: String,
        }

        let response = self
            .http
            .post(format!("{}/api/chat", self.base_url))
            .json(&Body {
                model: &request.model,
                messages: &request.messages,
                stream: false,
                options: Options {
                    temperature: request.temperature,
                    num_predict: request.max_tokens,
                },
            })
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Request(format!(
                "http {}",
                response.status()
            )));
        }

        let body: Response = response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        Ok(body.message.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Message, MessageRole};

    #[test]
    fn converts_session_message_to_chat_message() {
        let message = Message {
            id: "1".to_string(),
            role: MessageRole::Assistant,
            content: "hello".to_string(),
            timestamp: chrono::Utc::now(),
            tool_calls: Vec::new(),
        };

        let chat = ChatMessage::from(&message);
        assert_eq!(chat.role, "assistant");
        assert_eq!(chat.content, "hello");
    }

    #[tokio::test]
    async fn rejects_unknown_provider() {
        let config = crate::config::AgentConfig {
            provider: "unknown".to_string(),
            ..Default::default()
        };

        assert!(matches!(
            ProviderFactory::create(&config, &SecurityConfig::default()).await,
            Err(ProviderError::UnsupportedProvider(_))
        ));
    }

    #[tokio::test]
    async fn provider_factory_reads_secret_store_when_env_missing() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_provider_secret_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut security = SecurityConfig::default();
        security.secrets_path = root.join("secrets.enc");
        SecurityLayer::with_config(security.clone())
            .store_secret("OPENAI_API_KEY", "from-store")
            .await
            .unwrap();

        let config = crate::config::AgentConfig {
            provider: "openai".to_string(),
            ..Default::default()
        };

        assert!(ProviderFactory::create(&config, &security).await.is_ok());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
