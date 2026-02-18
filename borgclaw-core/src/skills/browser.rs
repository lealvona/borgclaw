//! Browser automation - Playwright (primary) + CDP (fallback)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BrowserType {
    Chromium,
    Firefox,
    Webkit,
}

impl Default for BrowserType {
    fn default() -> Self {
        Self::Chromium
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub browser: BrowserType,
    pub headless: bool,
    pub bridge_path: PathBuf,
    pub node_path: PathBuf,
    pub cdp_url: Option<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            browser: BrowserType::Chromium,
            headless: true,
            bridge_path: PathBuf::from(".local/tools/playwright/playwright-bridge.js"),
            node_path: PathBuf::from("node"),
            cdp_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: Option<bool>,
    pub http_only: Option<bool>,
    pub same_site: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeRequest {
    pub id: u64,
    pub action: String,
    pub args: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub id: u64,
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[async_trait]
pub trait BrowserSkill: Send + Sync {
    async fn navigate(&self, url: &str) -> Result<(), BrowserError>;
    async fn click(&self, selector: &str) -> Result<(), BrowserError>;
    async fn fill(&self, selector: &str, value: &str) -> Result<(), BrowserError>;
    async fn screenshot(&self) -> Result<Vec<u8>, BrowserError>;
    async fn wait_for(&self, selector: &str, timeout_ms: u64) -> Result<(), BrowserError>;
    async fn wait_for_text(&self, text: &str, timeout_ms: u64) -> Result<(), BrowserError>;
    async fn extract_text(&self, selector: &str) -> Result<String, BrowserError>;
    async fn extract_html(&self, selector: &str) -> Result<String, BrowserError>;
    async fn eval_js(&self, script: &str) -> Result<serde_json::Value, BrowserError>;
    async fn get_cookies(&self) -> Result<Vec<Cookie>, BrowserError>;
    async fn set_cookie(&self, cookie: Cookie) -> Result<(), BrowserError>;
    async fn get_url(&self) -> Result<String, BrowserError>;
    async fn close(&self) -> Result<(), BrowserError>;
}

pub struct PlaywrightClient {
    config: BrowserConfig,
    process: Arc<RwLock<Option<tokio::process::Child>>>,
    stdin: Arc<RwLock<Option<tokio::io::BufWriter<tokio::process::ChildStdin>>>>,
    stdout: Arc<RwLock<Option<tokio::io::BufReader<tokio::process::ChildStdout>>>>,
    pending: Arc<RwLock<HashMap<u64, mpsc::Sender<BridgeResponse>>>>,
    next_id: Arc<RwLock<u64>>,
}

impl PlaywrightClient {
    pub fn new(config: BrowserConfig) -> Self {
        Self {
            config,
            process: Arc::new(RwLock::new(None)),
            stdin: Arc::new(RwLock::new(None)),
            stdout: Arc::new(RwLock::new(None)),
            pending: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        }
    }

    pub async fn launch(&self) -> Result<(), BrowserError> {
        let mut cmd = tokio::process::Command::new(&self.config.node_path);
        cmd.arg(&self.config.bridge_path);
        cmd.arg("--browser");
        cmd.arg(match self.config.browser {
            BrowserType::Chromium => "chromium",
            BrowserType::Firefox => "firefox",
            BrowserType::Webkit => "webkit",
        });
        if self.config.headless {
            cmd.arg("--headless");
        }
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| BrowserError::LaunchFailed(e.to_string()))?;

        let stdin = child.stdin.take().ok_or(BrowserError::LaunchFailed("No stdin".to_string()))?;
        let stdout = child.stdout.take().ok_or(BrowserError::LaunchFailed("No stdout".to_string()))?;

        *self.process.write().await = Some(child);
        *self.stdin.write().await = Some(tokio::io::BufWriter::new(stdin));
        *self.stdout.write().await = Some(tokio::io::BufReader::new(stdout));

        let response = self.send_request("new_page", HashMap::new()).await?;
        if !response.success {
            return Err(BrowserError::ActionFailed(response.error.unwrap_or_default()));
        }

        Ok(())
    }

    async fn send_request(&self, action: &str, args: HashMap<String, serde_json::Value>) -> Result<BridgeResponse, BrowserError> {
        let id = {
            let mut next = self.next_id.write().await;
            let id = *next;
            *next += 1;
            id
        };

        let (tx, mut rx) = mpsc::channel(1);
        self.pending.write().await.insert(id, tx);

        let request = BridgeRequest {
            id,
            action: action.to_string(),
            args,
        };

        let request_json = serde_json::to_string(&request)
            .map_err(|e| BrowserError::ParseFailed(e.to_string()))?;

        let mut stdin = self.stdin.write().await;
        if let Some(ref mut s) = *stdin {
            s.write_all(request_json.as_bytes()).await
                .map_err(|e| BrowserError::SendFailed(e.to_string()))?;
            s.write_all(b"\n").await
                .map_err(|e| BrowserError::SendFailed(e.to_string()))?;
            s.flush().await
                .map_err(|e| BrowserError::SendFailed(e.to_string()))?;
        } else {
            return Err(BrowserError::NotConnected);
        }
        drop(stdin);

        let response = rx.recv().await.ok_or(BrowserError::Timeout)?;
        self.pending.write().await.remove(&id);

        Ok(response)
    }
}

#[async_trait]
impl BrowserSkill for PlaywrightClient {
    async fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("url".to_string(), serde_json::json!(url));
        let response = self.send_request("navigate", args).await?;
        if response.success { Ok(()) } else { Err(BrowserError::ActionFailed(response.error.unwrap_or_default())) }
    }

    async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("selector".to_string(), serde_json::json!(selector));
        let response = self.send_request("click", args).await?;
        if response.success { Ok(()) } else { Err(BrowserError::ActionFailed(response.error.unwrap_or_default())) }
    }

    async fn fill(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("selector".to_string(), serde_json::json!(selector));
        args.insert("value".to_string(), serde_json::json!(value));
        let response = self.send_request("fill", args).await?;
        if response.success { Ok(()) } else { Err(BrowserError::ActionFailed(response.error.unwrap_or_default())) }
    }

    async fn screenshot(&self) -> Result<Vec<u8>, BrowserError> {
        let args = HashMap::new();
        let response = self.send_request("screenshot", args).await?;
        if let Some(result) = response.result {
            let base64_data = result.as_str().ok_or(BrowserError::ParseFailed("Expected string".to_string()))?;
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD.decode(base64_data)
                .map_err(|e| BrowserError::ParseFailed(e.to_string()))?;
            Ok(bytes)
        } else {
            Err(BrowserError::ActionFailed(response.error.unwrap_or_default()))
        }
    }

    async fn wait_for(&self, selector: &str, timeout_ms: u64) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("selector".to_string(), serde_json::json!(selector));
        args.insert("timeout".to_string(), serde_json::json!(timeout_ms));
        let response = self.send_request("wait_for", args).await?;
        if response.success { Ok(()) } else { Err(BrowserError::ActionFailed(response.error.unwrap_or_default())) }
    }

    async fn wait_for_text(&self, text: &str, timeout_ms: u64) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("text".to_string(), serde_json::json!(text));
        args.insert("timeout".to_string(), serde_json::json!(timeout_ms));
        let response = self.send_request("wait_for_text", args).await?;
        if response.success { Ok(()) } else { Err(BrowserError::ActionFailed(response.error.unwrap_or_default())) }
    }

    async fn extract_text(&self, selector: &str) -> Result<String, BrowserError> {
        let mut args = HashMap::new();
        args.insert("selector".to_string(), serde_json::json!(selector));
        let response = self.send_request("extract_text", args).await?;
        if let Some(result) = response.result {
            result.as_str()
                .map(|s| s.to_string())
                .ok_or(BrowserError::ParseFailed("Expected string".to_string()))
        } else {
            Err(BrowserError::ActionFailed(response.error.unwrap_or_default()))
        }
    }

    async fn extract_html(&self, selector: &str) -> Result<String, BrowserError> {
        let mut args = HashMap::new();
        args.insert("selector".to_string(), serde_json::json!(selector));
        let response = self.send_request("extract_html", args).await?;
        if let Some(result) = response.result {
            result.as_str()
                .map(|s| s.to_string())
                .ok_or(BrowserError::ParseFailed("Expected string".to_string()))
        } else {
            Err(BrowserError::ActionFailed(response.error.unwrap_or_default()))
        }
    }

    async fn eval_js(&self, script: &str) -> Result<serde_json::Value, BrowserError> {
        let mut args = HashMap::new();
        args.insert("script".to_string(), serde_json::json!(script));
        let response = self.send_request("eval_js", args).await?;
        response.result.ok_or_else(|| BrowserError::ActionFailed(response.error.unwrap_or_default()))
    }

    async fn get_cookies(&self) -> Result<Vec<Cookie>, BrowserError> {
        let args = HashMap::new();
        let response = self.send_request("get_cookies", args).await?;
        if let Some(result) = response.result {
            serde_json::from_value(result)
                .map_err(|e| BrowserError::ParseFailed(e.to_string()))
        } else {
            Err(BrowserError::ActionFailed(response.error.unwrap_or_default()))
        }
    }

    async fn set_cookie(&self, cookie: Cookie) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("cookie".to_string(), serde_json::to_value(cookie).map_err(|e| BrowserError::ParseFailed(e.to_string()))?);
        let response = self.send_request("set_cookie", args).await?;
        if response.success { Ok(()) } else { Err(BrowserError::ActionFailed(response.error.unwrap_or_default())) }
    }

    async fn get_url(&self) -> Result<String, BrowserError> {
        let args = HashMap::new();
        let response = self.send_request("get_url", args).await?;
        if let Some(result) = response.result {
            result.as_str()
                .map(|s| s.to_string())
                .ok_or(BrowserError::ParseFailed("Expected string".to_string()))
        } else {
            Err(BrowserError::ActionFailed(response.error.unwrap_or_default()))
        }
    }

    async fn close(&self) -> Result<(), BrowserError> {
        let args = HashMap::new();
        let _ = self.send_request("close", args).await;
        
        if let Some(mut child) = self.process.write().await.take() {
            child.kill().await.ok();
        }
        
        *self.stdin.write().await = None;
        *self.stdout.write().await = None;
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("Launch failed: {0}")]
    LaunchFailed(String),
    
    #[error("Not connected")]
    NotConnected,
    
    #[error("Send failed: {0}")]
    SendFailed(String),
    
    #[error("Action failed: {0}")]
    ActionFailed(String),
    
    #[error("Parse failed: {0}")]
    ParseFailed(String),
    
    #[error("Timeout")]
    Timeout,
}
