//! Telegram channel implementation

use super::{
    Channel, ChannelConfig, ChannelError, ChannelStatus, ChannelType, InboundMessage,
    MessagePayload, OutboundMessage, Sender,
};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use teloxide::prelude::*;
use tokio::sync::{mpsc, RwLock};

pub struct TelegramChannel {
    bot: Arc<RwLock<Option<Bot>>>,
    channel_type: ChannelType,
    status: Arc<RwLock<ChannelStatus>>,
    msg_count: AtomicU64,
    allowed_users: Vec<String>,
}

impl TelegramChannel {
    pub fn new() -> Self {
        Self {
            bot: Arc::new(RwLock::new(None)),
            channel_type: ChannelType::telegram(),
            status: Arc::new(RwLock::new(ChannelStatus::disconnected())),
            msg_count: AtomicU64::new(0),
            allowed_users: Vec::new(),
        }
    }

    pub fn with_allowed_users(mut self, users: Vec<String>) -> Self {
        self.allowed_users = users;
        self
    }
}

impl Default for TelegramChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn channel_type(&self) -> ChannelType {
        self.channel_type.clone()
    }

    async fn init(&mut self, config: &ChannelConfig) -> Result<(), ChannelError> {
        let token = config
            .credentials
            .as_deref()
            .and_then(resolve_telegram_token)
            .ok_or_else(|| {
            ChannelError::AuthFailed("Telegram bot token not provided".to_string())
        })?;

        let bot = Bot::new(token);

        // Test the bot
        bot.get_me()
            .await
            .map_err(|e| ChannelError::AuthFailed(format!("Invalid bot token: {}", e)))?;

        *self.bot.write().await = Some(bot);
        *self.status.write().await = ChannelStatus::connected();

        // Set allowed users from config
        self.allowed_users = config.allow_from.clone();

        Ok(())
    }

    async fn start_receiving(
        &self,
        sender: mpsc::Sender<InboundMessage>,
    ) -> Result<(), ChannelError> {
        // Clone bot for use in spawned task
        let bot = {
            let bot_guard = self.bot.read().await;
            bot_guard
                .as_ref()
                .ok_or_else(|| ChannelError::ConnectionFailed("Bot not initialized".to_string()))?
                .clone()
        };

        let allowed_users = self.allowed_users.clone();
        let msg_count = Arc::new(AtomicU64::new(0));
        let status = self.status.clone();
        let channel_type = self.channel_type.clone();

        let sender_clone = sender.clone();

        // Spawn message handler
        tokio::spawn(async move {
            teloxide::repl(bot.clone(), move |msg: Message| {
                let sender = sender_clone.clone();
                let allowed_users = allowed_users.clone();
                let msg_count = msg_count.clone();
                let status = status.clone();
                let channel_type = channel_type.clone();

                async move {
                    // Check if user is allowed
                    if !allowed_users.is_empty() {
                        let user_id = msg.from().map(|u| u.id.to_string()).unwrap_or_default();
                        if !allowed_users.contains(&user_id) {
                            log::warn!("Message from blocked user: {}", user_id);
                            return Ok(());
                        }
                    }

                    // Extract message content - handle text only for now
                    let content = if let Some(text) = msg.text() {
                        MessagePayload::text(text)
                    } else {
                        MessagePayload::text("[Non-text message]")
                    };

                    // Create sender info
                    let telegram_user = msg.from();
                    let sender_info =
                        Sender::new(telegram_user.map(|u| u.id.to_string()).unwrap_or_default())
                            .with_name(
                                telegram_user
                                    .map(|u| u.full_name().to_string())
                                    .unwrap_or_default(),
                            );

                    // Get chat ID (for group support)
                    let chat_id = msg.chat.id.to_string();

                    let inbound = InboundMessage {
                        channel: channel_type,
                        sender: sender_info,
                        content,
                        group_id: Some(chat_id),
                        timestamp: msg.date,
                        raw: serde_json::json!({
                            "message_id": msg.id,
                            "chat_id": msg.chat.id,
                        }),
                    };

                    // Update counters
                    msg_count.fetch_add(1, Ordering::Relaxed);
                    {
                        let mut s = status.write().await;
                        s.last_activity = Some(Utc::now());
                    }

                    // Send to router
                    let _ = sender.send(inbound).await;

                    Ok(())
                }
            })
            .await;
        });

        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), ChannelError> {
        let bot_guard = self.bot.read().await;
        let bot = bot_guard
            .as_ref()
            .ok_or_else(|| ChannelError::ConnectionFailed("Bot not initialized".to_string()))?;

        let chat_id = ChatId(
            message
                .target
                .parse()
                .map_err(|_| ChannelError::SendFailed("Invalid chat ID".to_string()))?,
        );

        match message.content {
            MessagePayload::Text(text) => {
                bot.send_message(chat_id, text)
                    .await
                    .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
            }
            MessagePayload::Markdown(text) | MessagePayload::Html(text) => {
                bot.send_message(chat_id, text)
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await
                    .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
            }
            MessagePayload::Media { url, .. } => {
                bot.send_message(chat_id, url)
                    .await
                    .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
            }
            MessagePayload::File { name, .. } => {
                bot.send_message(chat_id, format!("[File: {}]", name))
                    .await
                    .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
            }
        }

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
        *self.bot.write().await = None;
        Ok(())
    }
}

fn resolve_telegram_token(configured: &str) -> Option<String> {
    if let Some(env_key) = configured
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
    {
        return std::env::var(env_key)
            .ok()
            .filter(|value| !value.trim().is_empty());
    }

    Some(configured.to_string())
}

#[cfg(test)]
mod tests {
    use super::resolve_telegram_token;

    #[test]
    fn telegram_token_resolves_documented_env_placeholder() {
        let key = "BORGCLAW_TEST_TELEGRAM_TOKEN";
        unsafe {
            std::env::set_var(key, "123456:ABC");
        }

        assert_eq!(
            resolve_telegram_token("${BORGCLAW_TEST_TELEGRAM_TOKEN}").as_deref(),
            Some("123456:ABC")
        );

        unsafe {
            std::env::remove_var(key);
        }
    }
}
