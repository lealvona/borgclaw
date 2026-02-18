//! Channel module - handles different messaging channels

mod cli;
mod signal;
mod telegram;
mod traits;
mod webhook;

pub use cli::{CliChannel, create_cli_message};
pub use signal::{SignalChannel, SignalChannelBuilder};
pub use telegram::TelegramChannel;
pub use traits::{Channel, ChannelSender, ChannelStatus};
pub use webhook::{WebhookChannel, WebhookTrigger};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Channel type identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelType(pub String);

impl ChannelType {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    
    pub fn cli() -> Self { Self("cli".to_string()) }
    pub fn telegram() -> Self { Self("telegram".to_string()) }
    pub fn discord() -> Self { Self("discord".to_string()) }
    pub fn signal() -> Self { Self("signal".to_string()) }
    pub fn slack() -> Self { Self("slack".to_string()) }
    pub fn whatsapp() -> Self { Self("whatsapp".to_string()) }
    pub fn websocket() -> Self { Self("websocket".to_string()) }
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
        Self::Media { url: url.into(), mime_type: mime.into() }
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
            allow_from: Vec::new(),
            dm_policy: super::config::DmPolicy::Pairing,
            extra: std::collections::HashMap::new(),
        }
    }
}

/// Message router - routes messages between channels and agent
pub struct MessageRouter {
    sender: mpsc::Sender<InboundMessage>,
}

impl MessageRouter {
    pub fn new(sender: mpsc::Sender<InboundMessage>) -> Self {
        Self { sender }
    }
    
    pub async fn route(&self, msg: InboundMessage) -> Result<(), ChannelError> {
        self.sender.send(msg).await.map_err(|e| {
            ChannelError::ReceiveFailed(e.to_string())
        })
    }
}
