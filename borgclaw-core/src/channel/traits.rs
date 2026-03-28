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
    async fn start_receiving(
        &self,
        sender: mpsc::Sender<InboundMessage>,
    ) -> Result<(), ChannelError>;

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
        Self {
            channel_type,
            sender,
        }
    }

    pub fn channel_type(&self) -> &ChannelType {
        &self.channel_type
    }

    pub async fn send(&self, message: OutboundMessage) -> Result<(), ChannelError> {
        self.sender
            .send(message)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))
    }

    pub async fn send_text(
        &self,
        target: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<(), ChannelError> {
        let msg = OutboundMessage::new(
            target,
            self.channel_type.clone(),
            crate::channel::MessagePayload::Text(text.into()),
        );
        self.send(msg).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_status_disconnected_creates_correct_state() {
        let status = ChannelStatus::disconnected();
        assert!(!status.connected);
        assert!(status.error.is_none());
        assert!(status.last_activity.is_none());
        assert_eq!(status.messages_received, 0);
        assert_eq!(status.messages_sent, 0);
    }

    #[test]
    fn channel_status_connected_creates_correct_state() {
        let status = ChannelStatus::connected();
        assert!(status.connected);
        assert!(status.error.is_none());
        assert!(status.last_activity.is_some());
        assert_eq!(status.messages_received, 0);
        assert_eq!(status.messages_sent, 0);
    }

    #[test]
    fn channel_status_connected_sets_recent_timestamp() {
        let before = chrono::Utc::now();
        let status = ChannelStatus::connected();
        let after = chrono::Utc::now();
        
        let last_activity = status.last_activity.unwrap();
        assert!(last_activity >= before);
        assert!(last_activity <= after);
    }

    #[test]
    fn channel_sender_new_stores_channel_type() {
        let (tx, _rx) = mpsc::channel(10);
        let sender = ChannelSender::new(ChannelType::telegram(), tx);
        
        assert_eq!(sender.channel_type(), &ChannelType::telegram());
    }

    #[tokio::test]
    async fn channel_sender_send_returns_error_when_channel_closed() {
        let (tx, rx) = mpsc::channel(10);
        let sender = ChannelSender::new(ChannelType::cli(), tx);
        
        // Drop the receiver to close the channel
        drop(rx);
        
        let msg = OutboundMessage::new(
            "test",
            ChannelType::cli(),
            crate::channel::MessagePayload::Text("hello".to_string()),
        );
        
        let result = sender.send(msg).await;
        assert!(result.is_err());
        match result {
            Err(ChannelError::SendFailed(_)) => (), // Expected
            _ => panic!("Expected SendFailed error"),
        }
    }

    #[tokio::test]
    async fn channel_sender_send_text_creates_text_payload() {
        let (tx, mut rx) = mpsc::channel(10);
        let sender = ChannelSender::new(ChannelType::new("webhook"), tx);
        
        let send_task = async {
            sender.send_text("target-123", "Hello World").await
        };
        
        let recv_task = async {
            rx.recv().await
        };
        
        let (send_result, recv_result) = tokio::join!(send_task, recv_task);
        
        assert!(send_result.is_ok());
        let msg = recv_result.unwrap();
        assert_eq!(msg.target, "target-123");
        // Channel type is in the message
    }
}
