//! Webhook channel implementation for HTTP triggers and callbacks with restart recovery

use super::{
    Channel, ChannelConfig, ChannelError, ChannelStatus, ChannelType, InboundMessage,
    MessagePayload, OutboundMessage, Sender,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;

/// Persistent state for restart recovery
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebhookState {
    /// Total webhooks processed
    pub total_webhooks: u64,
    /// Last activity timestamp
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
    /// Trigger statistics
    pub trigger_stats: HashMap<String, u64>,
}

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
    msg_count: Arc<AtomicU64>,
    triggers: Arc<RwLock<Vec<WebhookTrigger>>>,
    rate_limits: Arc<RwLock<HashMap<String, Vec<chrono::DateTime<chrono::Utc>>>>>,
    state_path: Option<PathBuf>,
    running: Arc<AtomicBool>,
    loop_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    trigger_stats: Arc<RwLock<HashMap<String, u64>>>,
}

impl WebhookChannel {
    pub fn new() -> Self {
        Self {
            channel_type: ChannelType::new("webhook"),
            status: Arc::new(RwLock::new(ChannelStatus::disconnected())),
            msg_count: Arc::new(AtomicU64::new(0)),
            triggers: Arc::new(RwLock::new(Vec::new())),
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
            state_path: None,
            running: Arc::new(AtomicBool::new(false)),
            loop_handle: Arc::new(Mutex::new(None)),
            trigger_stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_state_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.state_path = Some(path.into());
        self
    }

    /// Load persisted state for restart recovery
    fn load_state(&self) -> WebhookState {
        if let Some(ref path) = self.state_path {
            if let Ok(contents) = std::fs::read_to_string(path) {
                if let Ok(state) = serde_json::from_str(&contents) {
                    return state;
                }
            }
        }
        WebhookState::default()
    }

    /// Persist state for restart recovery
    async fn persist_state(&self) {
        if let Some(ref path) = self.state_path {
            let state = WebhookState {
                total_webhooks: self.msg_count.load(Ordering::Relaxed),
                last_activity: self.status.read().await.last_activity,
                trigger_stats: self.trigger_stats.read().await.clone(),
            };
            if let Ok(contents) = serde_json::to_string_pretty(&state) {
                let _ = std::fs::write(path, contents);
            }
        }
    }

    /// Check if restart recovery state is available
    pub fn has_restart_state(&self) -> bool {
        self.state_path.as_ref().is_some_and(|p| p.exists())
    }

    /// Get total webhook count for restart recovery
    pub fn total_webhooks(&self) -> u64 {
        self.msg_count.load(Ordering::Relaxed)
    }

    pub async fn register_trigger(&self, trigger: WebhookTrigger) {
        let mut triggers = self.triggers.write().await;
        triggers.push(trigger);
    }

    /// Start background state persistence loop
    async fn start_state_persistence(&self) {
        let state_path = self.state_path.clone();
        let status = self.status.clone();
        let msg_count = self.msg_count.clone();
        let trigger_stats = self.trigger_stats.clone();
        let running = self.running.clone();

        let handle = tokio::spawn(async move {
            while running.load(Ordering::Relaxed) {
                if let Some(ref path) = state_path {
                    let state = WebhookState {
                        total_webhooks: msg_count.load(Ordering::Relaxed),
                        last_activity: status.read().await.last_activity,
                        trigger_stats: trigger_stats.read().await.clone(),
                    };
                    if let Ok(contents) = serde_json::to_string_pretty(&state) {
                        let _ = std::fs::write(path, contents);
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            }
        });

        *self.loop_handle.lock().await = Some(handle);
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
        {
            let mut stats = self.trigger_stats.write().await;
            *stats.entry(trigger.id.clone()).or_insert(0) += 1;
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
        // Load persisted state for restart recovery
        let state = self.load_state();
        self.msg_count
            .store(state.total_webhooks, Ordering::Relaxed);
        *self.trigger_stats.write().await = state.trigger_stats;
        if let Some(last_activity) = state.last_activity {
            self.status.write().await.last_activity = Some(last_activity);
        }

        *self.status.write().await = ChannelStatus::connected();
        Ok(())
    }

    async fn start_receiving(
        &self,
        _sender: mpsc::Sender<InboundMessage>,
    ) -> Result<(), ChannelError> {
        if !self.status.read().await.connected {
            return Err(ChannelError::ConnectionFailed(
                "Channel not initialized".to_string(),
            ));
        }

        // Duplicate-start rejection using AtomicBool compare_exchange
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(ChannelError::ConnectionFailed(
                "Webhook state persistence already running".to_string(),
            ));
        }

        // Start background state persistence loop
        self.start_state_persistence().await;

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
        // Stop the background loop
        self.running.store(false, Ordering::SeqCst);

        // Abort the loop handle if present
        if let Some(handle) = self.loop_handle.lock().await.take() {
            handle.abort();
        }

        // Persist final state
        self.persist_state().await;

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

    #[tokio::test]
    async fn webhook_start_receiving_rejects_duplicate_starts() {
        let mut channel = WebhookChannel::new();
        let (tx, _rx) = mpsc::channel(1);

        // Initialize channel first
        channel
            .init(&ChannelConfig {
                channel_type: ChannelType::new("webhook"),
                enabled: true,
                credentials: None,
                allow_from: vec![],
                dm_policy: crate::config::DmPolicy::Open,
                extra: HashMap::new(),
            })
            .await
            .unwrap();

        // First start should succeed
        let result1 = channel.start_receiving(tx.clone()).await;
        assert!(result1.is_ok());

        // Second start should fail with duplicate error
        let result2 = channel.start_receiving(tx).await;
        assert!(result2.is_err());
        assert!(result2.unwrap_err().to_string().contains("already running"));

        // Clean up
        let _ = channel.shutdown().await;
    }

    #[tokio::test]
    async fn webhook_shutdown_stops_state_persistence() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_webhook_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let state_path = root.join("webhook_state.json");

        let mut channel = WebhookChannel::new().with_state_path(&state_path);
        let (tx, _rx) = mpsc::channel(1);

        // Initialize channel first
        channel
            .init(&ChannelConfig {
                channel_type: ChannelType::new("webhook"),
                enabled: true,
                credentials: None,
                allow_from: vec![],
                dm_policy: crate::config::DmPolicy::Open,
                extra: HashMap::new(),
            })
            .await
            .unwrap();

        // Start and process some webhooks
        channel.start_receiving(tx.clone()).await.unwrap();

        channel
            .register_trigger(WebhookTrigger::new("test", "/webhook").with_id("test-trigger"))
            .await;

        // Process a webhook
        let (req_tx, mut req_rx) = mpsc::channel(1);
        channel
            .handle_request(
                "/webhook",
                "POST",
                HashMap::from([("x-forwarded-for".to_string(), "127.0.0.1".to_string())]),
                br#"{"content":"test"}"#.to_vec(),
                req_tx,
            )
            .await
            .unwrap();

        let _ = req_rx.recv().await;

        // Force immediate state persistence
        channel.persist_state().await;

        // Verify state was persisted
        assert!(state_path.exists());
        let contents = std::fs::read_to_string(&state_path).unwrap();
        let state: WebhookState = serde_json::from_str(&contents).unwrap();
        assert_eq!(state.total_webhooks, 1);
        assert!(state.trigger_stats.contains_key("test-trigger"));

        // Shutdown
        channel.shutdown().await.unwrap();

        // Clean up
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn webhook_init_loads_persisted_state() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_webhook_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let state_path = root.join("webhook_state.json");

        // Create persisted state
        let state = WebhookState {
            total_webhooks: 42,
            last_activity: Some(Utc::now()),
            trigger_stats: HashMap::from([("test-trigger".to_string(), 10)]),
        };
        std::fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap()).unwrap();

        // Create channel and init (should load state)
        let mut channel = WebhookChannel::new().with_state_path(&state_path);
        channel
            .init(&ChannelConfig {
                channel_type: ChannelType::new("webhook"),
                enabled: true,
                credentials: None,
                allow_from: vec![],
                dm_policy: crate::config::DmPolicy::Open,
                extra: HashMap::new(),
            })
            .await
            .unwrap();

        // Verify state was loaded
        assert_eq!(channel.total_webhooks(), 42);
        assert!(channel.has_restart_state());
        let stats = channel.trigger_stats.read().await;
        assert_eq!(stats.get("test-trigger"), Some(&10));

        // Clean up
        std::fs::remove_dir_all(root).unwrap();
    }
}
