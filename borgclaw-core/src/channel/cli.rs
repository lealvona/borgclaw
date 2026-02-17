//! CLI Channel implementation

use super::{
    Channel, ChannelConfig, ChannelError, ChannelType, InboundMessage,
    MessagePayload, OutboundMessage, Sender,
};
use super::traits::ChannelStatus;
use async_trait::async_trait;
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// CLI Channel - for terminal-based interaction
pub struct CliChannel {
    channel_type: ChannelType,
    status: Arc<RwLock<ChannelStatus>>,
    msg_count: AtomicU64,
}

impl CliChannel {
    pub fn new() -> Self {
        Self {
            channel_type: ChannelType::cli(),
            status: Arc::new(RwLock::new(ChannelStatus::connected())),
            msg_count: AtomicU64::new(0),
        }
    }
}

impl Default for CliChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Channel for CliChannel {
    fn channel_type(&self) -> ChannelType {
        self.channel_type.clone()
    }
    
    async fn init(&mut self, _config: &ChannelConfig) -> Result<(), ChannelError> {
        *self.status.write().await = ChannelStatus::connected();
        Ok(())
    }
    
    async fn start_receiving(&self, sender: mpsc::Sender<InboundMessage>) -> Result<(), ChannelError> {
        // CLI channel doesn't auto-receive - messages come via stdin handler
        // This is a no-op for the channel itself
        let _ = sender;
        Ok(())
    }
    
    async fn send(&self, message: OutboundMessage) -> Result<(), ChannelError> {
        let text = match message.content {
            MessagePayload::Text(s) => s,
            MessagePayload::Markdown(s) => s,
            MessagePayload::Html(s) => s,
            MessagePayload::Media { url, .. } => url,
            MessagePayload::File { name, .. } => name,
        };
        
        println!("{}", text);
        self.msg_count.fetch_add(1, Ordering::Relaxed);
        
        Ok(())
    }
    
    async fn status(&self) -> ChannelStatus {
        let status = self.status.read().await;
        ChannelStatus {
            connected: status.connected,
            error: status.error.clone(),
            last_activity: status.last_activity,
            messages_received: self.msg_count.load(Ordering::Relaxed),
            messages_sent: self.msg_count.load(Ordering::Relaxed),
        }
    }
    
    async fn shutdown(&self) -> Result<(), ChannelError> {
        *self.status.write().await = ChannelStatus::disconnected();
        Ok(())
    }
}

/// Create an inbound message from CLI input
pub fn create_cli_message(content: String, sender_id: &str) -> InboundMessage {
    InboundMessage {
        channel: ChannelType::cli(),
        sender: Sender::new(sender_id),
        content: MessagePayload::Text(content),
        group_id: None,
        timestamp: Utc::now(),
        raw: serde_json::json!({ "source": "cli" }),
    }
}
