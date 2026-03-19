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
    pub forward_url: Option<String>,
    pub body_template: Option<String>,
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
            forward_url: None,
            body_template: None,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
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

    pub fn with_forward_url(mut self, url: impl Into<String>) -> Self {
        self.forward_url = Some(url.into());
        self
    }

    pub fn with_body_template(mut self, template: impl Into<String>) -> Self {
        self.body_template = Some(template.into());
        self
    }

    pub fn matches(&self, path: &str, method: &str) -> bool {
        self.enabled
            && trigger_path_matches(&self.path, path)
            && self.method.to_uppercase() == method.to_uppercase()
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

    async fn check_rate_limit(&self, trigger: &WebhookTrigger, requester: &str) -> Result<(), u64> {
        if let Some(rpm) = trigger.rate_limit {
            let mut limits = self.rate_limits.write().await;
            let now = Utc::now();
            let minute_ago = now - chrono::Duration::seconds(60);

            let requests = limits
                .entry(format!("{}:{}", trigger.id, requester))
                .or_default();
            requests.retain(|&t| t > minute_ago);

            if requests.len() >= rpm as usize {
                let retry_after = requests
                    .iter()
                    .min()
                    .map(|earliest| {
                        let retry_at = *earliest + chrono::Duration::seconds(60);
                        let remaining = (retry_at - now).num_seconds().max(1);
                        remaining as u64
                    })
                    .unwrap_or(60);
                return Err(retry_after);
            }

            requests.push(now);
        }
        Ok(())
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
            .filter(|t| t.matches(path, method))
            .max_by_key(|trigger| trigger_match_priority(trigger))
            .ok_or(WebhookError::NotFound)?;

        let secret = header_value(&headers, "x-webhook-secret");
        if !trigger.verify_secret(secret) {
            return Err(WebhookError::Unauthorized);
        }

        let requester = webhook_requester(&headers);
        if let Err(retry_after_seconds) = self.check_rate_limit(trigger, &requester).await {
            return Err(WebhookError::RateLimited(retry_after_seconds));
        }

        let body_str = String::from_utf8_lossy(&body);
        let parsed_json = serde_json::from_str::<serde_json::Value>(&body_str).ok();
        let content = webhook_content(parsed_json.as_ref(), &body_str);
        let sender_info = webhook_sender(parsed_json.as_ref(), trigger);
        let group_id = parsed_json
            .as_ref()
            .and_then(|json| json.get("group_id"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
            .or_else(|| header_value(&headers, "x-group-id").map(ToString::to_string))
            .or_else(|| trigger_path_value(&trigger.path, path));

        let inbound = InboundMessage {
            channel: self.channel_type.clone(),
            sender: sender_info,
            content,
            group_id,
            timestamp: Utc::now(),
            raw: serde_json::json!({
                "trigger_id": trigger.id,
                "trigger_name": trigger.name,
                "path": path,
                "method": method,
                "headers": headers,
                "body": parsed_json.unwrap_or_else(|| serde_json::Value::String(body_str.to_string())),
            }),
        };

        self.msg_count.fetch_add(1, Ordering::Relaxed);
        {
            let mut status = self.status.write().await;
            status.last_activity = Some(Utc::now());
        }

        sender
            .send(inbound)
            .await
            .map_err(|_| WebhookError::ChannelClosed)?;

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

    async fn start_receiving(
        &self,
        _sender: mpsc::Sender<InboundMessage>,
    ) -> Result<(), ChannelError> {
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
    RateLimited(u64),
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

fn webhook_requester(headers: &HashMap<String, String>) -> String {
    header_value(headers, "x-forwarded-for")
        .or_else(|| header_value(headers, "x-real-ip"))
        .map(ToString::to_string)
        .unwrap_or_else(|| "anonymous".to_string())
}

fn webhook_content(payload: Option<&serde_json::Value>, body: &str) -> MessagePayload {
    let text = payload
        .and_then(|json| json.get("content"))
        .and_then(|value| value.as_str())
        .unwrap_or(body);
    MessagePayload::Text(text.to_string())
}

fn webhook_sender(payload: Option<&serde_json::Value>, trigger: &WebhookTrigger) -> Sender {
    let sender_id = payload
        .and_then(|json| json.get("sender"))
        .and_then(|value| value.as_str())
        .unwrap_or(&trigger.id);
    let sender_name = payload
        .and_then(|json| json.get("sender_name"))
        .and_then(|value| value.as_str())
        .unwrap_or(&trigger.name);
    Sender::new(sender_id).with_name(sender_name)
}

fn trigger_path_matches(pattern: &str, actual: &str) -> bool {
    if pattern == actual {
        return true;
    }

    if let Some(prefix) = pattern.strip_suffix("{id}") {
        return actual.starts_with(prefix) && actual.len() > prefix.len();
    }

    false
}

fn trigger_path_value(pattern: &str, actual: &str) -> Option<String> {
    let prefix = pattern.strip_suffix("{id}")?;
    actual
        .strip_prefix(prefix)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn trigger_match_priority(trigger: &WebhookTrigger) -> (usize, usize) {
    let wildcard_count = trigger.path.matches('{').count();
    (
        usize::MAX.saturating_sub(wildcard_count),
        trigger.path.len(),
    )
}

fn header_value<'a>(headers: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn webhook_uses_payload_content_sender_and_group() {
        let channel = WebhookChannel::new();
        channel
            .register_trigger(WebhookTrigger::new("incoming", "/webhook"))
            .await;
        let (tx, mut rx) = mpsc::channel(1);

        let response = channel
            .handle_request(
                "/webhook",
                "POST",
                HashMap::from([("x-forwarded-for".to_string(), "127.0.0.1".to_string())]),
                br#"{"content":"hello","sender":"user123","sender_name":"User","group_id":"grp"}"#
                    .to_vec(),
                tx,
            )
            .await
            .unwrap();

        let inbound = rx.recv().await.unwrap();
        assert_eq!(response.status, 200);
        assert!(matches!(inbound.content, MessagePayload::Text(ref text) if text == "hello"));
        assert_eq!(inbound.sender.id, "user123");
        assert_eq!(inbound.sender.name.as_deref(), Some("User"));
        assert_eq!(inbound.group_id.as_deref(), Some("grp"));
    }

    #[tokio::test]
    async fn webhook_rate_limit_isolated_per_requester() {
        let channel = WebhookChannel::new();
        channel
            .register_trigger(WebhookTrigger::new("incoming", "/webhook").with_rate_limit(1))
            .await;
        let (tx, _rx) = mpsc::channel(4);

        let headers_a = HashMap::from([("X-Forwarded-For".to_string(), "10.0.0.1".to_string())]);
        let headers_b = HashMap::from([("X-Forwarded-For".to_string(), "10.0.0.2".to_string())]);

        channel
            .handle_request(
                "/webhook",
                "POST",
                headers_a.clone(),
                b"one".to_vec(),
                tx.clone(),
            )
            .await
            .unwrap();
        let limited = channel
            .handle_request("/webhook", "POST", headers_a, b"two".to_vec(), tx.clone())
            .await;
        let other_requester = channel
            .handle_request("/webhook", "POST", headers_b, b"three".to_vec(), tx)
            .await;

        assert!(matches!(limited, Err(WebhookError::RateLimited(retry_after)) if retry_after >= 1));
        assert!(other_requester.is_ok());
    }

    #[tokio::test]
    async fn webhook_uses_group_id_from_header_or_named_path() {
        let channel = WebhookChannel::new();
        channel
            .register_trigger(WebhookTrigger::new("incoming", "/webhook"))
            .await;
        channel
            .register_trigger(WebhookTrigger::new("named", "/webhook/trigger/{id}"))
            .await;

        let (tx_header, mut rx_header) = mpsc::channel(1);
        channel
            .handle_request(
                "/webhook",
                "POST",
                HashMap::from([("x-group-id".to_string(), "header-group".to_string())]),
                br#"{"content":"hello"}"#.to_vec(),
                tx_header,
            )
            .await
            .unwrap();
        assert_eq!(
            rx_header.recv().await.unwrap().group_id.as_deref(),
            Some("header-group")
        );

        let (tx_path, mut rx_path) = mpsc::channel(1);
        channel
            .handle_request(
                "/webhook/trigger/team-ops",
                "POST",
                HashMap::new(),
                br#"{"content":"hello"}"#.to_vec(),
                tx_path,
            )
            .await
            .unwrap();
        assert_eq!(
            rx_path.recv().await.unwrap().group_id.as_deref(),
            Some("team-ops")
        );
    }

    #[test]
    fn webhook_trigger_path_matches_named_pattern() {
        assert!(trigger_path_matches(
            "/webhook/trigger/{id}",
            "/webhook/trigger/backup"
        ));
        assert!(!trigger_path_matches(
            "/webhook/trigger/{id}",
            "/webhook/trigger"
        ));
    }

    #[tokio::test]
    async fn exact_webhook_trigger_path_wins_over_named_wildcard() {
        let channel = WebhookChannel::new();
        channel
            .register_trigger(WebhookTrigger::new("named", "/webhook/trigger/{id}"))
            .await;
        channel
            .register_trigger(
                WebhookTrigger::new("notify_slack", "/webhook/trigger/notify_slack")
                    .with_id("notify_slack"),
            )
            .await;

        let (tx, mut rx) = mpsc::channel(1);
        let response = channel
            .handle_request(
                "/webhook/trigger/notify_slack",
                "POST",
                HashMap::new(),
                br#"{"content":"hello"}"#.to_vec(),
                tx,
            )
            .await
            .unwrap();

        let inbound = rx.recv().await.unwrap();
        assert_eq!(response.body["trigger_id"], "notify_slack");
        assert_eq!(inbound.sender.id, "notify_slack");
    }
}
