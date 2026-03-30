use super::session::{Message, MessageRole, TranscriptArtifacts};
use crate::{
    config::{AgentConfig, SecurityConfig},
    security::SecurityLayer,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Duration;

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub text: String,
    #[serde(default)]
    pub artifacts: TranscriptArtifacts,
}

impl ProviderResponse {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            artifacts: TranscriptArtifacts::default(),
        }
    }

    fn from_text_with_think_blocks(text: String, provider_name: &'static str) -> Self {
        let mut provider_metadata = std::collections::HashMap::new();
        provider_metadata.insert("provider".to_string(), provider_name.to_string());
        Self {
            artifacts: TranscriptArtifacts {
                reasoning: extract_think_blocks(&text),
                provider_metadata,
            },
            text,
        }
    }
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    async fn complete(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError>;
}

struct RateLimitedProvider {
    inner: Box<dyn ChatProvider>,
    rate_limiter: Arc<RwLock<RateLimiter>>,
}

struct RateLimiter {
    requests_per_minute: u32,
    request_times: Vec<std::time::Instant>,
}

impl RateLimiter {
    fn new(requests_per_minute: u32) -> Self {
        Self {
            requests_per_minute,
            request_times: Vec::new(),
        }
    }

    async fn acquire(&mut self) -> Result<(), ProviderError> {
        let now = std::time::Instant::now();
        let window = Duration::from_secs(60);

        self.request_times
            .retain(|&t| now.duration_since(t) < window);

        if self.request_times.len() >= self.requests_per_minute as usize {
            let oldest = self.request_times.first().unwrap();
            let wait_time = window - now.duration_since(*oldest);
            tokio::time::sleep(wait_time).await;
            self.request_times
                .retain(|&t| std::time::Instant::now().duration_since(t) < window);
        }

        self.request_times.push(std::time::Instant::now());
        Ok(())
    }
}

#[async_trait]
impl ChatProvider for RateLimitedProvider {
    async fn complete(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        const MAX_RETRIES: u32 = 3;
        let mut backoff_ms = 1000;

        for attempt in 0..MAX_RETRIES {
            {
                let mut limiter = self.rate_limiter.write().await;
                limiter.acquire().await?;
            }

            match self.inner.complete(request).await {
                Ok(result) => return Ok(result),
                Err(ProviderError::RateLimited(retry_after)) => {
                    if attempt + 1 >= MAX_RETRIES {
                        return Err(ProviderError::RateLimited(retry_after));
                    }
                    tokio::time::sleep(Duration::from_millis(retry_after * 1000)).await;
                    backoff_ms = (backoff_ms * 2).min(30000);
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }
}

pub struct ProviderFactory;

impl ProviderFactory {
    pub async fn create(
        config: &crate::config::AgentConfig,
        security_config: &SecurityConfig,
    ) -> Result<Box<dyn ChatProvider>, ProviderError> {
        tracing::info!(
            "Creating provider: {} with model: {}",
            config.provider,
            config.model
        );

        let provider: Box<dyn ChatProvider> = match config.provider.as_str() {
            "openai" => Box::new(OpenAiProvider::from_security(config, security_config).await?),
            "anthropic" => {
                Box::new(AnthropicProvider::from_security(config, security_config).await?)
            }
            "google" => Box::new(GoogleProvider::from_security(config, security_config).await?),
            "ollama" => Box::new(OllamaProvider::default()),
            "kimi" => Box::new(KimiProvider::from_security(config, security_config).await?),
            "minimax" => Box::new(MiniMaxProvider::from_security(config, security_config).await?),
            "z" => Box::new(ZProvider::from_security(config, security_config).await?),
            other => return Err(ProviderError::UnsupportedProvider(other.to_string())),
        };

        let rate_limit_rpm = config
            .rate_limit_rpm
            .unwrap_or(match config.provider.as_str() {
                "openai" => 60,
                "anthropic" => 50,
                "google" => 15,
                "kimi" => 30,
                "minimax" => 30,
                "z" => 30,
                "ollama" => 120,
                _ => 60,
            });

        Ok(Box::new(RateLimitedProvider {
            inner: provider,
            rate_limiter: Arc::new(RwLock::new(RateLimiter::new(rate_limit_rpm))),
        }))
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
    #[error("rate limited, retry after {0} seconds")]
    RateLimited(u64),
}

struct OpenAiProvider {
    api_key: String,
    http: reqwest::Client,
}

impl OpenAiProvider {
    async fn from_security(
        agent: &AgentConfig,
        config: &SecurityConfig,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            api_key: resolve_provider_secret(agent, config, "OPENAI_API_KEY").await?,
            http: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl ChatProvider for OpenAiProvider {
    async fn complete(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
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
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            if status.as_u16() == 429 {
                let retry_after = error_body.parse().ok().unwrap_or(60);
                return Err(ProviderError::RateLimited(retry_after));
            }
            return Err(ProviderError::Request(format!(
                "http {}: {}",
                status, error_body
            )));
        }

        let body: Response = response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        body.choices
            .into_iter()
            .next()
            .map(|choice| ProviderResponse::text(choice.message.content))
            .ok_or_else(|| ProviderError::Parse("missing choice".to_string()))
    }
}

struct AnthropicProvider {
    api_key: String,
    http: reqwest::Client,
}

impl AnthropicProvider {
    async fn from_security(
        agent: &AgentConfig,
        config: &SecurityConfig,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            api_key: resolve_provider_secret(agent, config, "ANTHROPIC_API_KEY").await?,
            http: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl ChatProvider for AnthropicProvider {
    async fn complete(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
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

        // Anthropic requires at least one non-system message
        if messages.is_empty() {
            return Err(ProviderError::Request(
                "Anthropic requires at least one user or assistant message".to_string(),
            ));
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
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            if status.as_u16() == 429 {
                let retry_after = error_body.parse().ok().unwrap_or(60);
                return Err(ProviderError::RateLimited(retry_after));
            }
            return Err(ProviderError::Request(format!(
                "http {}: {}",
                status, error_body
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
            Ok(ProviderResponse::text(text))
        }
    }
}

struct GoogleProvider {
    api_key: String,
    http: reqwest::Client,
}

impl GoogleProvider {
    async fn from_security(
        agent: &AgentConfig,
        config: &SecurityConfig,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            api_key: resolve_provider_secret(agent, config, "GOOGLE_API_KEY").await?,
            http: reqwest::Client::new(),
        })
    }
}

async fn resolve_provider_secret(
    agent: &AgentConfig,
    config: &SecurityConfig,
    env_key: &'static str,
) -> Result<String, ProviderError> {
    if let Some(profile_id) = agent.provider_profile.as_deref() {
        if let Some(profile) = SecurityLayer::with_config(config.clone())
            .get_provider_profile(profile_id)
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?
        {
            let env_matches = profile
                .env_key
                .as_deref()
                .map(|value| value == env_key)
                .unwrap_or(true);
            if profile.provider == agent.provider && env_matches {
                if let Some(api_key) = profile.api_key {
                    if !api_key.trim().is_empty() {
                        return Ok(api_key);
                    }
                }
            }
        }
    }

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
    async fn complete(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
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
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            if status.as_u16() == 429 {
                let retry_after = error_body.parse().ok().unwrap_or(60);
                return Err(ProviderError::RateLimited(retry_after));
            }
            return Err(ProviderError::Request(format!(
                "http {}: {}",
                status, error_body
            )));
        }

        let body: Response = response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        body.candidates
            .and_then(|candidates| candidates.into_iter().next())
            .map(|candidate| {
                ProviderResponse::text(
                    candidate
                        .content
                        .parts
                        .into_iter()
                        .filter_map(|part| part.text)
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            })
            .filter(|response| !response.text.is_empty())
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
    async fn complete(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
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
            if response.status().as_u16() == 429 {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                return Err(ProviderError::RateLimited(retry_after));
            }
            return Err(ProviderError::Request(format!(
                "http {}",
                response.status()
            )));
        }

        let body: Response = response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        Ok(ProviderResponse::text(body.message.content))
    }
}

/// Macro to generate OpenAI-compatible providers
///
/// API base URL can be overridden via {PROVIDER}_API_BASE env var
/// e.g., OPENAI_API_BASE, ANTHROPIC_API_BASE, KIMI_API_BASE, etc.
macro_rules! openai_compatible_provider {
    ($name:ident, $env_key:expr, $default_base_url:expr, $reasoning_split:expr) => {
        struct $name {
            api_key: String,
            http: reqwest::Client,
            base_url: String,
        }

        impl $name {
            async fn from_security(
                agent: &AgentConfig,
                config: &SecurityConfig,
            ) -> Result<Self, ProviderError> {
                let api_key = resolve_provider_secret(agent, config, $env_key).await?;
                // Allow API base URL override via env var
                let base_url = Self::resolve_base_url();
                Ok(Self {
                    api_key,
                    http: reqwest::Client::new(),
                    base_url,
                })
            }

            fn reasoning_split() -> Option<bool> {
                $reasoning_split
            }

            fn build_request_body(request: &ProviderRequest) -> serde_json::Value {
                let mut body = serde_json::json!({
                    "model": request.model,
                    "temperature": request.temperature,
                    "max_tokens": request.max_tokens,
                    "messages": request.messages,
                });

                if let Some(reasoning_split) = Self::reasoning_split() {
                    body["reasoning_split"] = serde_json::json!(reasoning_split);
                }

                body
            }

            fn resolve_base_url() -> String {
                // Extract provider name from the struct name (e.g., "KimiProvider" -> "KIMI")
                let provider_name = stringify!($name)
                    .trim_end_matches("Provider")
                    .to_uppercase();
                let env_var_name = format!("{}_API_BASE", provider_name);

                // Check for env var override
                if let Ok(custom_base) = std::env::var(&env_var_name) {
                    if !custom_base.is_empty() {
                        tracing::info!(
                            "Using custom API base from {}: {}",
                            env_var_name,
                            custom_base
                        );
                        return custom_base.trim_end_matches('/').to_string();
                    }
                }

                // Fallback to default
                $default_base_url.to_string()
            }
        }

        #[async_trait]
        impl ChatProvider for $name {
            async fn complete(
                &self,
                request: &ProviderRequest,
            ) -> Result<ProviderResponse, ProviderError> {
                tracing::info!(
                    "{} request: model={}, temperature={}, max_tokens={}",
                    stringify!($name),
                    request.model,
                    request.temperature,
                    request.max_tokens
                );

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

                let url = format!("{}/chat/completions", self.base_url);
                let response = self
                    .http
                    .post(&url)
                    .bearer_auth(&self.api_key)
                    .json(&Self::build_request_body(request))
                    .send()
                    .await
                    .map_err(|e| ProviderError::Request(e.to_string()))?;

                if !response.status().is_success() {
                    if response.status().as_u16() == 429 {
                        let retry_after = response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(60);
                        return Err(ProviderError::RateLimited(retry_after));
                    }
                    let status = response.status();
                    let error_body = response.text().await.unwrap_or_default();
                    return Err(ProviderError::Request(format!(
                        "http {}: {}",
                        status, error_body
                    )));
                }

                let body: Response = response
                    .json()
                    .await
                    .map_err(|e| ProviderError::Parse(e.to_string()))?;

                body.choices
                    .into_iter()
                    .next()
                    .map(|choice| {
                        ProviderResponse::from_text_with_think_blocks(
                            choice.message.content,
                            stringify!($name),
                        )
                    })
                    .ok_or_else(|| ProviderError::Parse("missing choice".to_string()))
            }
        }
    };
}

// Generate OpenAI-compatible providers
openai_compatible_provider!(
    KimiProvider,
    "KIMI_API_KEY",
    "https://api.moonshot.cn/v1",
    None
);
/// MiniMax provider with system message handling
/// MiniMax API doesn't support "system" role, so we prepend system content to the first user message
struct MiniMaxProvider {
    api_key: String,
    http: reqwest::Client,
    base_url: String,
}

impl MiniMaxProvider {
    async fn from_security(
        agent: &AgentConfig,
        config: &SecurityConfig,
    ) -> Result<Self, ProviderError> {
        let api_key = resolve_provider_secret(agent, config, "MINIMAX_API_KEY").await?;
        let base_url = Self::resolve_base_url();
        Ok(Self {
            api_key,
            http: reqwest::Client::new(),
            base_url,
        })
    }

    fn resolve_base_url() -> String {
        if let Ok(custom_base) = std::env::var("MINIMAX_API_BASE") {
            if !custom_base.is_empty() {
                tracing::info!("Using custom API base from MINIMAX_API_BASE: {}", custom_base);
                return custom_base.trim_end_matches('/').to_string();
            }
        }
        "https://api.minimax.io/v1".to_string()
    }

    /// Convert messages for MiniMax API by handling system messages
    /// System messages are prepended to the first user message
    fn convert_messages(messages: &[ChatMessage]) -> Vec<ChatMessage> {
        let mut system_content = Vec::new();
        let mut converted = Vec::new();
        let mut first_user = true;

        for msg in messages {
            match msg.role.as_str() {
                "system" => {
                    system_content.push(msg.content.clone());
                }
                "user" => {
                    if first_user && !system_content.is_empty() {
                        // Prepend system content to first user message
                        let combined = format!(
                            "{}",
                            system_content.join("\n\n")
                        );
                        converted.push(ChatMessage {
                            role: "user".to_string(),
                            content: format!("{}\n\n{}", combined, msg.content),
                        });
                        first_user = false;
                    } else {
                        converted.push(msg.clone());
                    }
                }
                _ => {
                    converted.push(msg.clone());
                }
            }
        }

        // If we only had system messages (no user message), convert to a single user message
        if converted.is_empty() && !system_content.is_empty() {
            converted.push(ChatMessage {
                role: "user".to_string(),
                content: system_content.join("\n\n"),
            });
        }

        converted
    }

    fn build_request_body(request: &ProviderRequest) -> serde_json::Value {
        let converted_messages = Self::convert_messages(&request.messages);
        
        serde_json::json!({
            "model": request.model,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "messages": converted_messages,
            "reasoning_split": false,
        })
    }
}

#[async_trait]
impl ChatProvider for MiniMaxProvider {
    async fn complete(
        &self,
        request: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        tracing::info!(
            "MiniMaxProvider request: model={}, temperature={}, max_tokens={}",
            request.model,
            request.temperature,
            request.max_tokens
        );

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

        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&Self::build_request_body(request))
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if !response.status().is_success() {
            if response.status().as_u16() == 429 {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60);
                return Err(ProviderError::RateLimited(retry_after));
            }
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Request(format!(
                "http {}: {}",
                status, error_body
            )));
        }

        let body: Response = response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        body.choices
            .into_iter()
            .next()
            .map(|choice| {
                ProviderResponse::from_text_with_think_blocks(
                    choice.message.content,
                    "MiniMaxProvider",
                )
            })
            .ok_or_else(|| ProviderError::Parse("missing choice".to_string()))
    }
}
openai_compatible_provider!(ZProvider, "Z_API_KEY", "https://api.z.ai/api/paas/v4", None);

fn extract_think_blocks(text: &str) -> Option<String> {
    let mut cursor = text;
    let mut blocks = Vec::new();

    while let Some(start) = cursor.find("<think>") {
        let after_start = &cursor[start + "<think>".len()..];
        let Some(end) = after_start.find("</think>") else {
            break;
        };
        let segment = after_start[..end].trim();
        if !segment.is_empty() {
            blocks.push(segment.to_string());
        }
        cursor = &after_start[end + "</think>".len()..];
    }

    if blocks.is_empty() {
        None
    } else {
        Some(blocks.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Message, MessageRole};
    use crate::security::ProviderProfile;

    #[test]
    fn converts_session_message_to_chat_message() {
        let message = Message {
            id: "1".to_string(),
            role: MessageRole::Assistant,
            content: "hello".to_string(),
            timestamp: chrono::Utc::now(),
            tool_calls: Vec::new(),
            artifacts: TranscriptArtifacts::default(),
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
        let security = SecurityConfig {
            secrets_path: root.join("secrets.enc"),
            ..Default::default()
        };
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

    #[tokio::test]
    async fn provider_factory_reads_selected_provider_profile() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_provider_profile_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let security = SecurityConfig {
            secrets_path: root.join("secrets.enc"),
            ..Default::default()
        };
        SecurityLayer::with_config(security.clone())
            .upsert_provider_profile(ProviderProfile {
                id: "openai-work".to_string(),
                provider: "openai".to_string(),
                env_key: Some("OPENAI_API_KEY".to_string()),
                api_key: Some("profile-key".to_string()),
                model: Some("gpt-4o".to_string()),
            })
            .await
            .unwrap();

        let config = crate::config::AgentConfig {
            provider: "openai".to_string(),
            provider_profile: Some("openai-work".to_string()),
            ..Default::default()
        };

        assert!(ProviderFactory::create(&config, &security).await.is_ok());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn minimax_request_body_converts_system_messages() {
        let request = ProviderRequest {
            model: "MiniMax-M2.7".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "You are helpful.".to_string(),
                },
                ChatMessage {
                    role: "assistant".to_string(),
                    content: "<think>reasoning</think>Hello".to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: "Second turn".to_string(),
                },
            ],
        };

        let body = MiniMaxProvider::build_request_body(&request);
        
        // Assistant message preserved first (comes before user in original)
        assert_eq!(body["messages"][0]["role"], "assistant");
        assert_eq!(
            body["messages"][0]["content"],
            "<think>reasoning</think>Hello"
        );
        // System message should be prepended to first user message
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(
            body["messages"][1]["content"],
            "You are helpful.\n\nSecond turn"
        );
        // reasoning_split should be false for MiniMax
        assert_eq!(body["reasoning_split"], serde_json::json!(false));
    }

    #[test]
    fn minimax_convert_messages_handles_multiple_system() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "System 1".to_string(),
            },
            ChatMessage {
                role: "system".to_string(),
                content: "System 2".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "User message".to_string(),
            },
        ];

        let converted = MiniMaxProvider::convert_messages(&messages);
        
        // Should have one message: system content prepended to user message
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[0].content, "System 1\n\nSystem 2\n\nUser message");
    }

    #[test]
    fn provider_response_extracts_think_blocks_into_artifacts() {
        let response = ProviderResponse::from_text_with_think_blocks(
            "<think>internal</think>Visible".to_string(),
            "MiniMaxProvider",
        );

        assert_eq!(response.text, "<think>internal</think>Visible");
        assert_eq!(response.artifacts.reasoning.as_deref(), Some("internal"));
        assert_eq!(
            response
                .artifacts
                .provider_metadata
                .get("provider")
                .map(String::as_str),
            Some("MiniMaxProvider")
        );
    }
}
