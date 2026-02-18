//! Webhook channel implementation for HTTP triggers and callbacks

use super::{
    Channel, ChannelConfig, ChannelError, ChannelStatus, ChannelType, InboundMessage,
    MessagePayload, OutboundMessage, Sender,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookTrigger {
    pub id: String,
    pub name: String,
    pub path: String,
    pub method: String,
    pub secret: Option<String>,
    pub enabled: bool,
    pub headers: HashMap<String, String>,
    pub rate_limit: Option<u32>,
}

impl WebhookTrigger {
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            path: path.into(),
            method: "POST".to_string(),
            secret: None,
            enabled: true,
            headers: HashMap::new(),
            rate_limit: None,
        }
    }

    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = method.into();
        self
    }

    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        self.secret = Some(secret.into());
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn with_rate_limit(mut self, requests_per_minute: u32) -> Self {
        self.rate_limit = Some(requests_per_minute);
        self
    }

    pub fn matches(&self, path: &str, method: &str) -> bool {
        self.enabled && self.path == path && self.method.to_uppercase() == method.to_uppercase()
    }

    pub fn verify_secret(&self, provided: Option<&str>) -> bool {
        match &self.secret {
            Some(expected) => provided == Some(expected.as_str()),
            None => true,
        }
    }
}

pub struct WebhookChannel {
    channel_type: ChannelType,
    status: Arc<RwLock<ChannelStatus>>,
    msg_count: AtomicU64,
    triggers: Arc<RwLock<Vec<WebhookTrigger>>>,
    rate_limits: Arc<RwLock<HashMap<String, Vec<chrono::DateTime<chrono::Utc>>>>>,
}

impl WebhookChannel {
    pub fn new() -> Self {
        Self {
            channel_type: ChannelType::new("webhook"),
            status: Arc::new(RwLock::new(ChannelStatus::disconnected())),
            msg_count: AtomicU64::new(0),
            triggers: Arc::new(RwLock::new(Vec::new())),
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_trigger(&self, trigger: WebhookTrigger) {
        let mut triggers = self.triggers.write().await;
        triggers.push(trigger);
    }

    pub async fn remove_trigger(&self, id: &str) -> bool {
        let mut triggers = self.triggers.write().await;
        let len_before = triggers.len();
        triggers.retain(|t| t.id != id);
        triggers.len() < len_before
    }

    pub async fn get_trigger(&self, id: &str) -> Option<WebhookTrigger> {
        let triggers = self.triggers.read().await;
        triggers.iter().find(|t| t.id == id).cloned()
    }

    pub async fn list_triggers(&self) -> Vec<WebhookTrigger> {
        self.triggers.read().await.clone()
    }

    async fn check_rate_limit(&self, trigger: &WebhookTrigger) -> bool {
        if let Some(rpm) = trigger.rate_limit {
            let mut limits = self.rate_limits.write().await;
            let now = Utc::now();
            let minute_ago = now - chrono::Duration::seconds(60);
            
            let requests = limits.entry(trigger.id.clone()).or_default();
            requests.retain(|&t| t > minute_ago);
            
            if requests.len() >= rpm as usize {
                return false;
            }
            
            requests.push(now);
        }
        true
    }

    pub async fn handle_request(
        &self,
        path: &str,
        method: &str,
        headers: HashMap<String, String>,
        body: Vec<u8>,
        sender: mpsc::Sender<InboundMessage>,
    ) -> Result<WebhookResponse, WebhookError> {
        let triggers = self.triggers.read().await;
        
        let trigger = triggers
            .iter()
            .find(|t| t.matches(path, method))
            .ok_or(WebhookError::NotFound)?;

        let secret = headers.get("X-Webhook-Secret").map(|s| s.as_str());
        if !trigger.verify_secret(secret) {
            return Err(WebhookError::Unauthorized);
        }

        if !self.check_rate_limit(trigger).await {
            return Err(WebhookError::RateLimited);
        }

        let body_str = String::from_utf8_lossy(&body);
        let content = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_str) {
            MessagePayload::Text(body_str.to_string())
        } else {
            MessagePayload::Text(body_str.to_string())
        };

        let inbound = InboundMessage {
            channel: self.channel_type.clone(),
            sender: Sender::new(&trigger.id).with_name(&trigger.name),
            content,
            group_id: None,
            timestamp: Utc::now(),
            raw: serde_json::json!({
                "trigger_id": trigger.id,
                "trigger_name": trigger.name,
                "path": path,
                "method": method,
                "headers": headers,
            }),
        };

        self.msg_count.fetch_add(1, Ordering::Relaxed);
        {
            let mut status = self.status.write().await;
            status.last_activity = Some(Utc::now());
        }

        sender.send(inbound).await.map_err(|_| WebhookError::ChannelClosed)?;

        Ok(WebhookResponse {
            status: 200,
            body: serde_json::json!({
                "status": "accepted",
                "trigger_id": trigger.id,
            }),
        })
    }
}

impl Default for WebhookChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Channel for WebhookChannel {
    fn channel_type(&self) -> ChannelType {
        self.channel_type.clone()
    }

    async fn init(&mut self, _config: &ChannelConfig) -> Result<(), ChannelError> {
        *self.status.write().await = ChannelStatus::connected();
        Ok(())
    }

    async fn start_receiving(&self, _sender: mpsc::Sender<InboundMessage>) -> Result<(), ChannelError> {
        Ok(())
    }

    async fn send(&self, _message: OutboundMessage) -> Result<(), ChannelError> {
        Err(ChannelError::NotSupported(
            "Webhook channel does not support sending".to_string(),
        ))
    }

    async fn status(&self) -> ChannelStatus {
        let status = self.status.read().await;
        ChannelStatus {
            connected: status.connected,
            error: status.error.clone(),
            last_activity: status.last_activity,
            messages_received: self.msg_count.load(Ordering::Relaxed),
            messages_sent: 0,
        }
    }

    async fn shutdown(&self) -> Result<(), ChannelError> {
        *self.status.write().await = ChannelStatus::disconnected();
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("Webhook not found")]
    NotFound,
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Rate limited")]
    RateLimited,
    #[error("Channel closed")]
    ChannelClosed,
}

pub struct WebhookChannelBuilder {
    triggers: Vec<WebhookTrigger>,
}

impl WebhookChannelBuilder {
    pub fn new() -> Self {
        Self {
            triggers: Vec::new(),
        }
    }

    pub fn trigger(mut self, trigger: WebhookTrigger) -> Self {
        self.triggers.push(trigger);
        self
    }

    pub async fn build(self) -> WebhookChannel {
        let channel = WebhookChannel::new();
        for trigger in self.triggers {
            channel.register_trigger(trigger).await;
        }
        channel
    }
}

impl Default for WebhookChannelBuilder {
    fn default() -> Self {
        Self::new()
    }
}
