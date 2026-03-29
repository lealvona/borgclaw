//! Browser automation - Playwright (primary) + CDP (fallback)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BrowserType {
    #[default]
    Chromium,
    Firefox,
    Webkit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    pub id: Option<u64>,
    pub success: bool,
    #[serde(default, alias = "data")]
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
    pending: Arc<RwLock<HashMap<u64, mpsc::Sender<BridgeResponse>>>>,
    next_id: Arc<RwLock<u64>>,
}

impl PlaywrightClient {
    pub fn new(config: BrowserConfig) -> Self {
        Self {
            config,
            process: Arc::new(RwLock::new(None)),
            stdin: Arc::new(RwLock::new(None)),
            pending: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        }
    }

    fn launch_args(&self) -> Vec<String> {
        let mut args = vec![
            self.config.bridge_path.display().to_string(),
            "--browser".to_string(),
            match self.config.browser {
                BrowserType::Chromium => "chromium".to_string(),
                BrowserType::Firefox => "firefox".to_string(),
                BrowserType::Webkit => "webkit".to_string(),
            },
        ];
        if self.config.headless {
            args.push("--headless".to_string());
        }
        if let Some(cdp_url) = &self.config.cdp_url {
            args.push("--cdp-url".to_string());
            args.push(cdp_url.clone());
        }
        args
    }

    pub async fn launch(&self) -> Result<(), BrowserError> {
        if self.process.read().await.is_some() {
            return Ok(());
        }

        let mut cmd = tokio::process::Command::new(&self.config.node_path);
        for arg in self.launch_args() {
            cmd.arg(arg);
        }
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| BrowserError::LaunchFailed(e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or(BrowserError::LaunchFailed("No stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(BrowserError::LaunchFailed("No stdout".to_string()))?;

        let pending = self.pending.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(response) = serde_json::from_str::<BridgeResponse>(&line) else {
                    continue;
                };
                let Some(id) = response.id else {
                    continue;
                };
                let sender = {
                    let pending = pending.read().await;
                    pending.get(&id).cloned()
                };
                if let Some(sender) = sender {
                    let _ = sender.send(response).await;
                }
            }
        });

        *self.process.write().await = Some(child);
        *self.stdin.write().await = Some(tokio::io::BufWriter::new(stdin));

        let response = self.send_request("new_page", HashMap::new()).await?;
        if !response.success {
            return Err(BrowserError::ActionFailed(
                response.error.unwrap_or_default(),
            ));
        }

        Ok(())
    }

    async fn send_request(
        &self,
        action: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<BridgeResponse, BrowserError> {
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
            s.write_all(request_json.as_bytes())
                .await
                .map_err(|e| BrowserError::SendFailed(e.to_string()))?;
            s.write_all(b"\n")
                .await
                .map_err(|e| BrowserError::SendFailed(e.to_string()))?;
            s.flush()
                .await
                .map_err(|e| BrowserError::SendFailed(e.to_string()))?;
        } else {
            return Err(BrowserError::NotConnected);
        }
        drop(stdin);

        let response = rx.recv().await.ok_or(BrowserError::Timeout)?;
        self.pending.write().await.remove(&id);

        Ok(response)
    }

    fn response_value<'a>(
        response: &'a BridgeResponse,
        field: Option<&str>,
    ) -> Result<&'a serde_json::Value, BrowserError> {
        let Some(result) = response.result.as_ref() else {
            return Err(BrowserError::ActionFailed(
                response.error.clone().unwrap_or_default(),
            ));
        };

        if let Some(field) = field {
            result.get(field).ok_or_else(|| {
                BrowserError::ParseFailed(format!("Expected '{}' field in bridge response", field))
            })
        } else {
            Ok(result)
        }
    }

    async fn capture_screenshot(&self, full_page: bool) -> Result<Vec<u8>, BrowserError> {
        let mut args = HashMap::new();
        args.insert("fullPage".to_string(), serde_json::json!(full_page));
        let response = self.send_request("screenshot", args).await?;
        let base64_data = Self::response_value(&response, Some("image"))?
            .as_str()
            .ok_or(BrowserError::ParseFailed(
                "Expected base64 image".to_string(),
            ))?;
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| BrowserError::ParseFailed(e.to_string()))
    }

    pub async fn screenshot(&self, full_page: bool) -> Result<Vec<u8>, BrowserError> {
        self.capture_screenshot(full_page).await
    }

    pub async fn get_text(&self, selector: &str) -> Result<String, BrowserError> {
        self.extract_text(selector).await
    }

    pub async fn get_html(&self, selector: &str) -> Result<String, BrowserError> {
        self.extract_html(selector).await
    }
}

#[async_trait]
impl BrowserSkill for PlaywrightClient {
    async fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("url".to_string(), serde_json::json!(url));
        let response = self.send_request("navigate", args).await?;
        if response.success {
            Ok(())
        } else {
            Err(BrowserError::ActionFailed(
                response.error.unwrap_or_default(),
            ))
        }
    }

    async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("selector".to_string(), serde_json::json!(selector));
        let response = self.send_request("click", args).await?;
        if response.success {
            Ok(())
        } else {
            Err(BrowserError::ActionFailed(
                response.error.unwrap_or_default(),
            ))
        }
    }

    async fn fill(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("selector".to_string(), serde_json::json!(selector));
        args.insert("value".to_string(), serde_json::json!(value));
        let response = self.send_request("fill", args).await?;
        if response.success {
            Ok(())
        } else {
            Err(BrowserError::ActionFailed(
                response.error.unwrap_or_default(),
            ))
        }
    }

    async fn screenshot(&self) -> Result<Vec<u8>, BrowserError> {
        self.capture_screenshot(true).await
    }

    async fn wait_for(&self, selector: &str, timeout_ms: u64) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("selector".to_string(), serde_json::json!(selector));
        args.insert("timeout".to_string(), serde_json::json!(timeout_ms));
        let response = self.send_request("wait_for_selector", args).await?;
        if response.success {
            Ok(())
        } else {
            Err(BrowserError::ActionFailed(
                response.error.unwrap_or_default(),
            ))
        }
    }

    async fn wait_for_text(&self, text: &str, timeout_ms: u64) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert("text".to_string(), serde_json::json!(text));
        args.insert("timeout".to_string(), serde_json::json!(timeout_ms));
        let response = self.send_request("wait_for_text", args).await?;
        if response.success {
            Ok(())
        } else {
            Err(BrowserError::ActionFailed(
                response.error.unwrap_or_default(),
            ))
        }
    }

    async fn extract_text(&self, selector: &str) -> Result<String, BrowserError> {
        let mut args = HashMap::new();
        args.insert("selector".to_string(), serde_json::json!(selector));
        let response = self.send_request("get_text", args).await?;
        Self::response_value(&response, Some("text"))?
            .as_str()
            .map(str::to_string)
            .ok_or(BrowserError::ParseFailed(
                "Expected text string".to_string(),
            ))
    }

    async fn extract_html(&self, selector: &str) -> Result<String, BrowserError> {
        let mut args = HashMap::new();
        args.insert("selector".to_string(), serde_json::json!(selector));
        let response = self.send_request("get_html", args).await?;
        Self::response_value(&response, Some("html"))?
            .as_str()
            .map(str::to_string)
            .ok_or(BrowserError::ParseFailed(
                "Expected html string".to_string(),
            ))
    }

    async fn eval_js(&self, script: &str) -> Result<serde_json::Value, BrowserError> {
        let mut args = HashMap::new();
        args.insert("script".to_string(), serde_json::json!(script));
        let response = self.send_request("evaluate", args).await?;
        Self::response_value(&response, None).cloned()
    }

    async fn get_cookies(&self) -> Result<Vec<Cookie>, BrowserError> {
        let args = HashMap::new();
        let response = self.send_request("get_cookies", args).await?;
        serde_json::from_value(Self::response_value(&response, Some("cookies"))?.clone())
            .map_err(|e| BrowserError::ParseFailed(e.to_string()))
    }

    async fn set_cookie(&self, cookie: Cookie) -> Result<(), BrowserError> {
        let mut args = HashMap::new();
        args.insert(
            "cookie".to_string(),
            serde_json::to_value(cookie).map_err(|e| BrowserError::ParseFailed(e.to_string()))?,
        );
        let response = self.send_request("set_cookie", args).await?;
        if response.success {
            Ok(())
        } else {
            Err(BrowserError::ActionFailed(
                response.error.unwrap_or_default(),
            ))
        }
    }

    async fn get_url(&self) -> Result<String, BrowserError> {
        let args = HashMap::new();
        let response = self.send_request("get_url", args).await?;
        Self::response_value(&response, Some("url"))?
            .as_str()
            .map(str::to_string)
            .ok_or(BrowserError::ParseFailed("Expected url string".to_string()))
    }

    async fn close(&self) -> Result<(), BrowserError> {
        let args = HashMap::new();
        let _ = self.send_request("close", args).await;

        if let Some(mut child) = self.process.write().await.take() {
            child.kill().await.ok();
        }

        *self.stdin.write().await = None;

        Ok(())
    }
}

pub type PlaywrightConfig = BrowserConfig;

pub struct CdpClient {
    inner: PlaywrightClient,
}

impl CdpClient {
    pub fn new(url: impl Into<String>) -> Self {
        let config = BrowserConfig {
            browser: BrowserType::Chromium,
            cdp_url: Some(url.into()),
            ..BrowserConfig::default()
        };
        Self {
            inner: PlaywrightClient::new(config),
        }
    }

    pub async fn launch(&self) -> Result<(), BrowserError> {
        self.inner.launch().await
    }
}

#[async_trait]
impl BrowserSkill for CdpClient {
    async fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        self.inner.navigate(url).await
    }

    async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        self.inner.click(selector).await
    }

    async fn fill(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        self.inner.fill(selector, value).await
    }

    async fn screenshot(&self) -> Result<Vec<u8>, BrowserError> {
        BrowserSkill::screenshot(&self.inner).await
    }

    async fn wait_for(&self, selector: &str, timeout_ms: u64) -> Result<(), BrowserError> {
        self.inner.wait_for(selector, timeout_ms).await
    }

    async fn wait_for_text(&self, text: &str, timeout_ms: u64) -> Result<(), BrowserError> {
        self.inner.wait_for_text(text, timeout_ms).await
    }

    async fn extract_text(&self, selector: &str) -> Result<String, BrowserError> {
        self.inner.extract_text(selector).await
    }

    async fn extract_html(&self, selector: &str) -> Result<String, BrowserError> {
        self.inner.extract_html(selector).await
    }

    async fn eval_js(&self, script: &str) -> Result<serde_json::Value, BrowserError> {
        self.inner.eval_js(script).await
    }

    async fn get_cookies(&self) -> Result<Vec<Cookie>, BrowserError> {
        self.inner.get_cookies().await
    }

    async fn set_cookie(&self, cookie: Cookie) -> Result<(), BrowserError> {
        self.inner.set_cookie(cookie).await
    }

    async fn get_url(&self) -> Result<String, BrowserError> {
        self.inner.get_url().await
    }

    async fn close(&self) -> Result<(), BrowserError> {
        self.inner.close().await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_response_accepts_bridge_data_payloads() {
        let response: BridgeResponse =
            serde_json::from_str(r#"{"id":1,"success":true,"data":{"text":"hello"}}"#).unwrap();

        assert_eq!(response.result.unwrap()["text"].as_str(), Some("hello"));
    }

    #[test]
    fn playwright_launch_args_include_cdp_when_configured() {
        let client = PlaywrightClient::new(BrowserConfig {
            cdp_url: Some("http://localhost:9222".to_string()),
            ..BrowserConfig::default()
        });

        let args = client.launch_args();

        assert!(args
            .windows(2)
            .any(|pair| { pair[0] == "--cdp-url" && pair[1] == "http://localhost:9222" }));
    }

    #[test]
    fn cdp_client_uses_chromium_with_cdp_url() {
        let client = CdpClient::new("http://localhost:9222");

        assert_eq!(client.inner.config.browser, BrowserType::Chromium);
        assert_eq!(
            client.inner.config.cdp_url.as_deref(),
            Some("http://localhost:9222")
        );
    }
}
