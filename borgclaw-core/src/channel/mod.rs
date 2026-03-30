//! Channel module - handles different messaging channels

mod cli;
mod signal;
mod telegram;
mod traits;
mod webhook;

pub use cli::{create_cli_message, CliChannel};
pub use signal::{SignalChannel, SignalChannelBuilder};
pub use telegram::TelegramChannel;
pub use traits::{Channel, ChannelSender, ChannelStatus};
pub use webhook::{WebhookChannel, WebhookError, WebhookResponse, WebhookTrigger};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Channel type identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelType(pub String);

impl ChannelType {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn cli() -> Self {
        Self("cli".to_string())
    }
    pub fn telegram() -> Self {
        Self("telegram".to_string())
    }
    pub fn discord() -> Self {
        Self("discord".to_string())
    }
    pub fn signal() -> Self {
        Self("signal".to_string())
    }
    pub fn slack() -> Self {
        Self("slack".to_string())
    }
    pub fn whatsapp() -> Self {
        Self("whatsapp".to_string())
    }
    pub fn websocket() -> Self {
        Self("websocket".to_string())
    }
}

impl Default for ChannelType {
    fn default() -> Self {
        Self::cli()
    }
}

/// Incoming message from a channel
#[derive(Debug, Clone)]
pub struct InboundMessage {
    /// Channel type
    pub channel: ChannelType,
    /// Sender info
    pub sender: Sender,
    /// Message content
    pub content: MessagePayload,
    /// Conversation/group ID
    pub group_id: Option<String>,
    /// Message timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Raw message (channel-specific)
    pub raw: serde_json::Value,
}

/// Sender information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sender {
    /// Unique sender ID
    pub id: String,
    /// Display name
    pub name: Option<String>,
    /// Avatar URL
    pub avatar: Option<String>,
}

impl Sender {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            avatar: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Outbound message to send
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    /// Target sender ID
    pub target: String,
    /// Channel type
    pub channel: ChannelType,
    /// Message content
    pub content: MessagePayload,
    /// Reply to message ID
    pub reply_to: Option<String>,
    /// Group ID for group messages
    pub group_id: Option<String>,
}

impl OutboundMessage {
    pub fn new(target: impl Into<String>, channel: ChannelType, content: MessagePayload) -> Self {
        Self {
            target: target.into(),
            channel,
            content,
            reply_to: None,
            group_id: None,
        }
    }

    pub fn with_reply(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }

    pub fn with_group(mut self, group_id: impl Into<String>) -> Self {
        self.group_id = Some(group_id.into());
        self
    }
}

/// Message payload (text or media)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum MessagePayload {
    /// Plain text
    Text(String),
    /// Markdown
    Markdown(String),
    /// HTML
    Html(String),
    /// Media with URL
    Media { url: String, mime_type: String },
    /// File attachment
    File { path: String, name: String },
}

impl MessagePayload {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    pub fn markdown(s: impl Into<String>) -> Self {
        Self::Markdown(s.into())
    }

    pub fn media(url: impl Into<String>, mime: impl Into<String>) -> Self {
        Self::Media {
            url: url.into(),
            mime_type: mime.into(),
        }
    }
}

/// Channel error
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("Send failed: {0}")]
    SendFailed(String),
    #[error("Receive failed: {0}")]
    ReceiveFailed(String),
    #[error("Not supported: {0}")]
    NotSupported(String),
}

/// Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelConfig {
    /// Channel type
    pub channel_type: ChannelType,
    /// Enable channel
    pub enabled: bool,
    /// Bot token / credentials
    pub credentials: Option<String>,
    /// Optional outbound proxy URL for channels with network egress
    pub proxy_url: Option<String>,
    /// Allowed users (empty = allow all)
    pub allow_from: Vec<String>,
    /// DM policy
    pub dm_policy: super::config::DmPolicy,
    /// Extra configuration (channel-specific)
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, toml::Value>,
}

impl ChannelConfig {
    pub fn new(channel_type: ChannelType) -> Self {
        Self {
            channel_type,
            enabled: false,
            credentials: None,
            proxy_url: None,
            allow_from: Vec::new(),
            dm_policy: super::config::DmPolicy::Pairing,
            extra: std::collections::HashMap::new(),
        }
    }
}

/// Message router - routes messages between channels and agent
pub struct MessageRouter {
    agent: Arc<Mutex<crate::agent::SimpleAgent>>,
    security: Arc<crate::security::SecurityLayer>,
    channel_configs: HashMap<String, crate::config::ChannelConfig>,
    agent_config: crate::config::AgentConfig,
    memory_config: crate::config::MemoryConfig,
    security_config: crate::config::SecurityConfig,
    sessions: Arc<RwLock<HashMap<String, crate::agent::SessionId>>>,
}

impl MessageRouter {
    pub fn from_config(config: &crate::config::AppConfig) -> Self {
        let mut agent = crate::agent::SimpleAgent::new(
            config.agent.clone(),
            Some(config.memory.clone()),
            Some(config.heartbeat.clone()),
            Some(config.scheduler.clone()),
            Some(config.skills.clone()),
            Some(config.mcp.clone()),
            Some(config.security.clone()),
        );
        for tool in crate::agent::builtin_tools() {
            agent.register_tool(tool);
        }

        Self {
            agent: Arc::new(Mutex::new(agent)),
            security: Arc::new(crate::security::SecurityLayer::with_config(
                config.security.clone(),
            )),
            channel_configs: config.channels.clone(),
            agent_config: config.agent.clone(),
            memory_config: config.memory.clone(),
            security_config: config.security.clone(),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn request_pairing_code(&self, sender_id: &str) -> Result<String, ChannelError> {
        self.security
            .generate_pairing(sender_id)
            .await
            .map_err(|e| ChannelError::AuthFailed(e.to_string()))
    }

    pub async fn approve_pairing_code(&self, code: &str) -> Result<String, ChannelError> {
        self.security
            .approve_pairing(code)
            .await
            .map_err(|e| ChannelError::AuthFailed(e.to_string()))
    }

    pub async fn route(&self, msg: InboundMessage) -> Result<RouteOutcome, ChannelError> {
        self.enforce_sender_policy(&msg).await?;

        let session_id = self.session_for(&msg).await;
        let content = message_text(&msg.content);
        if let Some(response) = self.builtin_command_response(&msg, &content) {
            let outbound = OutboundMessage {
                target: msg.sender.id.clone(),
                channel: msg.channel.clone(),
                content: MessagePayload::text(response.text.clone()),
                reply_to: None,
                group_id: msg.group_id.clone(),
            };

            return Ok(RouteOutcome {
                session_id,
                response,
                outbound,
            });
        }

        let mut metadata = HashMap::new();
        if let Some(group_id) = &msg.group_id {
            metadata.insert("group_id".to_string(), group_id.clone());
        }

        let ctx = crate::agent::AgentContext {
            session_id: session_id.clone(),
            message: content,
            sender: crate::agent::SenderInfo {
                id: msg.sender.id.clone(),
                name: msg.sender.name.clone(),
                channel: msg.channel.0.clone(),
            },
            metadata,
        };

        let mut agent = self.agent.lock().await;
        let response = crate::agent::Agent::process(&mut *agent, &ctx).await;
        let outbound = OutboundMessage {
            target: msg.sender.id.clone(),
            channel: msg.channel.clone(),
            content: MessagePayload::text(response.text.clone()),
            reply_to: None,
            group_id: msg.group_id.clone(),
        };

        Ok(RouteOutcome {
            session_id,
            response,
            outbound,
        })
    }

    async fn enforce_sender_policy(&self, msg: &InboundMessage) -> Result<(), ChannelError> {
        let configured = self.channel_configs.get(&msg.channel.0).cloned();
        if let Some(config) = configured.as_ref() {
            if !config.enabled {
                return Err(ChannelError::AuthFailed(format!(
                    "channel '{}' is disabled",
                    msg.channel.0
                )));
            }
        }

        let config = configured.unwrap_or_default();

        if !config.allow_from.is_empty()
            && !config
                .allow_from
                .iter()
                .any(|allowed| allowed == &msg.sender.id)
        {
            return Err(ChannelError::AuthFailed(format!(
                "sender '{}' is not allowed",
                msg.sender.id
            )));
        }

        if msg.group_id.is_none() {
            match config.dm_policy {
                crate::config::DmPolicy::Open => {}
                crate::config::DmPolicy::Blocked => {
                    return Err(ChannelError::AuthFailed(
                        "direct messages are disabled".to_string(),
                    ));
                }
                crate::config::DmPolicy::Pairing => {
                    match self.security.check_pairing(&msg.sender.id).await {
                        crate::security::PairingStatus::Approved => {}
                        crate::security::PairingStatus::Pending => {
                            return Err(ChannelError::AuthFailed(
                                "pairing pending approval".to_string(),
                            ));
                        }
                        crate::security::PairingStatus::Unknown => {
                            return Err(ChannelError::AuthFailed("pairing required".to_string()));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn session_for(&self, msg: &InboundMessage) -> crate::agent::SessionId {
        let key = if let Some(group_id) = &msg.group_id {
            format!("{}:group:{}", msg.channel.0, group_id)
        } else {
            format!("{}:dm:{}", msg.channel.0, msg.sender.id)
        };

        if let Some(existing) = self.sessions.read().await.get(&key).cloned() {
            return existing;
        }

        let mut sessions = self.sessions.write().await;
        sessions
            .entry(key)
            .or_insert_with(crate::agent::SessionId::new)
            .clone()
    }

    fn builtin_command_response(
        &self,
        msg: &InboundMessage,
        content: &str,
    ) -> Option<crate::agent::AgentResponse> {
        let command = content.trim();
        if command.is_empty() {
            return None;
        }

        match command {
            "/help" => Some(crate::agent::AgentResponse::text(
                "Available commands:\n/help - Show channel command help\n/status - Show agent status",
            )),
            "/status" => Some(crate::agent::AgentResponse::text(format!(
                "BorgClaw status\nchannel={}\nprovider={}\nmodel={}\nworkspace={}\nmemory={}\npairing={}",
                msg.channel.0,
                self.agent_config.provider,
                self.agent_config.model,
                self.agent_config.workspace.display(),
                self.memory_config.database_path.display(),
                if self.security_config.pairing.enabled { "enabled" } else { "disabled" }
            ))),
            _ => None,
        }
    }
}

fn message_text(content: &MessagePayload) -> String {
    match content {
        MessagePayload::Text(text)
        | MessagePayload::Markdown(text)
        | MessagePayload::Html(text) => text.clone(),
        MessagePayload::Media { url, .. } => url.clone(),
        MessagePayload::File { path, .. } => path.clone(),
    }
}

pub struct RouteOutcome {
    pub session_id: crate::agent::SessionId,
    pub response: crate::agent::AgentResponse,
    pub outbound: OutboundMessage,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn websocket_config() -> crate::config::AppConfig {
        let mut config = crate::config::AppConfig::default();
        config.agent.provider = "unsupported".to_string();
        config.channels.insert(
            "websocket".to_string(),
            crate::config::ChannelConfig {
                enabled: true,
                allow_from: Vec::new(),
                dm_policy: crate::config::DmPolicy::Pairing,
                credentials: None,
                proxy_url: None,
                extra: HashMap::new(),
            },
        );
        config
    }

    #[tokio::test]
    async fn router_requires_pairing_then_reuses_session() {
        let router = MessageRouter::from_config(&websocket_config());
        let inbound = InboundMessage {
            channel: ChannelType::websocket(),
            sender: Sender::new("client-1").with_name("Client"),
            content: MessagePayload::text("hello"),
            group_id: None,
            timestamp: chrono::Utc::now(),
            raw: serde_json::Value::Null,
        };

        let first = router.route(inbound.clone()).await;
        assert!(
            matches!(first, Err(ChannelError::AuthFailed(message)) if message == "pairing required")
        );

        let code = router.request_pairing_code("client-1").await.unwrap();
        let approved = router.approve_pairing_code(&code).await.unwrap();
        assert_eq!(approved, "client-1");

        let one = router.route(inbound.clone()).await.unwrap();
        let two = router.route(inbound).await.unwrap();
        assert_eq!(one.session_id.0, two.session_id.0);
    }

    #[tokio::test]
    async fn router_handles_documented_help_and_status_commands() {
        let router = MessageRouter::from_config(&websocket_config());
        let help = InboundMessage {
            channel: ChannelType::telegram(),
            sender: Sender::new("user-1").with_name("User"),
            content: MessagePayload::text("/help"),
            group_id: Some("group-1".to_string()),
            timestamp: chrono::Utc::now(),
            raw: serde_json::Value::Null,
        };
        let status = InboundMessage {
            content: MessagePayload::text("/status"),
            ..help.clone()
        };

        let help_outcome = router.route(help).await.unwrap();
        assert!(help_outcome.response.text.contains("/help"));
        assert!(help_outcome.response.text.contains("/status"));

        let status_outcome = router.route(status).await.unwrap();
        assert!(status_outcome.response.text.contains("BorgClaw status"));
        assert!(status_outcome
            .response
            .text
            .contains("provider=unsupported"));
    }

    #[tokio::test]
    async fn router_rejects_explicitly_disabled_channels() {
        let mut config = crate::config::AppConfig::default();
        config.agent.provider = "unsupported".to_string();
        config.channels.insert(
            "webhook".to_string(),
            crate::config::ChannelConfig {
                enabled: false,
                allow_from: Vec::new(),
                dm_policy: crate::config::DmPolicy::Open,
                credentials: None,
                proxy_url: None,
                extra: HashMap::new(),
            },
        );

        let router = MessageRouter::from_config(&config);
        let inbound = InboundMessage {
            channel: ChannelType::new("webhook"),
            sender: Sender::new("user-1").with_name("User"),
            content: MessagePayload::text("hello"),
            group_id: Some("ops".to_string()),
            timestamp: chrono::Utc::now(),
            raw: serde_json::Value::Null,
        };

        let result = router.route(inbound).await;
        assert!(
            matches!(result, Err(ChannelError::AuthFailed(message)) if message == "channel 'webhook' is disabled")
        );
    }
}
