//! MCP Protocol types

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { resource: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceContent {
    pub uri: String,
    pub mime_type: String,
    pub content: McpContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInitializeRequest {
    pub protocol_version: String,
    pub capabilities: McpCapabilities,
    pub client_info: McpClientInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilities {
    pub tools: Option<serde_json::Value>,
    pub resources: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInitializeResult {
    pub protocol_version: String,
    pub capabilities: McpCapabilities,
    pub server_info: McpServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpJsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpJsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<McpJsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpJsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_tool_serialization_roundtrip() {
        let tool = McpTool {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                }
            }),
        };

        let json = serde_json::to_string(&tool).unwrap();
        let deserialized: McpTool = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "test_tool");
        assert_eq!(deserialized.description, Some("A test tool".to_string()));
    }

    #[test]
    fn mcp_resource_serialization_roundtrip() {
        let resource = McpResource {
            uri: "file:///test.txt".to_string(),
            name: Some("test.txt".to_string()),
            description: Some("A test file".to_string()),
            mime_type: Some("text/plain".to_string()),
        };

        let json = serde_json::to_string(&resource).unwrap();
        let deserialized: McpResource = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.uri, "file:///test.txt");
        assert_eq!(deserialized.name, Some("test.txt".to_string()));
        assert_eq!(deserialized.mime_type, Some("text/plain".to_string()));
    }

    #[test]
    fn mcp_content_text_variant() {
        let content = McpContent::Text {
            text: "Hello".to_string(),
        };
        let json = serde_json::to_string(&content).unwrap();

        assert!(json.contains("\"type\":\"Text\""));
        assert!(json.contains("\"text\":\"Hello\""));

        let deserialized: McpContent = serde_json::from_str(&json).unwrap();
        match deserialized {
            McpContent::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn mcp_content_image_variant() {
        let content = McpContent::Image {
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        };
        let json = serde_json::to_string(&content).unwrap();

        assert!(json.contains("\"type\":\"Image\""));

        let deserialized: McpContent = serde_json::from_str(&json).unwrap();
        match deserialized {
            McpContent::Image { data, mime_type } => {
                assert_eq!(data, "base64data");
                assert_eq!(mime_type, "image/png");
            }
            _ => panic!("Expected Image variant"),
        }
    }

    #[test]
    fn mcp_content_resource_variant() {
        let content = McpContent::Resource {
            resource: "resource-data".to_string(),
        };
        let json = serde_json::to_string(&content).unwrap();

        assert!(json.contains("\"type\":\"Resource\""));

        let deserialized: McpContent = serde_json::from_str(&json).unwrap();
        match deserialized {
            McpContent::Resource { resource } => assert_eq!(resource, "resource-data"),
            _ => panic!("Expected Resource variant"),
        }
    }

    #[test]
    fn mcp_tool_result_serialization() {
        let result = McpToolResult {
            content: vec![
                McpContent::Text {
                    text: "Result 1".to_string(),
                },
                McpContent::Text {
                    text: "Result 2".to_string(),
                },
            ],
            is_error: Some(false),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: McpToolResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.content.len(), 2);
        assert_eq!(deserialized.is_error, Some(false));
    }

    #[test]
    fn mcp_initialize_request_serialization() {
        let request = McpInitializeRequest {
            protocol_version: "1.0".to_string(),
            capabilities: McpCapabilities {
                tools: Some(serde_json::json!({})),
                resources: None,
            },
            client_info: McpClientInfo {
                name: "TestClient".to_string(),
                version: "1.0.0".to_string(),
            },
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: McpInitializeRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.protocol_version, "1.0");
        assert_eq!(deserialized.client_info.name, "TestClient");
        assert_eq!(deserialized.client_info.version, "1.0.0");
    }

    #[test]
    fn mcp_initialize_result_serialization() {
        let result = McpInitializeResult {
            protocol_version: "1.0".to_string(),
            capabilities: McpCapabilities {
                tools: None,
                resources: Some(serde_json::json!({})),
            },
            server_info: McpServerInfo {
                name: "TestServer".to_string(),
                version: "2.0.0".to_string(),
            },
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: McpInitializeResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.protocol_version, "1.0");
        assert_eq!(deserialized.server_info.name, "TestServer");
        assert_eq!(deserialized.server_info.version, "2.0.0");
    }

    #[test]
    fn mcp_json_rpc_request_serialization() {
        let request = McpJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(123),
            method: "tools/list".to_string(),
            params: Some(serde_json::json!({"limit": 10})),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: McpJsonRpcRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.jsonrpc, "2.0");
        assert_eq!(deserialized.id, serde_json::json!(123));
        assert_eq!(deserialized.method, "tools/list");
        assert!(deserialized.params.is_some());
    }

    #[test]
    fn mcp_json_rpc_response_success() {
        let response = McpJsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!("req-1"),
            result: Some(serde_json::json!({"tools": []})),
            error: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: McpJsonRpcResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.jsonrpc, "2.0");
        assert_eq!(deserialized.id, serde_json::json!("req-1"));
        assert!(deserialized.result.is_some());
        assert!(deserialized.error.is_none());
    }

    #[test]
    fn mcp_json_rpc_response_error() {
        let response = McpJsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(456),
            result: None,
            error: Some(McpJsonRpcError {
                code: -32600,
                message: "Invalid request".to_string(),
                data: Some(serde_json::json!("additional info")),
            }),
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: McpJsonRpcResponse = serde_json::from_str(&json).unwrap();

        assert!(deserialized.result.is_none());
        assert!(deserialized.error.is_some());

        let error = deserialized.error.unwrap();
        assert_eq!(error.code, -32600);
        assert_eq!(error.message, "Invalid request");
        assert!(error.data.is_some());
    }

    #[test]
    fn mcp_capabilities_defaults() {
        let caps = McpCapabilities {
            tools: None,
            resources: None,
        };

        assert!(caps.tools.is_none());
        assert!(caps.resources.is_none());
    }

    #[test]
    fn mcp_resource_content_serialization() {
        let content = McpResourceContent {
            uri: "file:///doc.txt".to_string(),
            mime_type: "text/plain".to_string(),
            content: McpContent::Text {
                text: "Document content".to_string(),
            },
        };

        let json = serde_json::to_string(&content).unwrap();
        let deserialized: McpResourceContent = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.uri, "file:///doc.txt");
        assert_eq!(deserialized.mime_type, "text/plain");
    }
}
