//! Signal channel implementation using signal-cli (primary) with JSON-RPC interface

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
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration};

pub struct SignalChannel {
    channel_type: ChannelType,
    status: Arc<RwLock<ChannelStatus>>,
    msg_count: Arc<AtomicU64>,
    phone_number: Option<String>,
    signal_cli_path: PathBuf,
    data_path: PathBuf,
    running: Arc<AtomicBool>,
    allowed_users: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalMessage {
    envelope: SignalEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalEnvelope {
    source: String,
    sourceNumber: Option<String>,
    sourceName: Option<String>,
    timestamp: i64,
    dataMessage: Option<SignalDataMessage>,
    syncMessage: Option<SignalSyncMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalDataMessage {
    timestamp: i64,
    message: Option<String>,
    groupInfo: Option<SignalGroupInfo>,
    attachments: Option<Vec<SignalAttachment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalSyncMessage {
    sentMessage: Option<SignalDataMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalGroupInfo {
    groupId: String,
    groupName: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalAttachment {
    contentType: String,
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
            allowed_users: Vec::new(),
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

        tokio::spawn(async move {
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
                                if let Some(ref data_msg) = msg.envelope.dataMessage {
                                    if let Some(ref text) = data_msg.message {
                                        let source = msg
                                            .envelope
                                            .sourceNumber
                                            .clone()
                                            .unwrap_or_else(|| msg.envelope.source.clone());
                                        let source_name =
                                            msg.envelope.sourceName.clone().unwrap_or_default();
                                        let raw = serde_json::to_value(&msg)
                                            .unwrap_or(serde_json::json!({}));
                                        let group_id =
                                            data_msg.groupInfo.as_ref().map(|g| g.groupId.clone());
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
            .get("phone")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if self.phone_number.is_none() {
            return Err(ChannelError::AuthFailed(
                "Signal phone number not configured. Set channel.phone in config.".to_string(),
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

        self.running.store(true, Ordering::Relaxed);
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
        self.running.store(false, Ordering::Relaxed);
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
            allowed_users: self.allowed_users,
        }
    }
}

impl Default for SignalChannelBuilder {
    fn default() -> Self {
        Self::new()
    }
}
