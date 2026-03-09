//! MCP Transport implementations

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::MaybeTlsStream;

#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn connect(&mut self) -> Result<(), TransportError>;
    async fn send(&mut self, message: &str) -> Result<(), TransportError>;
    async fn receive(&mut self) -> Result<String, TransportError>;
    async fn close(&mut self) -> Result<(), TransportError>;
}

#[derive(Debug, Clone)]
pub enum McpTransportConfig {
    Stdio(StdioTransportConfig),
    Sse(SseTransportConfig),
    WebSocket(WebSocketTransportConfig),
}

#[derive(Debug, Clone)]
pub struct StdioTransportConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SseTransportConfig {
    pub url: String,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct WebSocketTransportConfig {
    pub url: String,
}

pub struct StdioTransport {
    config: StdioTransportConfig,
    child: Option<tokio::process::Child>,
    stdin: Option<tokio::io::BufWriter<tokio::process::ChildStdin>>,
    stdout: Option<BufReader<tokio::process::ChildStdout>>,
}

impl StdioTransport {
    pub fn new(config: StdioTransportConfig) -> Self {
        Self {
            config,
            child: None,
            stdin: None,
            stdout: None,
        }
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn connect(&mut self) -> Result<(), TransportError> {
        let mut cmd = tokio::process::Command::new(&self.config.command);
        cmd.args(&self.config.args);
        cmd.envs(&self.config.env);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| TransportError::ConnectionFailed("No stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TransportError::ConnectionFailed("No stdout".to_string()))?;

        self.stdin = Some(tokio::io::BufWriter::new(stdin));
        self.stdout = Some(BufReader::new(stdout));
        self.child = Some(child);

        Ok(())
    }

    async fn send(&mut self, message: &str) -> Result<(), TransportError> {
        let stdin = self.stdin.as_mut().ok_or(TransportError::NotConnected)?;
        stdin
            .write_all(message.as_bytes())
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<String, TransportError> {
        let stdout = self.stdout.as_mut().ok_or(TransportError::NotConnected)?;
        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .await
            .map_err(|e| TransportError::ReceiveFailed(e.to_string()))?;
        Ok(line)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        if let Some(mut child) = self.child.take() {
            child
                .kill()
                .await
                .map_err(|e| TransportError::CloseFailed(e.to_string()))?;
        }
        self.stdin = None;
        self.stdout = None;
        Ok(())
    }
}

pub struct SseTransport {
    config: SseTransportConfig,
    client: reqwest::Client,
    event_rx: Option<mpsc::Receiver<String>>,
}

impl SseTransport {
    pub fn new(config: SseTransportConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            event_rx: None,
        }
    }
}

#[async_trait]
impl McpTransport for SseTransport {
    async fn connect(&mut self) -> Result<(), TransportError> {
        let (tx, rx) = mpsc::channel(100);
        self.event_rx = Some(rx);

        let url = self.config.url.clone();
        let headers = self.config.headers.clone();

        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let mut request = client.get(&url);
            for (k, v) in &headers {
                request = request.header(k, v);
            }

            if let Ok(response) = request.send().await {
                let mut stream = response.bytes_stream();
                while let Some(chunk) = stream.next().await {
                    if let Ok(bytes) = chunk {
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            for line in text.lines() {
                                if line.starts_with("data: ") {
                                    let data = line.trim_start_matches("data: ");
                                    let _ = tx.send(data.to_string()).await;
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn send(&mut self, _message: &str) -> Result<(), TransportError> {
        Ok(())
    }

    async fn receive(&mut self) -> Result<String, TransportError> {
        let rx = self.event_rx.as_mut().ok_or(TransportError::NotConnected)?;
        rx.recv()
            .await
            .ok_or(TransportError::ReceiveFailed("Channel closed".to_string()))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.event_rx = None;
        Ok(())
    }
}

pub struct WebSocketTransport {
    config: WebSocketTransportConfig,
    ws: Option<tokio_tungstenite::WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl WebSocketTransport {
    pub fn new(config: WebSocketTransportConfig) -> Self {
        Self { config, ws: None }
    }
}

#[async_trait]
impl McpTransport for WebSocketTransport {
    async fn connect(&mut self) -> Result<(), TransportError> {
        let (ws, _) = tokio_tungstenite::connect_async(&self.config.url)
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        self.ws = Some(ws);
        Ok(())
    }

    async fn send(&mut self, message: &str) -> Result<(), TransportError> {
        let ws = self.ws.as_mut().ok_or(TransportError::NotConnected)?;
        ws.send(Message::Text(message.into()))
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<String, TransportError> {
        let ws = self.ws.as_mut().ok_or(TransportError::NotConnected)?;
        match ws.next().await {
            Some(Ok(Message::Text(text))) => Ok(text.to_string()),
            Some(Ok(Message::Close(_))) => Err(TransportError::ReceiveFailed(
                "Connection closed".to_string(),
            )),
            Some(Err(e)) => Err(TransportError::ReceiveFailed(e.to_string())),
            None => Err(TransportError::ReceiveFailed("Stream ended".to_string())),
            _ => Err(TransportError::ReceiveFailed(
                "Unexpected message type".to_string(),
            )),
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        if let Some(mut ws) = self.ws.take() {
            ws.close(None)
                .await
                .map_err(|e| TransportError::CloseFailed(e.to_string()))?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Not connected")]
    NotConnected,
    #[error("Send failed: {0}")]
    SendFailed(String),
    #[error("Receive failed: {0}")]
    ReceiveFailed(String),
    #[error("Close failed: {0}")]
    CloseFailed(String),
}
