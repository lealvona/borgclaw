//! Channel traits

use super::{ChannelConfig, ChannelError, ChannelType, InboundMessage, OutboundMessage};
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Channel trait - implemented by all channel backends
#[async_trait]
pub trait Channel: Send + Sync {
    /// Get channel type
    fn channel_type(&self) -> ChannelType;
    
    /// Initialize channel with config
    async fn init(&mut self, config: &ChannelConfig) -> Result<(), ChannelError>;
    
    /// Start receiving messages
    async fn start_receiving(&self, sender: mpsc::Sender<InboundMessage>) -> Result<(), ChannelError>;
    
    /// Send a message
    async fn send(&self, message: OutboundMessage) -> Result<(), ChannelError>;
    
    /// Get channel status
    async fn status(&self) -> ChannelStatus;
    
    /// Shutdown channel
    async fn shutdown(&self) -> Result<(), ChannelError>;
}

/// Channel status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelStatus {
    /// Is connected
    pub connected: bool,
    /// Error message if any
    pub error: Option<String>,
    /// Last activity
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
    /// Messages received
    pub messages_received: u64,
    /// Messages sent
    pub messages_sent: u64,
}

impl ChannelStatus {
    pub fn disconnected() -> Self {
        Self {
            connected: false,
            error: None,
            last_activity: None,
            messages_received: 0,
            messages_sent: 0,
        }
    }
    
    pub fn connected() -> Self {
        Self {
            connected: true,
            error: None,
            last_activity: Some(chrono::Utc::now()),
            messages_received: 0,
            messages_sent: 0,
        }
    }
}

/// Channel sender - used to send messages through a specific channel
#[derive(Clone)]
pub struct ChannelSender {
    channel_type: ChannelType,
    sender: mpsc::Sender<OutboundMessage>,
}

impl ChannelSender {
    pub fn new(channel_type: ChannelType, sender: mpsc::Sender<OutboundMessage>) -> Self {
        Self { channel_type, sender }
    }
    
    pub fn channel_type(&self) -> &ChannelType {
        &self.channel_type
    }
    
    pub async fn send(&self, message: OutboundMessage) -> Result<(), ChannelError> {
        self.sender.send(message).await.map_err(|e| {
            ChannelError::SendFailed(e.to_string())
        })
    }
    
    pub async fn send_text(&self, target: impl Into<String>, text: impl Into<String>) -> Result<(), ChannelError> {
        let msg = OutboundMessage::new(target, self.channel_type.clone(), super::MessagePayload::text(text));
        self.send(msg).await
    }
}

/// Helper trait for creating channels
pub trait ChannelFactory: Send + Sync {
    fn create(&self) -> Box<dyn Channel>;
    fn channel_type(&self) -> ChannelType;
}
