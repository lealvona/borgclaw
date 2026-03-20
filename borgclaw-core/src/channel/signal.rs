//! Signal channel implementation using signal-cli (primary) with JSON-RPC interface with restart recovery

use super::{
    Channel, ChannelConfig, ChannelError, ChannelStatus, ChannelType, InboundMessage,
    MessagePayload, OutboundMessage, Sender,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

/// Persistent state for restart recovery
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignalState {
    /// Last processed message timestamp
    pub last_timestamp: Option<i64>,
    /// Total messages processed (for stats)
    pub total_messages: u64,
    /// Last activity timestamp
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct SignalChannel {
    channel_type: ChannelType,
    status: Arc<RwLock<ChannelStatus>>,
    msg_count: Arc<AtomicU64>,
    phone_number: Option<String>,
    signal_cli_path: PathBuf,
    data_path: PathBuf,
    running: Arc<AtomicBool>,
    loop_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    allowed_users: Vec<String>,
    state_path: Option<PathBuf>,
    last_timestamp: Arc<RwLock<Option<i64>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalMessage {
    envelope: SignalEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalEnvelope {
    source: String,
    #[serde(rename = "sourceNumber")]
    source_number: Option<String>,
    #[serde(rename = "sourceName")]
    source_name: Option<String>,
    timestamp: i64,
    #[serde(rename = "dataMessage")]
    data_message: Option<SignalDataMessage>,
    #[serde(rename = "syncMessage")]
    sync_message: Option<SignalSyncMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalDataMessage {
    timestamp: i64,
    message: Option<String>,
    #[serde(rename = "groupInfo")]
    group_info: Option<SignalGroupInfo>,
    attachments: Option<Vec<SignalAttachment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalSyncMessage {
    #[serde(rename = "sentMessage")]
    sent_message: Option<SignalDataMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalGroupInfo {
    #[serde(rename = "groupId")]
    group_id: String,
    #[serde(rename = "groupName")]
    group_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalAttachment {
    #[serde(rename = "contentType")]
    content_type: String,
    filename: Option<String>,
    id: Option<String>,
    size: Option<u64>,
}

impl SignalChannel {
    pub fn new() -> Self {
        Self {
            channel_type: ChannelType::signal(),
            status: Arc::new(RwLock::new(ChannelStatus::disconnected())),
            msg_count: Arc::new(AtomicU64::new(0)),
            phone_number: None,
            signal_cli_path: PathBuf::from("signal-cli"),
            data_path: PathBuf::from(".borgclaw/signal-data"),
            running: Arc::new(AtomicBool::new(false)),
            loop_handle: Arc::new(Mutex::new(None)),
            allowed_users: Vec::new(),
            state_path: None,
            last_timestamp: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_phone_number(mut self, phone: impl Into<String>) -> Self {
        self.phone_number = Some(phone.into());
        self
    }

    pub fn with_signal_cli_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.signal_cli_path = path.into();
        self
    }

    pub fn with_data_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.data_path = path.into();
        self
    }

    pub fn with_state_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.state_path = Some(path.into());
        self
    }

    /// Load persisted state for restart recovery
    fn load_state(&self) -> SignalState {
        if let Some(ref path) = self.state_path {
            if let Ok(contents) = std::fs::read_to_string(path) {
                if let Ok(state) = serde_json::from_str(&contents) {
                    return state;
                }
            }
        }
        SignalState::default()
    }

    /// Persist state for restart recovery
    async fn persist_state(&self) {
        if let Some(ref path) = self.state_path {
            let state = SignalState {
                last_timestamp: *self.last_timestamp.read().await,
                total_messages: self.msg_count.load(Ordering::Relaxed),
                last_activity: self.status.read().await.last_activity,
            };
            if let Ok(contents) = serde_json::to_string_pretty(&state) {
                let _ = std::fs::write(path, contents);
            }
        }
    }

    /// Health check for signal-cli
    pub async fn health_check(&self) -> Result<(), ChannelError> {
        if !self.signal_cli_path.exists() && !self.signal_cli_path.to_string_lossy().contains('/') {
            // Try to find in PATH
            match Command::new("which").arg(&self.signal_cli_path).output() {
                Ok(output) if output.status.success() => Ok(()),
                _ => Err(ChannelError::ConnectionFailed(
                    "signal-cli not found in PATH".to_string(),
                )),
            }
        } else if self.signal_cli_path.exists() {
            Ok(())
        } else {
            Err(ChannelError::ConnectionFailed(format!(
                "signal-cli not found at {}",
                self.signal_cli_path.display()
            )))
        }
    }

    /// Check if restart recovery state is available
    pub fn has_restart_state(&self) -> bool {
        self.state_path.as_ref().map_or(false, |p| p.exists())
    }

    /// Get last timestamp for restart recovery
    pub async fn last_timestamp(&self) -> Option<i64> {
        *self.last_timestamp.read().await
    }

    pub fn with_allowed_users(mut self, users: Vec<String>) -> Self {
        self.allowed_users = users;
        self
    }

    fn check_signal_cli_available(&self) -> Result<(), ChannelError> {
        let output = Command::new(&self.signal_cli_path)
            .arg("--version")
            .output()
            .map_err(|e| ChannelError::ConnectionFailed(format!("signal-cli not found: {}", e)))?;

        if !output.status.success() {
            return Err(ChannelError::ConnectionFailed(
                "signal-cli version check failed".to_string(),
            ));
        }

        Ok(())
    }

    async fn receive_messages(&self, sender: mpsc::Sender<InboundMessage>) {
        let phone = match &self.phone_number {
            Some(p) => p.clone(),
            None => {
                log::error!("Signal: No phone number configured");
                return;
            }
        };

        let signal_cli = self.signal_cli_path.clone();
        let data_path = self.data_path.clone();
        let running = self.running.clone();
        let allowed_users = self.allowed_users.clone();
        let status = self.status.clone();
        let channel_type = self.channel_type.clone();
        let msg_count = self.msg_count.clone();
        let loop_handle = self.loop_handle.clone();

        let handle = tokio::spawn(async move {
            log::info!("Signal: Starting message receiver for {}", phone);

            while running.load(Ordering::Relaxed) {
                let output = Command::new(&signal_cli)
                    .arg("-u")
                    .arg(&phone)
                    .arg("--config")
                    .arg(&data_path)
                    .arg("receive")
                    .arg("--json")
                    .arg("-t")
                    .arg("-1")
                    .output();

                match output {
                    Ok(out) if out.status.success() => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        for line in stdout.lines() {
                            if line.trim().is_empty() {
                                continue;
                            }

                            if let Ok(msg) = serde_json::from_str::<SignalMessage>(line) {
                                if let Some(ref data_msg) = msg.envelope.data_message {
                                    if let Some(ref text) = data_msg.message {
                                        let source = msg
                                            .envelope
                                            .source_number
                                            .clone()
                                            .unwrap_or_else(|| msg.envelope.source.clone());
                                        let source_name =
                                            msg.envelope.source_name.clone().unwrap_or_default();
                                        let raw = serde_json::to_value(&msg)
                                            .unwrap_or(serde_json::json!({}));
                                        let group_id =
                                            data_msg.group_info.as_ref().map(|g| g.group_id.clone());
                                        let timestamp = chrono::DateTime::from_timestamp(
                                            data_msg.timestamp / 1000,
                                            0,
                                        )
                                        .unwrap_or_else(Utc::now);

                                        if !allowed_users.is_empty()
                                            && !allowed_users.contains(&source)
                                        {
                                            log::debug!(
                                                "Signal: Ignoring message from unallowed user: {}",
                                                source
                                            );
                                            continue;
                                        }

                                        let inbound = InboundMessage {
                                            channel: channel_type.clone(),
                                            sender: Sender::new(&source).with_name(source_name),
                                            content: MessagePayload::Text(text.clone()),
                                            group_id,
                                            timestamp,
                                            raw,
                                        };

                                        msg_count.fetch_add(1, Ordering::Relaxed);
                                        {
                                            let mut s = status.write().await;
                                            s.last_activity = Some(Utc::now());
                                        }

                                        if sender.send(inbound).await.is_err() {
                                            log::error!("Signal: Failed to send message to router");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(out) => {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        log::warn!("Signal: receive command failed: {}", stderr);
                    }
                    Err(e) => {
                        log::error!("Signal: Failed to execute receive command: {}", e);
                    }
                }

                sleep(Duration::from_secs(2)).await;
            }

            log::info!("Signal: Message receiver stopped");
        });

        *loop_handle.lock().await = Some(handle);
    }
}

impl Default for SignalChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Channel for SignalChannel {
    fn channel_type(&self) -> ChannelType {
        self.channel_type.clone()
    }

    async fn init(&mut self, config: &ChannelConfig) -> Result<(), ChannelError> {
        self.check_signal_cli_available()?;

        self.phone_number = config
            .extra
            .get("phone_number")
            .or_else(|| config.extra.get("phone"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if self.phone_number.is_none() {
            return Err(ChannelError::AuthFailed(
                "Signal phone number not configured. Set channels.signal.phone_number in config."
                    .to_string(),
            ));
        }

        if let Some(path) = config.extra.get("signal_cli_path").and_then(|v| v.as_str()) {
            self.signal_cli_path = PathBuf::from(path);
        }

        if let Some(path) = config.extra.get("data_path").and_then(|v| v.as_str()) {
            self.data_path = PathBuf::from(path);
        }

        self.allowed_users = config.allow_from.clone();

        std::fs::create_dir_all(&self.data_path).map_err(|e| {
            ChannelError::ConnectionFailed(format!("Failed to create data dir: {}", e))
        })?;

        let phone = self.phone_number.as_ref().unwrap();
        let output = Command::new(&self.signal_cli_path)
            .arg("-u")
            .arg(phone)
            .arg("--config")
            .arg(&self.data_path)
            .arg("getUserStatus")
            .arg(phone)
            .output()
            .map_err(|e| ChannelError::AuthFailed(format!("Failed to check user status: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not registered") || stderr.contains("DeviceId") {
                return Err(ChannelError::AuthFailed(
                    "Signal number not registered. Run 'signal-cli -u <phone> register' first."
                        .to_string(),
                ));
            }
        }

        *self.status.write().await = ChannelStatus::connected();

        // Load persisted state for restart recovery
        let state = self.load_state();
        *self.last_timestamp.write().await = state.last_timestamp;
        self.msg_count.store(state.total_messages, Ordering::Relaxed);
        if let Some(last_activity) = state.last_activity {
            self.status.write().await.last_activity = Some(last_activity);
        }

        log::info!("Signal: Channel initialized for {}", phone);

        Ok(())
    }

    async fn start_receiving(
        &self,
        sender: mpsc::Sender<InboundMessage>,
    ) -> Result<(), ChannelError> {
        if !self.status.read().await.connected {
            return Err(ChannelError::ConnectionFailed(
                "Channel not initialized".to_string(),
            ));
        }

        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(ChannelError::ConnectionFailed(
                "Signal receiver already running".to_string(),
            ));
        }

        if let Err(err) = self.health_check().await {
            self.running.store(false, Ordering::SeqCst);
            return Err(err);
        }

        self.receive_messages(sender).await;

        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), ChannelError> {
        let phone = self
            .phone_number
            .as_ref()
            .ok_or_else(|| ChannelError::SendFailed("Phone number not configured".to_string()))?;

        if !self.status.read().await.connected {
            return Err(ChannelError::ConnectionFailed(
                "Channel not connected".to_string(),
            ));
        }

        let text = match &message.content {
            MessagePayload::Text(s) => s.clone(),
            MessagePayload::Markdown(s) => s.clone(),
            MessagePayload::Html(s) => s.clone(),
            MessagePayload::Media { url, .. } => url.clone(),
            MessagePayload::File { name, .. } => format!("[File: {}]", name),
        };

        let mut cmd = Command::new(&self.signal_cli_path);
        cmd.arg("-u")
            .arg(phone)
            .arg("--config")
            .arg(&self.data_path)
            .arg("send")
            .arg(&message.target)
            .arg("-m")
            .arg(&text);

        if let Some(group_id) = &message.group_id {
            cmd.arg("-g").arg(group_id);
        }

        let output = cmd.output().map_err(|e| {
            ChannelError::SendFailed(format!("Failed to execute signal-cli: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ChannelError::SendFailed(format!("Send failed: {}", stderr)));
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
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.loop_handle.lock().await.take() {
            handle.abort();
        }
        // Persist final state before shutdown
        self.persist_state().await;
        *self.status.write().await = ChannelStatus::disconnected();
        log::info!("Signal: Channel shutdown");
        Ok(())
    }
}

pub struct SignalChannelBuilder {
    phone_number: Option<String>,
    signal_cli_path: PathBuf,
    data_path: PathBuf,
    allowed_users: Vec<String>,
}

impl SignalChannelBuilder {
    pub fn new() -> Self {
        Self {
            phone_number: None,
            signal_cli_path: PathBuf::from("signal-cli"),
            data_path: PathBuf::from(".borgclaw/signal-data"),
            allowed_users: Vec::new(),
        }
    }

    pub fn phone_number(mut self, phone: impl Into<String>) -> Self {
        self.phone_number = Some(phone.into());
        self
    }

    pub fn signal_cli_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.signal_cli_path = path.into();
        self
    }

    pub fn data_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.data_path = path.into();
        self
    }

    pub fn allowed_users(mut self, users: Vec<String>) -> Self {
        self.allowed_users = users;
        self
    }

    pub fn build(self) -> SignalChannel {
        SignalChannel {
            channel_type: ChannelType::signal(),
            status: Arc::new(RwLock::new(ChannelStatus::disconnected())),
            msg_count: Arc::new(AtomicU64::new(0)),
            phone_number: self.phone_number,
            signal_cli_path: self.signal_cli_path,
            data_path: self.data_path,
            running: Arc::new(AtomicBool::new(false)),
            loop_handle: Arc::new(Mutex::new(None)),
            allowed_users: self.allowed_users,
            state_path: None,
            last_timestamp: Arc::new(RwLock::new(None)),
        }
    }
}

impl Default for SignalChannelBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_signal_cli_script() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("borgclaw_signal_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("signal-cli");
        std::fs::write(
            &path,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo 1.0.0\n  exit 0\nfi\ncase \"$5\" in\n  getUserStatus)\n    echo '{\"registered\":true}'\n    exit 0\n    ;;\n  receive)\n    sleep 5\n    exit 0\n    ;;\nesac\nexit 0\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
        }
        path
    }

    #[tokio::test]
    async fn signal_start_receiving_rejects_duplicate_starts_and_shutdown_clears_handle() {
        let script = fake_signal_cli_script();
        let channel = SignalChannelBuilder::new()
            .phone_number("+15551234567")
            .signal_cli_path(script)
            .build();
        *channel.status.write().await = ChannelStatus::connected();

        let (tx, _rx) = mpsc::channel(1);
        channel.start_receiving(tx.clone()).await.unwrap();

        let err = channel.start_receiving(tx).await.unwrap_err();
        assert!(err.to_string().contains("already running"));

        channel.shutdown().await.unwrap();
        assert!(!channel.running.load(Ordering::SeqCst));
        assert!(channel.loop_handle.lock().await.is_none());
    }
}
