//! MCP Client implementation

use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::transport::{McpTransport, McpTransportConfig, TransportError};
use super::types::*;

pub struct McpClientConfig {
    pub name: String,
    pub transport_config: McpTransportConfig,
    pub protocol_version: String,
}

pub struct McpClient {
    config: McpClientConfig,
    transport: Box<dyn McpTransport>,
    request_id: AtomicU64,
    initialized: Arc<RwLock<bool>>,
    server_info: Arc<RwLock<Option<McpServerInfo>>>,
}

impl McpClient {
    pub fn new(config: McpClientConfig) -> Self {
        let transport: Box<dyn McpTransport> = match config.transport_config.clone() {
            McpTransportConfig::Stdio(c) => Box::new(super::transport::StdioTransport::new(c)),
            McpTransportConfig::Sse(c) => Box::new(super::transport::SseTransport::new(c)),
            McpTransportConfig::WebSocket(c) => {
                Box::new(super::transport::WebSocketTransport::new(c))
            }
        };

        Self {
            config,
            transport,
            request_id: AtomicU64::new(0),
            initialized: Arc::new(RwLock::new(false)),
            server_info: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn connect(transport_config: McpTransportConfig) -> Result<Self, McpError> {
        let mut client = Self::new(McpClientConfig {
            name: "borgclaw".to_string(),
            transport_config,
            protocol_version: "2024-11-05".to_string(),
        });
        client.initialize().await?;
        Ok(client)
    }

    pub async fn initialize(&mut self) -> Result<(), McpError> {
        self.transport
            .connect()
            .await
            .map_err(McpError::Transport)?;

        let request = McpJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(1),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": self.config.protocol_version,
                "capabilities": {},
                "clientInfo": {
                    "name": self.config.name,
                    "version": "0.1.0"
                }
            })),
        };

        let response = self.send_request(request).await?;

        if let Some(result) = response.result {
            let init_result: McpInitializeResult =
                serde_json::from_value(result).map_err(|e| McpError::ParseFailed(e.to_string()))?;

            *self.server_info.write().await = Some(init_result.server_info);
            *self.initialized.write().await = true;

            let notification = McpJsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::Value::Null,
                method: "notifications/initialized".to_string(),
                params: None,
            };
            self.send_notification(notification).await?;

            Ok(())
        } else if let Some(error) = response.error {
            Err(McpError::RpcError(error.code, error.message))
        } else {
            Err(McpError::NoResponse)
        }
    }

    async fn send_request(
        &mut self,
        request: McpJsonRpcRequest,
    ) -> Result<McpJsonRpcResponse, McpError> {
        let request_json =
            serde_json::to_string(&request).map_err(|e| McpError::ParseFailed(e.to_string()))?;

        self.transport
            .send(&request_json)
            .await
            .map_err(McpError::Transport)?;
        let response_json = self
            .transport
            .receive()
            .await
            .map_err(McpError::Transport)?;
        serde_json::from_str(&response_json).map_err(|e| McpError::ParseFailed(e.to_string()))
    }

    async fn send_notification(&mut self, request: McpJsonRpcRequest) -> Result<(), McpError> {
        let request_json =
            serde_json::to_string(&request).map_err(|e| McpError::ParseFailed(e.to_string()))?;

        self.transport
            .send(&request_json)
            .await
            .map_err(McpError::Transport)?;
        Ok(())
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>, McpError> {
        let request = McpJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(self.request_id.fetch_add(1, Ordering::Relaxed)),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = self.send_request(request).await?;

        if let Some(result) = response.result {
            #[derive(Deserialize)]
            struct ToolsResponse {
                tools: Vec<McpTool>,
            }

            let tools_resp: ToolsResponse =
                serde_json::from_value(result).map_err(|e| McpError::ParseFailed(e.to_string()))?;
            Ok(tools_resp.tools)
        } else {
            Err(McpError::NoResponse)
        }
    }

    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpToolResult, McpError> {
        let request = McpJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(self.request_id.fetch_add(1, Ordering::Relaxed)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": name,
                "arguments": arguments
            })),
        };

        let response = self.send_request(request).await?;

        if let Some(result) = response.result {
            let tool_result: McpToolResult =
                serde_json::from_value(result).map_err(|e| McpError::ParseFailed(e.to_string()))?;
            Ok(tool_result)
        } else {
            Err(McpError::NoResponse)
        }
    }

    pub async fn list_resources(&mut self) -> Result<Vec<McpResource>, McpError> {
        let request = McpJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(self.request_id.fetch_add(1, Ordering::Relaxed)),
            method: "resources/list".to_string(),
            params: None,
        };

        let response = self.send_request(request).await?;

        if let Some(result) = response.result {
            #[derive(Deserialize)]
            struct ResourcesResponse {
                resources: Vec<McpResource>,
            }

            let resources_resp: ResourcesResponse =
                serde_json::from_value(result).map_err(|e| McpError::ParseFailed(e.to_string()))?;
            Ok(resources_resp.resources)
        } else {
            Err(McpError::NoResponse)
        }
    }

    pub async fn read_resource(&mut self, uri: &str) -> Result<McpResourceContent, McpError> {
        let request = McpJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(self.request_id.fetch_add(1, Ordering::Relaxed)),
            method: "resources/read".to_string(),
            params: Some(serde_json::json!({
                "uri": uri
            })),
        };

        let response = self.send_request(request).await?;

        if let Some(result) = response.result {
            let content: McpResourceContent =
                serde_json::from_value(result).map_err(|e| McpError::ParseFailed(e.to_string()))?;
            Ok(content)
        } else {
            Err(McpError::NoResponse)
        }
    }

    pub async fn disconnect(&mut self) -> Result<(), McpError> {
        self.transport.close().await.map_err(McpError::Transport)?;
        *self.initialized.write().await = false;
        Ok(())
    }

    pub async fn is_initialized(&self) -> bool {
        *self.initialized.read().await
    }

    pub async fn server_info(&self) -> Option<McpServerInfo> {
        self.server_info.read().await.clone()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("Parse failed: {0}")]
    ParseFailed(String),

    #[error("RPC error: {0} - {1}")]
    RpcError(i32, String),

    #[error("No response received")]
    NoResponse,

    #[error("Request timeout")]
    Timeout,

    #[error("Not initialized")]
    NotInitialized,
}
