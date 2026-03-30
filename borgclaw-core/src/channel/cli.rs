//! CLI Channel implementation

use super::traits::ChannelStatus;
use super::{
    Channel, ChannelConfig, ChannelError, ChannelType, InboundMessage, MessagePayload,
    OutboundMessage, Sender,
};
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

    async fn start_receiving(
        &self,
        sender: mpsc::Sender<InboundMessage>,
    ) -> Result<(), ChannelError> {
        // CLI channel doesn't auto-receive - messages come via stdin handler
        // This is a no-op for the channel itself
        let _ = sender;
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), ChannelError> {
        let text = match message.content {
            MessagePayload::Text(s) => s,
            MessagePayload::Markdown(s) => {
                // For CLI, render markdown simply by stripping formatting characters
                // that would clutter terminal output
                render_markdown_simple(&s)
            }
            MessagePayload::Html(s) => strip_html_tags(&s),
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

/// Simple markdown rendering for terminal output
/// Strips markdown formatting characters that would clutter CLI output
fn render_markdown_simple(md: &str) -> String {
    let mut result = md.to_string();
    
    // Headers: replace with bold/underline style
    for i in (1..=6).rev() {
        let hashes = "#".repeat(i);
        result = result.replace(&format!("{} ", hashes), &format!("\n{} ", hashes));
    }
    
    // Bold: keep the text, remove **
    result = result.replace("**", "");
    
    // Italic: keep the text, remove * (but not bullet points)
    // This is a simplification - proper parsing would track context
    
    // Code blocks: keep content, fence markers become indentation indicators
    result = result.replace("```\n", "\n");
    result = result.replace("```", "");
    result = result.replace("`", "");
    
    // Links: extract just the text part [text](url) -> text
    // Process links iteratively
    loop {
        if let Some(start) = result.find('[') {
            if let Some(end_bracket) = result[start..].find("](") {
                let bracket_end = start + end_bracket;
                if let Some(end_paren) = result[bracket_end..].find(')') {
                    let link_text = result[start + 1..bracket_end].to_string();
                    let full_len = end_bracket + end_paren + 1;
                    result.replace_range(start..start + full_len, &link_text);
                    continue; // Continue to process more links
                }
            }
        }
        break;
    }
    
    result
}

/// Strip HTML tags from text
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    
    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_channel_new() {
        let channel = CliChannel::new();
        assert_eq!(channel.channel_type, ChannelType::cli());
        // Channel starts with 0 messages
        assert_eq!(channel.msg_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn cli_channel_default() {
        let channel: CliChannel = Default::default();
        assert_eq!(channel.channel_type, ChannelType::cli());
    }

    #[tokio::test]
    async fn cli_channel_init_sets_connected() {
        let mut channel = CliChannel::new();
        let config = ChannelConfig::default();

        let result = channel.init(&config).await;
        assert!(result.is_ok());

        let status = channel.status().await;
        assert!(status.connected);
    }

    #[tokio::test]
    async fn cli_channel_start_receiving_returns_ok() {
        let channel = CliChannel::new();
        let (tx, _rx) = mpsc::channel(10);

        let result = channel.start_receiving(tx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cli_channel_status_tracks_messages() {
        let channel = CliChannel::new();

        // Initially 0 messages
        let status = channel.status().await;
        assert_eq!(status.messages_sent, 0);
        assert_eq!(status.messages_received, 0);

        // Simulate sending a message by incrementing counter
        channel.msg_count.fetch_add(1, Ordering::Relaxed);

        let status = channel.status().await;
        assert_eq!(status.messages_sent, 1);
        assert_eq!(status.messages_received, 1);
    }

    #[tokio::test]
    async fn cli_channel_shutdown_sets_disconnected() {
        let channel = CliChannel::new();

        // Start connected
        let status = channel.status().await;
        assert!(status.connected);

        // Shutdown
        let result = channel.shutdown().await;
        assert!(result.is_ok());

        let status = channel.status().await;
        assert!(!status.connected);
    }

    #[test]
    fn create_cli_message_creates_inbound_message() {
        let msg = create_cli_message("Hello, world!".to_string(), "user-123");

        assert_eq!(msg.channel, ChannelType::cli());
        assert_eq!(msg.sender.id, "user-123");
        assert!(msg.group_id.is_none());

        match msg.content {
            MessagePayload::Text(text) => assert_eq!(text, "Hello, world!"),
            _ => panic!("Expected Text payload"),
        }

        // Raw should contain source info
        assert!(msg.raw.get("source").is_some());
        assert_eq!(msg.raw["source"], "cli");
    }

    #[test]
    fn create_cli_message_with_empty_content() {
        let msg = create_cli_message("".to_string(), "anonymous");

        match msg.content {
            MessagePayload::Text(text) => assert!(text.is_empty()),
            _ => panic!("Expected Text payload"),
        }
        assert_eq!(msg.sender.id, "anonymous");
    }

    #[test]
    fn create_cli_message_timestamp_is_recent() {
        let before = Utc::now();
        let msg = create_cli_message("test".to_string(), "test-user");
        let after = Utc::now();

        assert!(msg.timestamp >= before);
        assert!(msg.timestamp <= after);
    }
}
