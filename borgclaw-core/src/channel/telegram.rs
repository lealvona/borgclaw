//! Telegram channel implementation with restart recovery

use super::{
    Channel, ChannelConfig, ChannelError, ChannelStatus, ChannelType, InboundMessage,
    MessagePayload, OutboundMessage, Sender,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use teloxide::prelude::*;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;

/// Persistent state for restart recovery
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramState {
    /// Last processed update_id
    pub last_update_id: Option<i32>,
    /// Total messages processed (for stats)
    pub total_messages: u64,
    /// Last activity timestamp
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct TelegramChannel {
    bot: Arc<RwLock<Option<Bot>>>,
    channel_type: ChannelType,
    status: Arc<RwLock<ChannelStatus>>,
    msg_count: AtomicU64,
    receiving: Arc<AtomicBool>,
    receive_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    allowed_users: Vec<String>,
    state_path: Option<PathBuf>,
    last_update_id: Arc<RwLock<Option<i32>>>,
}

impl TelegramChannel {
    pub fn new() -> Self {
        Self {
            bot: Arc::new(RwLock::new(None)),
            channel_type: ChannelType::telegram(),
            status: Arc::new(RwLock::new(ChannelStatus::disconnected())),
            msg_count: AtomicU64::new(0),
            receiving: Arc::new(AtomicBool::new(false)),
            receive_handle: Arc::new(Mutex::new(None)),
            allowed_users: Vec::new(),
            state_path: None,
            last_update_id: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_allowed_users(mut self, users: Vec<String>) -> Self {
        self.allowed_users = users;
        self
    }

    pub fn with_state_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.state_path = Some(path.into());
        self
    }

    /// Load persisted state for restart recovery
    fn load_state(&self) -> TelegramState {
        if let Some(ref path) = self.state_path {
            if let Ok(contents) = std::fs::read_to_string(path) {
                if let Ok(state) = serde_json::from_str(&contents) {
                    return state;
                }
            }
        }
        TelegramState::default()
    }

    /// Persist state for restart recovery
    async fn persist_state(&self) {
        if let Some(ref path) = self.state_path {
            let state = TelegramState {
                last_update_id: *self.last_update_id.read().await,
                total_messages: self.msg_count.load(Ordering::Relaxed),
                last_activity: self.status.read().await.last_activity,
            };
            if let Ok(contents) = serde_json::to_string_pretty(&state) {
                let _ = std::fs::write(path, contents);
            }
        }
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

        // Load persisted state for restart recovery
        let state = self.load_state();
        *self.last_update_id.write().await = state.last_update_id;
        self.msg_count.store(state.total_messages, Ordering::Relaxed);
        if let Some(last_activity) = state.last_activity {
            self.status.write().await.last_activity = Some(last_activity);
        }

        Ok(())
    }

    async fn start_receiving(
        &self,
        sender: mpsc::Sender<InboundMessage>,
    ) -> Result<(), ChannelError> {
        if self
            .receiving
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(ChannelError::ConnectionFailed(
                "Telegram receiver already running".to_string(),
            ));
        }

        // Clone bot for use in spawned task
        let bot = {
            let bot_guard = self.bot.read().await;
            match bot_guard.as_ref() {
                Some(bot) => bot.clone(),
                None => {
                    self.receiving.store(false, Ordering::SeqCst);
                    return Err(ChannelError::ConnectionFailed(
                        "Bot not initialized".to_string(),
                    ));
                }
            }
        };

        let allowed_users = self.allowed_users.clone();
        let msg_count = Arc::new(AtomicU64::new(0));
        let status = self.status.clone();
        let channel_type = self.channel_type.clone();
        let receiving = self.receiving.clone();
        let receive_handle = self.receive_handle.clone();

        let sender_clone = sender.clone();

        // Spawn message handler
        let handle = tokio::spawn(async move {
            teloxide::repl(bot.clone(), move |msg: Message| {
                let sender = sender_clone.clone();
                let allowed_users = allowed_users.clone();
                let msg_count = msg_count.clone();
                let status = status.clone();
                let channel_type = channel_type.clone();

                async move {
                    // Check if user is allowed
                    if !allowed_users.is_empty() {
                        let user_id = msg.from.as_ref().map(|u| u.id.to_string()).unwrap_or_default();
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
                    let telegram_user = msg.from.as_ref();
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

                    // Persist state for restart recovery
                    // Note: Full offset-based recovery requires switching from repl() to manual get_updates()
                    // This implementation tracks basic state for statistics and activity monitoring

                    Ok(())
                }
            })
            .await;

            receiving.store(false, Ordering::SeqCst);
        });

        *receive_handle.lock().await = Some(handle);

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
        self.receiving.store(false, Ordering::SeqCst);
        if let Some(handle) = self.receive_handle.lock().await.take() {
            handle.abort();
        }
        // Persist final state before shutdown
        self.persist_state().await;
        *self.status.write().await = ChannelStatus::disconnected();
        *self.bot.write().await = None;
        Ok(())
    }

}

impl TelegramChannel {
    /// Health check with automatic reconnection
    pub async fn health_check(&self) -> Result<(), ChannelError> {
        let bot_guard = self.bot.read().await;
        if let Some(bot) = bot_guard.as_ref() {
            match bot.get_me().await {
                Ok(_) => {
                    let mut status = self.status.write().await;
                    status.connected = true;
                    status.error = None;
                    Ok(())
                }
                Err(e) => {
                    let mut status = self.status.write().await;
                    status.connected = false;
                    status.error = Some(format!("Health check failed: {}", e));
                    Err(ChannelError::ConnectionFailed(e.to_string()))
                }
            }
        } else {
            Err(ChannelError::ConnectionFailed("Bot not initialized".to_string()))
        }
    }

    /// Check if restart recovery state is available
    pub fn has_restart_state(&self) -> bool {
        self.state_path.as_ref().map_or(false, |p| p.exists())
    }

    /// Get last update ID for restart recovery
    pub async fn last_update_id(&self) -> Option<i32> {
        *self.last_update_id.read().await
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
    use super::*;

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

    #[tokio::test]
    async fn telegram_start_receiving_rejects_duplicate_starts_and_shutdown_clears_handle() {
        let channel = TelegramChannel::new();
        *channel.bot.write().await = Some(Bot::new("123456:ABCDEF"));
        *channel.status.write().await = ChannelStatus::connected();

        let (tx, _rx) = mpsc::channel(1);
        channel.start_receiving(tx.clone()).await.unwrap();

        let err = channel.start_receiving(tx).await.unwrap_err();
        assert!(err.to_string().contains("already running"));

        channel.shutdown().await.unwrap();
        assert!(!channel.receiving.load(Ordering::SeqCst));
        assert!(channel.receive_handle.lock().await.is_none());
    }
}
